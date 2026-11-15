//! 性能の計測: PTY から送り先まで(`arin_lib::sessions::SessionRegistry`)。T24。NFR-004、設計書 §2.2「流量制御」、§7.3。
//! 記録するだけで合否にしない。
//!
//! 1. 処理済みをすぐ返す送り先で、10 万行の `cat` の所要時間、送った回数、1 回あたりの大きさ、未処理の量の最大値
//! 2. 処理済みを返さない送り先で、未処理の量が上限の付近で頭打ちになり、読み取りが止まること。
//!    そのあと処理済みを返すと、読み取りが再開して最後まで届くこと
//!
//! 普段の `cargo test` では動かさない(`#[ignore]`)。`npm run perf`(`scripts/perf/run.mjs`)が
//! `--release -- --ignored --nocapture` で動かし、`ARIN_PERF ` で始まる行(1 行 1 つの JSON)を読み取る。
//! Tauri のアプリは作らないので標準のハーネスで走らせる(`session_pty` と同じ)。
//! 書き方は `session_pty.rs` と `crates/arin-platform/tests/perf_pty.rs` にならう。
//!
//! M4(計画書 T49b。NFR-009、ADR-001 の「複数枚のときのしきい値は M4 で実測して見直す」)で足したもの:
//!
//! 3. 1 枚と 9 枚で同時に `yes | head -c` を流したときの、送り先までの時間と量、未処理の量の最大値。
//!    ベル(BEL)を含む行を流す組も測り、アテンションの走査(T48c。読み取りスレッドが常に走査し、契機のある塊のあとに
//!    Workspace のロックを取る)が入った状態での速さを比べられるようにする
//! 4. 9 枚で処理済みを返さないとき、各セッションの未処理の量が上限の付近で頭打ちになり、読み取りが止まること
//! 5. アテンションの走査器だけの速さ(メモリ上の 10 万行。PTY を使わない)
//!
//! 流す量は 1 枚あたり `MULTI_BYTES`(4 MiB)に限る(長く流しっぱなしにしない。テストのプロセスのメモリを見るため)。

use std::ffi::OsString;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use arin_core::terminal::{AttentionScanner, TerminalId};
use arin_core::workspace::Workspace;
use arin_lib::sessions::{OutputSink, SessionConfig, SessionRegistry, ShellDefaults};
use arin_platform::pty::NativePty;

const WAIT: Duration = Duration::from_secs(60);
const ZSH: &str = "/bin/zsh";
/// 流す行数(NFR-004)。
const LINES: usize = 100_000;
/// 計測の回数。中央値と最小・最大は `scripts/perf/run.mjs` が出す。
const RUNS: usize = 3;
/// 処理済みを返さない送り先で、未処理の量が変わらなくなったとみなす時間。
const SETTLE: Duration = Duration::from_millis(1_000);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// 計測だけに使う一時フォルダ(`HOME`、cwd、計測用のファイル)。抜けるときに自分が作ったフォルダだけを消す。
struct TempHome(PathBuf);

