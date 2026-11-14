//! 性能の計測: 実 PTY の読み取りの速さ(T24。NFR-004、設計書 §2.2「流量制御」、§7.3)。記録するだけで合否にしない。
//!
//! 10 万行のファイルを `cat` し、出力の最初のバイトから最後のバイトまでの所要時間とスループットを測る。
//! 普段の `cargo test` では動かさない(`#[ignore]`)。`npm run perf`(`scripts/perf/run.mjs`)が
//! `--release -- --ignored --nocapture` で動かし、`ARIN_PERF ` で始まる行(1 行 1 つの JSON)を読み取る。
//!
//! 書き方は `tests/pty.rs` にならう。
//! - シェルは `/bin/zsh` に固定し、`HOME` を一時フォルダに向けて利用者の設定ファイルを読ませない
//! - 出力は目印で区切る。目印は `printf` が組み立てるので、入力のエコーにはそのままの形では現れない
//! - どの待ちにも上限時間を付ける。計測用のファイルは一時フォルダに作り、抜けるときに消す

use std::ffi::OsString;
use std::fmt::Write as _;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use arin_core::ports::{PtyPort, PtySession, PtySize};
use arin_core::terminal::ShellLaunchSpec;
use arin_platform::pty::NativePty;

/// 1 回の待ちの上限。
const WAIT: Duration = Duration::from_secs(60);
const ZSH: &str = "/bin/zsh";
const SIZE: PtySize = PtySize { rows: 24, cols: 80 };
/// 流す行数(NFR-004)。
const LINES: usize = 100_000;
/// 計測の回数。中央値と最小・最大は `scripts/perf/run.mjs` が出す。
const RUNS: usize = 3;
/// 1 回の読み取りの大きさ。`src-tauri/src/sessions.rs` の `SessionConfig::default().read_buffer` と同じ。
const READ_BUFFER: usize = 16 * 1024;

/// 計測だけに使う一時フォルダ。抜けるときに自分が作ったフォルダだけを消す。
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("時計が 1970 年より前")
            .as_nanos();
        let base = std::env::temp_dir();
        let dir = base.join(format!("arin-perf-{name}-{}-{nanos}", std::process::id()));
        assert!(dir.starts_with(&base), "一時領域の外を使おうとした");
        fs::create_dir(&dir).expect("一時フォルダを作れなかった");
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// 10 万行のファイルを書き、その大きさ(バイト)を返す。1 行はおよそ 80 桁(折り返さない幅)。
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

/// 目印の頭。出力では `@@ARIN:<名前>@@` の形で現れる。
const MARK_HEAD: &[u8] = b"@@ARIN:";
const MARK_TAIL: &[u8] = b"@@";
/// 目印の名前の長さの上限。
const MARK_NAME_MAX: usize = 32;

/// 目印を出させる入力。エコーには `@@AR''IN:` と出るので取り違えない。
fn mark_command(name: &str) -> String {
    format!("printf '@@AR''IN:%s@@\\n' {name}")
}

/// 流れてくる出力から目印を探す。全体を貯めずに、末尾の少しだけを持つ。
struct MarkScanner {
    window: Vec<u8>,
    /// `window[0]` までに受け取った量(バイト)。
    window_start: u64,
}

impl MarkScanner {
    fn new() -> Self {
        Self {
            window: Vec::new(),
            window_start: 0,
        }
    }

    /// 受け取った塊を足し、見つかった目印の名前と、その目印の終わりまでに受け取った量を返す。
    fn push(&mut self, chunk: &[u8]) -> Vec<(String, u64)> {
        self.window.extend_from_slice(chunk);
        let mut found = Vec::new();
        while let Some(head) = find(&self.window, MARK_HEAD) {
            let name_start = head + MARK_HEAD.len();
            let Some(tail) = find(&self.window[name_start..], MARK_TAIL) else {
                break;
            };
            if tail > MARK_NAME_MAX {
                // 目印ではなかった。頭の次から探し直す。
                self.drain(head + 1);
                continue;
            }
            let end = name_start + tail + MARK_TAIL.len();
            let name = String::from_utf8_lossy(&self.window[name_start..name_start + tail]);
            found.push((name.into_owned(), self.window_start + end as u64));
            self.drain(end);
        }
        // 途中まで届いた目印を残せる長さだけ持つ。
        let keep = MARK_HEAD.len() + MARK_NAME_MAX + MARK_TAIL.len();
        if self.window.len() > keep {
            self.drain(self.window.len() - keep);
        }
        found
    }