impl TempHome {
    fn new(name: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("時計が 1970 年より前")
            .as_nanos();
        let base = std::env::temp_dir();
        let dir = base.join(format!(
            "arin-perf-sessions-{name}-{}-{nanos}",
            std::process::id()
        ));
        assert!(dir.starts_with(&base), "一時領域の外を使おうとした");
        fs::create_dir(&dir).expect("一時フォルダを作れなかった");
        fs::write(dir.join(".zshrc"), "").expect(".zshrc を作れなかった");
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempHome {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// 10 万行のファイルを書き、その大きさ(バイト)を返す。`perf_pty.rs` と同じ中身。
fn write_lines(path: &Path) -> u64 {
    let mut text = String::with_capacity(LINES * 80);
    for i in 1..=LINES {
        let _ = writeln!(
            text,
            "{i:06} the quick brown fox jumps over the lazy dog 0123456789 abcdefghijklmn"
        );
    }
    fs::write(path, &text).expect("計測用のファイルを書けなかった");
    u64::try_from(text.len()).expect("大きさが u64 に収まらない")
}

const MARK_HEAD: &[u8] = b"@@ARIN:";
const MARK_TAIL: &[u8] = b"@@";
const MARK_NAME_MAX: usize = 32;

/// 目印を出させる入力。エコーには `@@AR''IN:` と出るので取り違えない。
fn mark_command(name: &str) -> String {
    format!("printf '@@AR''IN:%s@@\\n' {name}")
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// 流れてくる出力から目印を探す。全体を貯めずに、末尾の少しだけを持つ(`perf_pty.rs` と同じ)。
#[derive(Default)]
struct MarkScanner {
    window: Vec<u8>,
}

impl MarkScanner {
    fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        self.window.extend_from_slice(chunk);
        let mut found = Vec::new();
        while let Some(head) = find(&self.window, MARK_HEAD) {
            let name_start = head + MARK_HEAD.len();
            let Some(tail) = find(&self.window[name_start..], MARK_TAIL) else {
                break;
            };
            if tail > MARK_NAME_MAX {
                self.window.drain(..=head);
                continue;
            }
            let end = name_start + tail + MARK_TAIL.len();
            found.push(
                String::from_utf8_lossy(&self.window[name_start..name_start + tail]).into_owned(),
            );
            self.window.drain(..end);
        }
        let keep = MARK_HEAD.len() + MARK_NAME_MAX + MARK_TAIL.len();
        if self.window.len() > keep {
            let n = self.window.len() - keep;
            self.window.drain(..n);
        }
        found
    }
}

/// 送り先が受け取ったものの集計。受け取った中身は貯めない(目印を探すだけ)。
#[derive(Default)]
struct Stats {
    sends: u64,
    bytes: u64,
    max_send: u64,
    /// 送られてきたときの未処理の量の最大値(処理済みを返す直前)。
    max_unacked: u64,
    /// 目印の名前と、届いたときの時刻・それまでの送った回数・量。
    marks: Vec<(String, Instant, u64, u64)>,
    scanner: MarkScanner,
}

/// 偽の送り先。`acking` が真なら、受け取ったらすぐ処理済みを返す。
struct Sink {
    stats: Mutex<Stats>,
    acking: AtomicBool,
    target: OnceLock<(SessionRegistry, TerminalId)>,
}

impl Sink {
    fn new(acking: bool) -> Arc<Self> {
        Arc::new(Self {
            stats: Mutex::new(Stats::default()),
            acking: AtomicBool::new(acking),
            target: OnceLock::new(),
        })
    }

    /// 名前 `name` の目印が届くまで待ち、届いたときの時刻・送った回数・量を返す。
    fn wait_mark(&self, name: &str) -> (Instant, u64, u64) {
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some((_, at, sends, bytes)) =
                lock(&self.stats).marks.iter().find(|m| m.0 == name)
            {
                return (*at, *sends, *bytes);
            }
            assert!(
                Instant::now() < deadline,
                "目印 {name} が {WAIT:?} 以内に届かなかった"
            );
            thread::sleep(Duration::from_millis(1));
        }
    }
}

impl OutputSink for Sink {
    fn send(&self, data: Vec<u8>) -> Result<(), String> {
        let now = Instant::now();
        let n = data.len() as u64;
        let target = self.target.get();
        let unacked = target
            .and_then(|(r, id)| r.backlog(id))
            .map_or(0, |b| b.unacked);
        {
            let mut guard = lock(&self.stats);
            let s = &mut *guard;
            s.sends += 1;
            s.bytes += n;
            s.max_send = s.max_send.max(n);
            s.max_unacked = s.max_unacked.max(unacked);
            let (sends, bytes) = (s.sends, s.bytes);
            for name in s.scanner.push(&data) {
                s.marks.push((name, now, sends, bytes));
            }
        }
        if self.acking.load(Ordering::SeqCst)
            && let Some((registry, id)) = target
        {
            let _ = registry.ack(id, n);
        }
        Ok(())
    }
}

/// 一時の `HOME` で `/bin/zsh` を 1 枚起動し、送り先 `sink` につなぐ。
fn start(home: &TempHome, sink: &Arc<Sink>) -> (SessionRegistry, TerminalId) {
    let mut vars: Vec<(OsString, OsString)> = ["USER", "LOGNAME", "TMPDIR"]
        .into_iter()
        .filter_map(|n| std::env::var_os(n).map(|v| (n.into(), v)))
        .collect();
    vars.push(("HOME".into(), home.path().as_os_str().to_owned()));
    // 9 枚の検証画面の形で作り、1 枚目だけを使う(起動しないターミナルは計測に影響しない)。
    let workspace = Workspace::for_verify_grid("0.0.0-perf", home.path().to_path_buf(), vars);
    // しきい値とまとめは製品の既定値のまま測る。
    let registry = SessionRegistry::new(
        workspace,
        Arc::new(NativePty),
        Arc::new(|| ShellDefaults {
            shell: PathBuf::from(ZSH),
            locale: Some("ja_JP".to_owned()),
        }),
        Arc::new(|_| {}),
        SessionConfig::default(),
    );
    let id = registry.snapshot().terminals[0].id.clone();
    let _ = sink.target.set((registry.clone(), id.clone()));
    registry
        .connect(&id, 24, 80, Arc::clone(sink) as Arc<dyn OutputSink>)
        .expect("シェルを起動できなかった");
    (registry, id)
}

/// このテストのプロセスの常駐メモリ(KiB。`ps -o rss=`)。取れなければ 0。
fn rss_kib() -> u64 {
    Command::new("/bin/ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

fn config_json(config: &SessionConfig) -> String {
    format!(
        "\"high_water\":{},\"low_water\":{},\"batch_max_bytes\":{},\"batch_window_ms\":{},\"read_buffer\":{}",
        config.high_water,
        config.low_water,
        config.batch_max_bytes,
        config.batch_window.as_millis(),
        config.read_buffer
    )
}

#[test]
#[ignore = "性能の計測。npm run perf が --ignored で動かす"]
fn 処理済みをすぐ返す送り先で_10_万行を_cat_したときの所要時間() {
    let home = TempHome::new("ack");
    let file_bytes = write_lines(&home.path().join("lines.txt"));
    let sink = Sink::new(true);
    let (registry, id) = start(&home, &sink);
    registry
        .write(&id, format!("{}\r", mark_command("ready")).as_bytes())
        .unwrap();
    sink.wait_mark("ready");

    for run in 1..=RUNS {
        let begin = format!("begin{run}");
        let end = format!("end{run}");
        lock(&sink.stats).max_unacked = 0;
        let rss_before = rss_kib();
        registry
            .write(
                &id,
                format!(
                    "{}; cat lines.txt; {}\r",
                    mark_command(&begin),
                    mark_command(&end)
                )
                .as_bytes(),
            )
            .unwrap();
        let (b_at, b_sends, b_bytes) = sink.wait_mark(&begin);
        let (e_at, e_sends, e_bytes) = sink.wait_mark(&end);
        let secs = e_at.duration_since(b_at).as_secs_f64();
        let sends = e_sends - b_sends;
        let bytes = e_bytes - b_bytes;
        let (max_send, max_unacked) = {
            let s = lock(&sink.stats);
            (s.max_send, s.max_unacked)
        };
        println!(
            "ARIN_PERF {{\"kind\":\"session_ack\",\"run\":{run},\"lines\":{LINES},\"file_bytes\":{file_bytes},\
             \"bytes\":{bytes},\"sends\":{sends},\"avg_send_bytes\":{:.0},\"max_send_bytes\":{max_send},\
             \"max_unacked\":{max_unacked},\"first_to_last_ms\":{:.3},\"mib_per_s\":{:.3},\
             \"rss_before_kib\":{rss_before},\"rss_after_kib\":{},{}}}",
            bytes as f64 / sends.max(1) as f64,
            secs * 1000.0,
            bytes as f64 / secs / (1024.0 * 1024.0),
            rss_kib(),
            config_json(&SessionConfig::default()),
        );
    }
    registry.shutdown();
}

#[test]
#[ignore = "性能の計測。npm run perf が --ignored で動かす"]
fn 処理済みを返さない送り先では未処理の量が上限の付近で頭打ちになり読み取りが止まる() {
    let home = TempHome::new("stall");
    let file_bytes = write_lines(&home.path().join("lines.txt"));

    for run in 1..=RUNS {
        let sink = Sink::new(true);
        let (registry, id) = start(&home, &sink);
        registry
            .write(&id, format!("{}\r", mark_command("ready")).as_bytes())
            .unwrap();
        sink.wait_mark("ready");
        // ここから処理済みを返さない。
        sink.acking.store(false, Ordering::SeqCst);
        registry.ack(&id, u64::MAX).unwrap();
        let rss_before = rss_kib();
        let received_before = lock(&sink.stats).bytes;
        let started = Instant::now();
        registry
            .write(
                &id,
                format!("cat lines.txt; {}\r", mark_command("end")).as_bytes(),
            )
            .unwrap();

        // 止まって、未処理の量と受け取った量が SETTLE のあいだ変わらなくなるまで待つ。
        let deadline = started + WAIT;
        let mut last = (u64::MAX, u64::MAX);
        let mut stable_since = Instant::now();
        let mut paused_at = None;
        let (unacked, received) = loop {
            let backlog = registry.backlog(&id).expect("セッションが無い");
            let received = lock(&sink.stats).bytes - received_before;
            if backlog.paused && paused_at.is_none() {
                paused_at = Some(started.elapsed());
            }
            let now = (backlog.unacked, received);
            if now != last {
                last = now;
                stable_since = Instant::now();
            } else if backlog.paused && stable_since.elapsed() >= SETTLE {
                break now;
            }
            assert!(
                Instant::now() < deadline,
                "未処理の量が頭打ちにならなかった: {backlog:?}"
            );
            thread::sleep(Duration::from_millis(5));
        };
        let rss_stalled = rss_kib();
        let ended_while_stalled = lock(&sink.stats).marks.iter().any(|m| m.0 == "end");

        // 処理済みを返すと再開し、最後まで届く。
        sink.acking.store(true, Ordering::SeqCst);
        let resumed_at = Instant::now();
        registry.ack(&id, u64::MAX).unwrap();
        let (e_at, _, total) = sink.wait_mark("end");
        let total = total - received_before;
        println!(
            "ARIN_PERF {{\"kind\":\"session_stall\",\"run\":{run},\"lines\":{LINES},\"file_bytes\":{file_bytes},\
             \"stalled_unacked\":{unacked},\"stalled_received\":{received},\"paused\":true,\
             \"paused_after_ms\":{:.3},\"settle_ms\":{},\"ended_while_stalled\":{ended_while_stalled},\
             \"resumed_to_end_ms\":{:.3},\"total_received\":{total},\
             \"rss_before_kib\":{rss_before},\"rss_stalled_kib\":{rss_stalled},{}}}",
            paused_at.unwrap_or_default().as_secs_f64() * 1000.0,
            SETTLE.as_millis(),
            e_at.duration_since(resumed_at).as_secs_f64() * 1000.0,
            config_json(&SessionConfig::default()),
        );
        registry.shutdown();
    }
}

// ---------------------------------------------------------------------------
// M4: 9 枚で同時に流す(T49b)
// ---------------------------------------------------------------------------

/// 9 枚の計測で 1 枚あたりに流す量(バイト。`head -c` に渡す。PTY が改行に CR を足すので、届く量は少し多い)。
const MULTI_BYTES: u64 = 4 * 1024 * 1024;
/// 9 枚の計測で流す行(`yes` の引数)。78 文字。
const MULTI_LINE: &str =
    "arin perf tiles 0123456789 abcdefghijklmnopqrstuvwxyz ABCDEFGHIJKLMNOPQRSTUVWXYZ";
/// 同時に流す枚数(NFR-009 の 9 枚。`Workspace::for_verify_grid` の枚数と同じ)。
const MULTI_TERMINALS: usize = 9;

/// 一時の `HOME` で `/bin/zsh` を `count` 枚起動し、1 枚ごとに送り先をつなぐ。返す並びは写しの順。
fn start_many(
    home: &TempHome,
    count: usize,
    acking: bool,
) -> (SessionRegistry, Vec<(TerminalId, Arc<Sink>)>) {
    let mut vars: Vec<(OsString, OsString)> = ["USER", "LOGNAME", "TMPDIR"]
        .into_iter()
        .filter_map(|n| std::env::var_os(n).map(|v| (n.into(), v)))
        .collect();
    vars.push(("HOME".into(), home.path().as_os_str().to_owned()));
    // 9 枚の検証画面の形で作る(持ち主の無い 9 枚。T49d で足場を置き換える)。使わない分は起動しない。
    let workspace = Workspace::for_verify_grid("0.0.0-perf", home.path().to_path_buf(), vars);
    let registry = SessionRegistry::new(
        workspace,
        Arc::new(NativePty),
        Arc::new(|| ShellDefaults {
            shell: PathBuf::from(ZSH),
            locale: Some("ja_JP".to_owned()),
        }),
        Arc::new(|_| {}),
        SessionConfig::default(),
    );
    let ids: Vec<TerminalId> = registry
        .snapshot()
        .terminals
        .iter()
        .map(|t| t.id.clone())
        .collect();
    assert!(
        ids.len() >= count,
        "ターミナルが {count} 枚無い({} 枚)",
        ids.len()
    );
    let mut out = Vec::with_capacity(count);
    for id in ids.into_iter().take(count) {
        let sink = Sink::new(acking);
        let _ = sink.target.set((registry.clone(), id.clone()));
        registry
            .connect(&id, 24, 80, Arc::clone(&sink) as Arc<dyn OutputSink>)
            .expect("シェルを起動できなかった");
        out.push((id, sink));
    }
    // どのシェルも入力を受け付けるまで待つ。
    for (id, sink) in &out {
        registry
            .write(id, format!("{}\r", mark_command("ready")).as_bytes())
            .unwrap();
        sink.wait_mark("ready");
    }
    (registry, out)
}

/// `yes` の行を `bytes` バイトだけ流す入力。`bel` なら行の終わりにベルを入れる(アテンションの契機が毎回ある)。
fn flood_command(bytes: u64, bel: bool) -> String {
    let line = if bel {
        format!("$'{MULTI_LINE}\\a'")
    } else {
        format!("'{MULTI_LINE}'")
    };
    format!("yes {line} | head -c {bytes}")
}

/// 1 回分の、`count` 枚で同時に流した結果を `ARIN_PERF` の行で出す。
fn run_multi_ack(home: &TempHome, count: usize, bel: bool, run: usize) {
    let (registry, terms) = start_many(home, count, true);
    for (_, sink) in &terms {
        lock(&sink.stats).max_unacked = 0;
    }
    let rss_before = rss_kib();
    let begin = format!("begin{run}");
    let end = format!("end{run}");
    let command = format!(
        "{}; {}; {}\r",
        mark_command(&begin),
        flood_command(MULTI_BYTES, bel),
        mark_command(&end)
    );
    // できるだけ同時に流し始める(書き込みは順に送るだけで、待たない)。
    for (id, _) in &terms {
        registry.write(id, command.as_bytes()).unwrap();
    }
    let mut first_begin: Option<Instant> = None;
    let mut last_end: Option<Instant> = None;
    let mut per_terminal_ms = Vec::with_capacity(count);
    let (mut sends, mut bytes, mut max_send, mut max_unacked) = (0u64, 0u64, 0u64, 0u64);
    for (_, sink) in &terms {
        let (b_at, b_sends, b_bytes) = sink.wait_mark(&begin);
        let (e_at, e_sends, e_bytes) = sink.wait_mark(&end);
        first_begin = Some(first_begin.map_or(b_at, |t| t.min(b_at)));
        last_end = Some(last_end.map_or(e_at, |t| t.max(e_at)));
        per_terminal_ms.push(e_at.duration_since(b_at).as_secs_f64() * 1000.0);
        sends += e_sends - b_sends;
        bytes += e_bytes - b_bytes;
        let s = lock(&sink.stats);
        max_send = max_send.max(s.max_send);
        max_unacked = max_unacked.max(s.max_unacked);
    }
    let rss_after = rss_kib();
    registry.shutdown();
    let wall = last_end
        .zip(first_begin)
        .map_or(0.0, |(e, b)| e.duration_since(b).as_secs_f64());
    per_terminal_ms.sort_by(f64::total_cmp);
    let slowest = per_terminal_ms.last().copied().unwrap_or_default();
    let fastest = per_terminal_ms.first().copied().unwrap_or_default();
    println!(
        "ARIN_PERF {{\"kind\":\"multi_ack\",\"run\":{run},\"terminals\":{count},\"bel\":{bel},\
         \"bytes_per_terminal\":{MULTI_BYTES},\"bytes\":{bytes},\"sends\":{sends},\"avg_send_bytes\":{:.0},\
         \"max_send_bytes\":{max_send},\"max_unacked\":{max_unacked},\"wall_ms\":{:.3},\
         \"slowest_ms\":{slowest:.3},\"fastest_ms\":{fastest:.3},\"mib_per_s_total\":{:.3},\
         \"rss_before_kib\":{rss_before},\"rss_after_kib\":{rss_after},{}}}",
        bytes as f64 / sends.max(1) as f64,
        wall * 1000.0,
        bytes as f64 / wall.max(f64::MIN_POSITIVE) / (1024.0 * 1024.0),
        config_json(&SessionConfig::default()),
    );
}

#[test]
#[ignore = "性能の計測。npm run perf が --ignored で動かす"]
fn 一枚と九枚で同時に_yes_を流したときの送り先までの時間と量() {
    let home = TempHome::new("multi");
    for run in 1..=RUNS {
        run_multi_ack(&home, 1, false, run);
        run_multi_ack(&home, MULTI_TERMINALS, false, run);
    }
}

#[test]
#[ignore = "性能の計測。npm run perf が --ignored で動かす"]
fn ベルを含む行を一枚と九枚で流したときの送り先までの時間と量() {
    let home = TempHome::new("multi-bel");
    for run in 1..=RUNS {
        run_multi_ack(&home, 1, true, run);
        run_multi_ack(&home, MULTI_TERMINALS, true, run);
    }
}

#[test]
#[ignore = "性能の計測。npm run perf が --ignored で動かす"]
fn 九枚で処理済みを返さないと各セッションの未処理の量が上限の付近で頭打ちになる() {
    let home = TempHome::new("multi-stall");
    for run in 1..=RUNS {
        let (registry, terms) = start_many(&home, MULTI_TERMINALS, true);
        // ここから処理済みを返さない。それまでの分は返しておく。
        for (id, sink) in &terms {
            sink.acking.store(false, Ordering::SeqCst);
            registry.ack(id, u64::MAX).unwrap();
        }
        let received_before: Vec<u64> = terms.iter().map(|(_, s)| lock(&s.stats).bytes).collect();
        let rss_before = rss_kib();
        let started = Instant::now();
        let command = format!(
            "{}; {}\r",
            flood_command(2 * MULTI_BYTES, false),
            mark_command("end")
        );
        for (id, _) in &terms {
            registry.write(id, command.as_bytes()).unwrap();
        }

        // 全セッションが止まり、未処理の量と受け取った量が SETTLE のあいだ変わらなくなるまで待つ。
        let deadline = started + WAIT;
        let mut last: Vec<(u64, u64)> = Vec::new();
        let mut stable_since = Instant::now();
        let mut paused_at: Option<Duration> = None;
        let now = loop {
            let backlogs: Vec<_> = terms
                .iter()
                .map(|(id, _)| registry.backlog(id).expect("セッションが無い"))
                .collect();
            let all_paused = backlogs.iter().all(|b| b.paused);
            if all_paused && paused_at.is_none() {
                paused_at = Some(started.elapsed());
            }
            let now: Vec<(u64, u64)> = backlogs
                .iter()
                .zip(&terms)
                .zip(&received_before)
                .map(|((b, (_, s)), before)| (b.unacked, lock(&s.stats).bytes - before))
                .collect();
            if now != last {
                last = now;
                stable_since = Instant::now();
            } else if all_paused && stable_since.elapsed() >= SETTLE {
                break now;
            }
            assert!(
                Instant::now() < deadline,
                "未処理の量が頭打ちにならなかった: {backlogs:?}"
            );
            thread::sleep(Duration::from_millis(5));
        };
        let rss_stalled = rss_kib();
        let ended_while_stalled = terms
            .iter()
            .filter(|(_, s)| lock(&s.stats).marks.iter().any(|m| m.0 == "end"))
            .count();
        let unacked: Vec<u64> = now.iter().map(|(u, _)| *u).collect();
        let received: u64 = now.iter().map(|(_, r)| *r).sum();

        // 処理済みを返すと再開し、最後まで届く。
        let resumed_at = Instant::now();
        for (id, sink) in &terms {
            sink.acking.store(true, Ordering::SeqCst);
            registry.ack(id, u64::MAX).unwrap();
        }
        let mut last_end = resumed_at;
        for (_, sink) in &terms {
            let (e_at, _, _) = sink.wait_mark("end");
            last_end = last_end.max(e_at);
        }
        registry.shutdown();
        println!(
            "ARIN_PERF {{\"kind\":\"multi_stall\",\"run\":{run},\"terminals\":{},\
             \"bytes_per_terminal\":{},\"stalled_unacked_max\":{},\"stalled_unacked_min\":{},\
             \"stalled_unacked_sum\":{},\"stalled_received_sum\":{received},\
             \"paused_after_ms\":{:.3},\"settle_ms\":{},\"ended_while_stalled\":{ended_while_stalled},\
             \"resumed_to_end_ms\":{:.3},\"rss_before_kib\":{rss_before},\"rss_stalled_kib\":{rss_stalled},{}}}",
            terms.len(),
            2 * MULTI_BYTES,
            unacked.iter().max().copied().unwrap_or_default(),
            unacked.iter().min().copied().unwrap_or_default(),
            unacked.iter().sum::<u64>(),
            paused_at.unwrap_or_default().as_secs_f64() * 1000.0,
            SETTLE.as_millis(),
            last_end.duration_since(resumed_at).as_secs_f64() * 1000.0,
            config_json(&SessionConfig::default()),
        );
    }
}

#[test]
#[ignore = "性能の計測。npm run perf が --ignored で動かす"]
fn アテンションの走査器だけの速さ() {
    // 10 万行(約 7.7 MB)をメモリに置き、読み取りの大きさ(16 KiB)ずつ走査する。PTY は使わない。
    let mut text = String::with_capacity(LINES * 80);
    for i in 1..=LINES {
        let _ = writeln!(
            text,
            "{i:06} the quick brown fox jumps over the lazy dog 0123456789 abcdefghijklmn"
        );
    }
    let data = text.into_bytes();
    let chunk = SessionConfig::default().read_buffer;
    for run in 1..=RUNS {
        let mut scanner = AttentionScanner::new();
        let mut hits = 0u64;
        let started = Instant::now();
        for piece in data.chunks(chunk) {
            if scanner.scan(std::hint::black_box(piece)) {
                hits += 1;
            }
        }
        let secs = started.elapsed().as_secs_f64();
        println!(
            "ARIN_PERF {{\"kind\":\"attention_scan\",\"run\":{run},\"bytes\":{},\"chunk\":{chunk},\
             \"hits\":{hits},\"ms\":{:.3},\"mib_per_s\":{:.1}}}",
            data.len(),
            secs * 1000.0,
            data.len() as f64 / secs.max(f64::MIN_POSITIVE) / (1024.0 * 1024.0),
        );
    }
}