    fn drain(&mut self, n: usize) {
        self.window.drain(..n);
        self.window_start += n as u64;
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// 読み取りスレッドが知らせる出来事。
struct Mark {
    name: String,
    at: Instant,
    /// 目印の終わりまでに受け取った量(バイト)。
    total: u64,
    /// 目印の終わりまでの読み取りの回数。
    reads: u64,
}

/// シェルを起動し、出力を別スレッドで読みながら目印を知らせる。
struct Shell {
    session: Box<dyn PtySession>,
    marks: Receiver<Mark>,
}

impl Shell {
    fn spawn(spec: &ShellLaunchSpec) -> Self {
        let spawned = NativePty
            .spawn(spec, SIZE)
            .expect("シェルを起動できなかった");
        let (tx, marks) = mpsc::channel();
        let mut reader = spawned.output;
        thread::spawn(move || {
            let mut buf = vec![0u8; READ_BUFFER];
            let mut scanner = MarkScanner::new();
            let mut reads = 0u64;
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        reads += 1;
                        let at = Instant::now();
                        for (name, total) in scanner.push(&buf[..n]) {
                            let mark = Mark {
                                name,
                                at,
                                total,
                                reads,
                            };
                            if tx.send(mark).is_err() {
                                return;
                            }
                        }
                    }
                }
            }
        });
        Self {
            session: spawned.session,
            marks,
        }
    }

    fn send(&mut self, input: &str) {
        self.session
            .write(input.as_bytes())
            .expect("入力を書き込めなかった");
    }

    /// 名前 `name` の目印が届くまで待つ。
    fn wait_mark(&self, name: &str) -> Mark {
        let deadline = Instant::now() + WAIT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let mark = self
                .marks
                .recv_timeout(left)
                .unwrap_or_else(|e| panic!("目印 {name} が {WAIT:?} 以内に届かなかった: {e}"));
            if mark.name == name {
                return mark;
            }
        }
    }
}

fn spec(home: &Path, cwd: &Path) -> ShellLaunchSpec {
    let mut env: Vec<(OsString, OsString)> = ["USER", "LOGNAME", "TMPDIR"]
        .into_iter()
        .filter_map(|n| std::env::var_os(n).map(|v| (n.into(), v)))
        .collect();
    env.push(("HOME".into(), home.as_os_str().to_owned()));
    ShellLaunchSpec::new(Path::new(ZSH), cwd, env, Some("ja_JP"))
        .expect("起動の仕様を組み立てられなかった")
}

#[test]
#[ignore = "性能の計測。npm run perf が --ignored で動かす"]
fn 実_pty_で_10_万行を_cat_したときの読み取りの速さ() {
    let dir = TempDir::new("pty");
    let home = dir.path().join("home");
    fs::create_dir(&home).expect("HOME を作れなかった");
    fs::write(home.join(".zshrc"), "").expect(".zshrc を作れなかった");
    let file = dir.path().join("lines.txt");
    let file_bytes = write_lines(&file);

    let mut shell = Shell::spawn(&spec(&home, dir.path()));
    shell.send(&format!("{}\n", mark_command("ready")));
    shell.wait_mark("ready");

    for run in 1..=RUNS {
        let begin = format!("begin{run}");
        let end = format!("end{run}");
        let sent = Instant::now();
        shell.send(&format!(
            "{}; cat lines.txt; {}\n",
            mark_command(&begin),
            mark_command(&end)
        ));
        let b = shell.wait_mark(&begin);
        let e = shell.wait_mark(&end);
        let secs = e.at.duration_since(b.at).as_secs_f64();
        let bytes = e.total - b.total;
        println!(
            "ARIN_PERF {{\"kind\":\"pty_read\",\"run\":{run},\"lines\":{LINES},\"file_bytes\":{file_bytes},\
             \"bytes\":{bytes},\"reads\":{},\"read_buffer\":{READ_BUFFER},\"first_to_last_ms\":{:.3},\
             \"send_to_last_ms\":{:.3},\"mib_per_s\":{:.3},\"lines_per_s\":{:.0}}}",
            e.reads - b.reads,
            secs * 1000.0,
            e.at.duration_since(sent).as_secs_f64() * 1000.0,
            bytes as f64 / secs / (1024.0 * 1024.0),
            LINES as f64 / secs,
        );
    }

    shell.send("exit\n");
    let deadline = Instant::now() + WAIT;
    while shell
        .session
        .try_wait()
        .expect("終了を確かめられなかった")
        .is_none()
    {
        assert!(Instant::now() < deadline, "シェルが終わらなかった");
        thread::sleep(Duration::from_millis(20));
    }
}
