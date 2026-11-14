//! セッション管理(`arin_lib::sessions`)を、偽の `PtyPort` と偽の出力の送り先で確かめる。
//! 設計書 §1.3 #2・#3、§7.1「ターミナルの入出力」、ADR-004、FR-043、NFR-004、NFR-006。
//!
//! - Tauri のアプリは作らないので、標準のテストハーネスで動かす(メニューの部品を作らない)
//! - どの待ちにも上限時間(`WAIT`)を付ける。待ちは条件が成り立つまでの見回りで、固定の時間に頼らない
//! - 「止まったまま読まない」ことだけは、成り立たないことの確認なので短い時間(`QUIET`)を置いて見る
//!   (読み取りが止まっていなければ必ず増えるので、遅いマシンでも誤って失敗することはない)

use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use arin_core::ports::{ChildExit, PtyError, PtyPort, PtySession, PtySize, SpawnedPty};
use arin_core::terminal::{ShellLaunchSpec, TerminalError, TerminalId, TerminalStatus};
use arin_core::workspace::{AppSnapshot, Workspace};
use arin_lib::sessions::{OutputSink, SessionConfig, SessionError, SessionRegistry, ShellDefaults};
use arin_lib::verify_log::{FlowSummary, VerifyConfig, VerifyRecord, VerifySink};

const WAIT: Duration = Duration::from_secs(10);
const QUIET: Duration = Duration::from_millis(100);
const HOME: &str = "/Users/someone";

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// 条件が成り立つまで待つ。上限を過ぎたら `false`。
fn wait_until(mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        thread::sleep(Duration::from_millis(1));
    }
    condition()
}

// ---------------------------------------------------------------------------
// 偽の PTY
// ---------------------------------------------------------------------------

/// 偽の PTY の出力の出し方。
#[derive(Clone, Copy)]
enum OutputMode {
    /// テストが `feed` で渡したものを順に出す。`close_output` で終わり(0 バイト)になる。
    Scripted,
    /// 読むたびに `n` バイトを出し続ける(大量出力)。
    Endless(usize),
}

/// 偽の PTY の操作の記録と、テストからの操作。
struct Control {
    mode: OutputMode,
    /// 起動の直後(接続の前)にシェルが出す出力。
    initial_output: Mutex<Vec<Vec<u8>>>,
    fail_spawn: AtomicBool,
    spawned: Mutex<Vec<(ShellLaunchSpec, PtySize)>>,
    feed: Mutex<Option<Sender<Vec<u8>>>>,
    bytes_read: AtomicUsize,
    written: Mutex<Vec<u8>>,
    sizes: Mutex<Vec<PtySize>>,
    killed: AtomicBool,
    exit: Mutex<Option<ChildExit>>,
}

impl Control {
    fn new(mode: OutputMode) -> Arc<Self> {
        Arc::new(Self {
            mode,
            initial_output: Mutex::new(Vec::new()),
            fail_spawn: AtomicBool::new(false),
            spawned: Mutex::new(Vec::new()),
            feed: Mutex::new(None),
            bytes_read: AtomicUsize::new(0),
            written: Mutex::new(Vec::new()),
            sizes: Mutex::new(Vec::new()),
            killed: AtomicBool::new(false),
            exit: Mutex::new(None),
        })
    }

    fn feed(&self, data: &[u8]) {
        lock(&self.feed)
            .as_ref()
            .expect("まだ起動していないか、出力を閉じた")
            .send(data.to_vec())
            .expect("読み取り側がいない");
    }

    fn close_output(&self) {
        lock(&self.feed).take();
    }

    fn set_exit(&self, code: u32) {
        *lock(&self.exit) = Some(ChildExit {
            exit_code: code,
            signal: None,
        });
    }

    fn spawn_count(&self) -> usize {
        lock(&self.spawned).len()
    }

    fn bytes_read(&self) -> usize {
        self.bytes_read.load(Ordering::SeqCst)
    }
}

struct FakePty(Arc<Control>);

impl PtyPort for FakePty {
    fn spawn(&self, spec: &ShellLaunchSpec, size: PtySize) -> Result<SpawnedPty, PtyError> {
        let control = &self.0;
        if control.fail_spawn.load(Ordering::SeqCst) {
            return Err(PtyError::Spawn("偽の起動の失敗".to_owned()));
        }
        lock(&control.spawned).push((spec.clone(), size));
        let (tx, rx) = mpsc::channel();
        for chunk in lock(&control.initial_output).drain(..) {
            tx.send(chunk).unwrap();
        }
        *lock(&control.feed) = Some(tx);
        Ok(SpawnedPty {
            session: Box::new(FakeSession(Arc::clone(control))),
            output: Box::new(FakeOutput {
                control: Arc::clone(control),
                rx,
                rest: Vec::new(),
            }),
        })
    }
}

struct FakeOutput {
    control: Arc<Control>,
    rx: Receiver<Vec<u8>>,
    rest: Vec<u8>,
}

impl Read for FakeOutput {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = match self.control.mode {
            OutputMode::Endless(n) => {
                if self.control.killed.load(Ordering::SeqCst) {
                    return Ok(0);
                }
                let n = n.min(buf.len());
                buf[..n].fill(b'x');
                n
            }
            OutputMode::Scripted => {
                if self.rest.is_empty() {
                    match self.rx.recv() {
                        Ok(chunk) => self.rest = chunk,
                        Err(_) => return Ok(0),
                    }
                }
                let n = self.rest.len().min(buf.len());
                buf[..n].copy_from_slice(&self.rest[..n]);
                self.rest.drain(..n);
                n
            }
        };
        self.control.bytes_read.fetch_add(n, Ordering::SeqCst);
        Ok(n)
    }
}

struct FakeSession(Arc<Control>);

impl PtySession for FakeSession {
    fn write(&mut self, data: &[u8]) -> Result<(), PtyError> {
        if self.0.killed.load(Ordering::SeqCst) {
            return Err(PtyError::Write("終了済み".to_owned()));
        }
        lock(&self.0.written).extend_from_slice(data);
        Ok(())
    }

    fn resize(&mut self, size: PtySize) -> Result<(), PtyError> {
        lock(&self.0.sizes).push(size);
        Ok(())
    }

    fn kill(&mut self) -> Result<(), PtyError> {
        self.0.killed.store(true, Ordering::SeqCst);
        // 本物と同じく、シェルが終わると出力も終わる。
        self.0.close_output();
        Ok(())
    }

    fn try_wait(&mut self) -> Result<Option<ChildExit>, PtyError> {
        Ok(lock(&self.0.exit).clone())
    }
}

// ---------------------------------------------------------------------------
// 偽の送り先と、状態の写しの受け取り
// ---------------------------------------------------------------------------

#[derive(Default)]
struct CollectSink {
    messages: Mutex<Vec<Vec<u8>>>,
    fail: AtomicBool,
    /// 失敗した分も含めて、送ろうとした回数。
    attempts: AtomicUsize,
}

impl CollectSink {
    fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn bytes(&self) -> Vec<u8> {
        lock(&self.messages).concat()
    }

    fn total(&self) -> usize {
        lock(&self.messages).iter().map(Vec::len).sum()
    }

    fn message_count(&self) -> usize {
        lock(&self.messages).len()
    }

    fn wait_total(&self, n: usize) -> bool {
        wait_until(|| self.total() >= n)
    }
}

impl OutputSink for CollectSink {
    fn send(&self, data: Vec<u8>) -> Result<(), String> {
        self.attempts.fetch_add(1, Ordering::SeqCst);
        if self.fail.load(Ordering::SeqCst) {
            return Err("画面が無い".to_owned());
        }
        lock(&self.messages).push(data);
        Ok(())
    }
}

struct Fixture {
    registry: SessionRegistry,
    control: Arc<Control>,
    snapshots: Arc<Mutex<Vec<AppSnapshot>>>,
    probes: Arc<AtomicUsize>,
    id: TerminalId,
}

impl Fixture {
    fn new(mode: OutputMode) -> Self {
        Self::with_config(mode, test_config())
    }

    fn with_config(mode: OutputMode, config: SessionConfig) -> Self {
        Self::with_verify(mode, config, None)
    }

    fn with_verify(mode: OutputMode, config: SessionConfig, verify: Option<VerifyConfig>) -> Self {
        let control = Control::new(mode);
        let snapshots = Arc::new(Mutex::new(Vec::new()));
        let probes = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&snapshots);
        let probe_count = Arc::clone(&probes);
        let workspace = Workspace::new("0.0.0-test", PathBuf::from(HOME), [("HOME", HOME)]);
        let registry = SessionRegistry::with_verify(
            workspace,
            Arc::new(FakePty(Arc::clone(&control))),
            Arc::new(move || {
                probe_count.fetch_add(1, Ordering::SeqCst);
                ShellDefaults {
                    shell: PathBuf::from("/bin/zsh"),
                    locale: Some("ja_JP".to_owned()),
                }
            }),
            Arc::new(move |snapshot| lock(&seen).push(snapshot)),
            config,
            verify,
        );
        let id = registry.snapshot().terminals[0].id.clone();
        Self {
            registry,
            control,
            snapshots,
            probes,
            id,
        }
    }

    fn connect(&self, sink: &Arc<CollectSink>) {
        self.registry
            .connect(&self.id, 24, 80, Arc::clone(sink) as Arc<dyn OutputSink>)
            .expect("接続できなかった");
    }

    fn status(&self) -> TerminalStatus {
        self.registry.snapshot().terminals[0].status.clone()
    }

    fn last_notified_status(&self) -> Option<TerminalStatus> {
        lock(&self.snapshots)
            .last()
            .map(|s| s.terminals[0].status.clone())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.registry.shutdown();
    }
}

/// テスト用の設定。終了の見回りを短くし、しきい値を小さくする。
fn test_config() -> SessionConfig {
    SessionConfig {
        high_water: 64 * 1024,
        low_water: 16 * 1024,
        exit_poll: Duration::from_millis(10),
        ..SessionConfig::default()
    }
}

fn exited(code: u32) -> TerminalStatus {
    TerminalStatus::Exited {
        code: Some(code),
        signal: None,
    }
}

// ---------------------------------------------------------------------------
// テスト
// ---------------------------------------------------------------------------

#[test]
fn 状態の写しにはまだ起動していないターミナルが_1_枚載り_接続で_running_になる() {
    let f = Fixture::new(OutputMode::Scripted);
    let snapshot = f.registry.snapshot();
    assert_eq!(snapshot.terminals.len(), 1);
    assert_eq!(snapshot.terminals[0].status, TerminalStatus::NotStarted);
    assert_eq!(f.control.spawn_count(), 0, "接続の前に PTY を起動した");

    f.connect(&CollectSink::new());

    assert_eq!(f.status(), TerminalStatus::Running);
    assert_eq!(f.last_notified_status(), Some(TerminalStatus::Running));
    assert_eq!(f.control.spawn_count(), 1);
}

#[test]
fn 最初の接続で接続の寸法とホームフォルダでシェルを起動する() {
    let f = Fixture::new(OutputMode::Scripted);

    f.registry
        .connect(&f.id, 30, 100, CollectSink::new())
        .unwrap();

    let spawned = lock(&f.control.spawned);
    let (spec, size) = &spawned[0];
    assert_eq!(
        *size,
        PtySize {
            rows: 30,
            cols: 100
        }
    );
    assert_eq!(spec.program(), Path::new("/bin/zsh"));
    assert_eq!(spec.cwd(), Path::new(HOME));
}

#[test]
fn 出力が順序どおりに届き_細切れにせずまとめて送る() {
    let f = Fixture::new(OutputMode::Scripted);
    let chunks: Vec<Vec<u8>> = (0..1000)
        .map(|i| format!("line-{i:04}\n").into_bytes())
        .collect();
    let expected = chunks.concat();
    // 読み取りが始まる前に 1,000 回ぶんの出力が溜まっている状態にする。
    *lock(&f.control.initial_output) = chunks;
    let sink = CollectSink::new();

    f.connect(&sink);

    assert!(sink.wait_total(expected.len()), "出力が全部届かなかった");
    assert_eq!(sink.bytes(), expected, "順序が崩れたか、欠けた");
    assert!(
        sink.message_count() < 1000,
        "読み取り 1 回ごとに送っている({} 回)",
        sink.message_count()
    );
}

#[test]
fn 接続の前にシェルが出した出力を取りこぼさない() {
    let f = Fixture::new(OutputMode::Scripted);
    *lock(&f.control.initial_output) = vec![b"Last login: today\r\n".to_vec(), b"% ".to_vec()];
    let sink = CollectSink::new();

    f.connect(&sink);
    f.control.feed(b"after");

    assert!(sink.wait_total(24));
    assert_eq!(sink.bytes(), b"Last login: today\r\n% after");
}

#[test]
fn 未処理の量が上限を超えたら読み取りを止め_処理済みの通知で再開する() {
    let config = test_config();
    let f = Fixture::with_config(OutputMode::Endless(4096), config.clone());
    let sink = CollectSink::new();

    f.connect(&sink);

    // 上限を超えて止まり、読んだ分は全部画面へ送られる。
    assert!(
        wait_until(|| f.registry.backlog(&f.id).is_some_and(|b| b.paused)),
        "上限を超えても止まらなかった"
    );
    assert!(
        wait_until(|| sink.total() == f.control.bytes_read()),
        "読んだ分が送られなかった"
    );
    let read_when_paused = f.control.bytes_read();
    assert!(read_when_paused as u64 > config.high_water);
    assert!(
        read_when_paused as u64 <= config.high_water + config.read_buffer as u64,
        "上限を大きく超えて読んだ({read_when_paused})"
    );

    // 止まっているあいだは読まない。
    thread::sleep(QUIET);
    assert_eq!(
        f.control.bytes_read(),
        read_when_paused,
        "止めたのに読み続けた"
    );
    assert_eq!(
        f.registry.backlog(&f.id).map(|b| b.unacked),
        Some(read_when_paused as u64)
    );

    // 下限より上までの通知では再開しない。
    let not_enough = read_when_paused as u64 - config.low_water - 1;
    f.registry.ack(&f.id, not_enough).unwrap();
    thread::sleep(QUIET);
    assert_eq!(
        f.control.bytes_read(),
        read_when_paused,
        "下限より上で再開した"
    );

    // 下限まで減ったら再開する。
    f.registry.ack(&f.id, 1).unwrap();
    assert!(
        wait_until(|| f.control.bytes_read() > read_when_paused),
        "処理済みの通知で再開しなかった"
    );
}

#[test]
fn 処理済みの通知が未処理の量を超えても_0_に留める() {
    let f = Fixture::new(OutputMode::Scripted);
    let sink = CollectSink::new();
    f.connect(&sink);
    f.control.feed(b"abc");
    assert!(sink.wait_total(3));

    f.registry.ack(&f.id, u64::MAX).unwrap();

    let backlog = f.registry.backlog(&f.id).unwrap();
    assert_eq!(backlog.unacked, 0);
    assert!(!backlog.paused);
}

#[test]
fn 二重の接続は_pty_を起動し直さず送り先だけを付け替える() {
    let f = Fixture::with_config(OutputMode::Endless(4096), test_config());
    let first = CollectSink::new();
    f.connect(&first);
    assert!(wait_until(|| f
        .registry
        .backlog(&f.id)
        .is_some_and(|b| b.paused)));

    let second = CollectSink::new();
    f.registry
        .connect(&f.id, 40, 120, Arc::clone(&second) as Arc<dyn OutputSink>)
        .unwrap();

    assert_eq!(f.control.spawn_count(), 1, "PTY を起動し直した");
    // 前の画面の未処理の量は捨てて再開し、新しい送り先へ流れる。
    assert!(second.wait_total(1), "新しい送り先に出力が届かなかった");
    // 新しい画面の寸法を PTY に伝える。
    assert_eq!(
        lock(&f.control.sizes).as_slice(),
        &[PtySize {
            rows: 40,
            cols: 120
        }]
    );
}

#[test]
fn 付け替えたあとは前の送り先に送らない() {
    let f = Fixture::new(OutputMode::Scripted);
    let first = CollectSink::new();
    f.connect(&first);
    f.control.feed(b"one");
    assert!(first.wait_total(3));

    let second = CollectSink::new();
    f.connect(&second);
    f.control.feed(b"two");

    assert!(second.wait_total(3));
    assert_eq!(second.bytes(), b"two");
    assert_eq!(first.bytes(), b"one");
}

#[test]
fn 送り先が失敗しても止まらず_付け替えで続きが届く() {
    let f = Fixture::new(OutputMode::Scripted);
    let broken = CollectSink::new();
    broken.fail.store(true, Ordering::SeqCst);
    f.connect(&broken);
    f.control.feed(b"lost");
    assert!(wait_until(|| broken.attempts.load(Ordering::SeqCst) == 1));

    let sink = CollectSink::new();
    f.connect(&sink);
    f.control.feed(b"kept");

    assert!(sink.wait_total(4));
    assert_eq!(sink.bytes(), b"kept");
}

#[test]
fn 入力と寸法の変更が_pty_に届く() {
    let f = Fixture::new(OutputMode::Scripted);
    f.connect(&CollectSink::new());

    f.registry.write(&f.id, "echo ok\r".as_bytes()).unwrap();
    f.registry.resize(&f.id, 50, 132).unwrap();

    assert_eq!(lock(&f.control.written).as_slice(), b"echo ok\r");
    assert_eq!(
        lock(&f.control.sizes).as_slice(),
        &[PtySize {
            rows: 50,
            cols: 132
        }]
    );
}

#[test]
fn 出力が終わりシェルが終わったら_exited_を写しに反映して知らせる() {
    let f = Fixture::new(OutputMode::Scripted);
    let sink = CollectSink::new();
    f.connect(&sink);
    f.control.feed(b"bye");
    f.control.set_exit(0);

    f.control.close_output();

    assert!(
        wait_until(|| f.status() == exited(0)),
        "終了を反映しなかった"
    );
    assert_eq!(f.last_notified_status(), Some(exited(0)));
    // 終わりの直前の出力も届いている。
    assert_eq!(sink.bytes(), b"bye");
}

#[test]
fn 出力が終わらなくても見回りでシェルの終了に気づく() {
    let f = Fixture::new(OutputMode::Scripted);
    f.connect(&CollectSink::new());

    // 端末を開いたままの子孫が残っている想定(出力は終わらない)。
    f.control.set_exit(3);

    assert!(
        wait_until(|| f.status() == exited(3)),
        "見回りで終了に気づかなかった"
    );
}

#[test]
fn シェルの終了後の書き込みは失敗し_寸法の変更は何もしない() {
    let f = Fixture::new(OutputMode::Scripted);
    f.connect(&CollectSink::new());
    f.control.set_exit(0);
    f.control.close_output();
    assert!(wait_until(|| f.status() == exited(0)));

    assert_eq!(
        f.registry.write(&f.id, b"ls\r"),
        Err(SessionError::Terminal(TerminalError::Exited(f.id.clone())))
    );
    assert_eq!(f.registry.resize(&f.id, 10, 10), Ok(()));
    assert!(lock(&f.control.sizes).is_empty());
    assert_eq!(f.registry.ack(&f.id, 10), Ok(()));
    assert_eq!(
        f.registry.connect(&f.id, 24, 80, CollectSink::new()),
        Err(SessionError::Terminal(TerminalError::Exited(f.id.clone())))
    );
}

#[test]
fn 接続の前の入力と寸法の変更は断り_処理済みの通知は受け流す() {
    let f = Fixture::new(OutputMode::Scripted);
    let not_started = Err(SessionError::Terminal(TerminalError::NotStarted(
        f.id.clone(),
    )));

    assert_eq!(f.registry.write(&f.id, b"x"), not_started);
    assert_eq!(f.registry.resize(&f.id, 24, 80), not_started);
    assert_eq!(f.registry.ack(&f.id, 10), Ok(()));
    assert_eq!(f.registry.backlog(&f.id), None);
}

#[test]
fn 存在しない_id_はどのコマンドでも見つからないと返す() {
    let f = Fixture::new(OutputMode::Scripted);
    let id = TerminalId::new("terminal-999");
    let not_found = Err(SessionError::Terminal(TerminalError::NotFound(id.clone())));

    assert_eq!(
        f.registry.connect(&id, 24, 80, CollectSink::new()),
        not_found
    );
    assert_eq!(f.registry.write(&id, b"x"), not_found);
    assert_eq!(f.registry.ack(&id, 1), not_found);
    assert_eq!(f.registry.resize(&id, 24, 80), not_found);
    assert_eq!(f.control.spawn_count(), 0);
}

#[test]
fn 寸法が_0_なら接続も寸法の変更も断る() {
    let f = Fixture::new(OutputMode::Scripted);

    assert_eq!(
        f.registry.connect(&f.id, 0, 80, CollectSink::new()),
        Err(SessionError::Terminal(TerminalError::InvalidSize {
            rows: 0,
            cols: 80
        }))
    );
    assert_eq!(f.control.spawn_count(), 0, "寸法が 0 なのに起動した");

    f.connect(&CollectSink::new());
    assert_eq!(
        f.registry.resize(&f.id, 24, 0),
        Err(SessionError::Terminal(TerminalError::InvalidSize {
            rows: 24,
            cols: 0
        }))
    );
    assert!(lock(&f.control.sizes).is_empty());
}

#[test]
fn 起動に失敗したら_failed_を知らせ_次の接続で起動し直す() {
    let f = Fixture::new(OutputMode::Scripted);
    f.control.fail_spawn.store(true, Ordering::SeqCst);

    let err = f
        .registry
        .connect(&f.id, 24, 80, CollectSink::new())
        .unwrap_err();

    assert_eq!(
        err,
        SessionError::Pty(PtyError::Spawn("偽の起動の失敗".to_owned()))
    );
    assert!(
        matches!(f.status(), TerminalStatus::Failed { message } if message.contains("偽の起動の失敗"))
    );
    assert!(matches!(
        f.last_notified_status(),
        Some(TerminalStatus::Failed { .. })
    ));

    f.control.fail_spawn.store(false, Ordering::SeqCst);
    f.connect(&CollectSink::new());
    assert_eq!(f.status(), TerminalStatus::Running);
    // シェルと言語設定の取得は 1 回だけ(dscl・defaults を何度も起動しない)。
    assert_eq!(f.probes.load(Ordering::SeqCst), 1);
}

#[test]
fn 終了の処理で_pty_を終わらせ_止まっていた読み取りも抜ける() {
    let f = Fixture::with_config(OutputMode::Endless(4096), test_config());
    f.connect(&CollectSink::new());
    assert!(wait_until(|| f
        .registry
        .backlog(&f.id)
        .is_some_and(|b| b.paused)));

    f.registry.shutdown();

    assert!(
        f.control.killed.load(Ordering::SeqCst),
        "シェルを終わらせなかった"
    );
    assert_eq!(f.registry.backlog(&f.id), None);
    assert!(f.registry.write(&f.id, b"x").is_err());
    // 2 回目は何もしない。
    f.registry.shutdown();
}

#[test]
fn 失敗の文言に原因が載る() {
    let id = TerminalId::new("t-1");
    assert!(
        SessionError::Terminal(TerminalError::NotFound(id))
            .to_string()
            .contains("t-1")
    );
    assert!(
        SessionError::Pty(PtyError::Write("broken pipe".to_owned()))
            .to_string()
            .contains("broken pipe")
    );
}

// ---------------------------------------------------------------------------
// 検証用の記録(T23。設計書 §2.2「ウィンドウが隠れたとき」「クリップボードの制御シーケンス」「流量制御」)
// ---------------------------------------------------------------------------

/// 記録の口の偽物。届いた記録を順に持つ。
#[derive(Default)]
struct CollectRecords(Mutex<Vec<VerifyRecord>>);

impl CollectRecords {
    fn clipboard_totals(&self) -> Vec<u64> {
        lock(&self.0)
            .iter()
            .filter_map(|r| match r {
                VerifyRecord::Clipboard { total, .. } => Some(*total),
                VerifyRecord::Flow(_) => None,
            })
            .collect()
    }

    fn flows(&self) -> Vec<FlowSummary> {
        lock(&self.0)
            .iter()
            .filter_map(|r| match r {
                VerifyRecord::Flow(s) => Some(s.clone()),
                VerifyRecord::Clipboard { .. } => None,
            })
            .collect()
    }

    fn lines(&self) -> Vec<String> {
        lock(&self.0).iter().map(VerifyRecord::line).collect()
    }
}

impl VerifySink for CollectRecords {
    fn record(&self, record: VerifyRecord) {
        lock(&self.0).push(record);
    }
}

/// 差し替えの時計。テストが進めた分だけ進む。
struct FakeClock {
    base: Instant,
    offset_ms: AtomicU64,
}

impl FakeClock {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            base: Instant::now(),
            offset_ms: AtomicU64::new(0),
        })
    }

    fn advance(&self, ms: u64) {
        self.offset_ms.fetch_add(ms, Ordering::SeqCst);
    }
}

const INTERVAL: Duration = Duration::from_secs(5);

fn verify_fixture(mode: OutputMode) -> (Fixture, Arc<CollectRecords>, Arc<FakeClock>) {
    let records = Arc::new(CollectRecords::default());
    let clock = FakeClock::new();
    let now = Arc::clone(&clock);
    let verify = VerifyConfig {
        sink: Arc::clone(&records) as Arc<dyn VerifySink>,
        clock: Arc::new(move || {
            now.base + Duration::from_millis(now.offset_ms.load(Ordering::SeqCst))
        }),
        interval: INTERVAL,
    };
    let f = Fixture::with_verify(mode, test_config(), Some(verify));
    (f, records, clock)
}

#[test]
fn 出力に_osc_52_が現れた回数が記録の口に届き_本文は載らない() {
    let (f, records, _clock) = verify_fixture(OutputMode::Scripted);
    let sink = CollectSink::new();
    f.connect(&sink);

    // 1 つ目は読み取りの切れ目をまたぐ。OSC 9 は数えない。
    let chunks: [&[u8]; 3] = [
        b"a\x1b]5",
        b"2;c;c2VjcmV0\x07\x1b]9;done\x07",
        b"\x1b]52;c;c2VjcmV0\x1b\\",
    ];
    f.control.feed(chunks[0]);
    assert!(sink.wait_total(chunks[0].len()));
    f.control.feed(chunks[1]);
    f.control.feed(chunks[2]);

    assert!(
        wait_until(|| records.clipboard_totals() == vec![1, 2]),
        "回数が届かなかった: {:?}",
        records.clipboard_totals()
    );
    // 出力はそのまま画面へ届く(走査は出力を変えない)。
    assert!(sink.wait_total(chunks.iter().map(|c| c.len()).sum()));
    assert_eq!(sink.bytes(), chunks.concat());
    for line in records.lines() {
        assert!(line.starts_with("[arin:verify] osc52 "), "{line}");
        assert!(!line.contains("c2VjcmV0"), "本文が載った: {line}");
    }
}

#[test]
fn 処理済みの通知を一定の間隔でまとめて記録する() {
    let (f, records, clock) = verify_fixture(OutputMode::Scripted);
    let sink = CollectSink::new();
    f.connect(&sink);
    f.control.feed(&[b'x'; 1000]);
    assert!(sink.wait_total(1000));

    f.registry.ack(&f.id, 100).unwrap();
    clock.advance(30);
    f.registry.ack(&f.id, 200).unwrap();
    clock.advance(50);
    f.registry.ack(&f.id, 300).unwrap();
    // 間隔に達するまではまとめを出さない。
    assert!(records.flows().is_empty());

    clock.advance(INTERVAL.as_millis() as u64);
    assert!(
        wait_until(|| !records.flows().is_empty()),
        "まとめが届かなかった"
    );
    let summary = records.flows()[0].clone();
    assert_eq!(summary.terminal, f.id);
    assert_eq!(summary.acks, 3);
    assert_eq!(summary.ack_bytes, 600);
    assert_eq!(summary.max_ack_gap_ms, Some(50));
    assert_eq!(summary.read_bytes, 1000);
    assert_eq!(summary.max_unacked, 1000);
    assert_eq!(summary.unacked, 400);
    assert!(!summary.paused);
    assert_eq!(summary.pauses, 0);
    assert!(summary.window_ms >= INTERVAL.as_millis() as u64);

    // 次の間隔は数え直す。通知も出力も無く止まってもいなければ出さない。
    let before = records.flows().len();
    f.registry.ack(&f.id, 50).unwrap();
    clock.advance(INTERVAL.as_millis() as u64);
    assert!(wait_until(|| records.flows().len() > before));
    let next = records.flows()[before].clone();
    assert_eq!(next.acks, 1);
    assert_eq!(next.ack_bytes, 50);
    assert_eq!(next.read_bytes, 0);
    assert_eq!(next.max_unacked, 400);
    assert_eq!(next.unacked, 350);
}

#[test]
fn 未処理の量の最大値と読み取りを止めた回数を記録する() {
    let (f, records, clock) = verify_fixture(OutputMode::Endless(4096));
    let config = test_config();
    f.connect(&CollectSink::new());
    assert!(wait_until(|| f
        .registry
        .backlog(&f.id)
        .is_some_and(|b| b.paused)));

    // 画面が通知を返さないまま(ウィンドウが隠れて止まった場合)でも、止まっていることが記録に出る。
    clock.advance(INTERVAL.as_millis() as u64);
    assert!(
        wait_until(|| !records.flows().is_empty()),
        "まとめが届かなかった"
    );
    let summary = records.flows()[0].clone();
    assert_eq!(summary.acks, 0);
    assert_eq!(summary.last_ack_ago_ms, None);
    assert!(summary.paused);
    assert_eq!(summary.pauses, 1);
    assert!(summary.max_unacked > config.high_water);
    // 上限で頭打ちになる(メモリが増え続けない)。
    assert!(summary.max_unacked <= config.high_water + config.read_buffer as u64);
    assert_eq!(summary.peak_unacked, summary.max_unacked);
    assert_eq!(summary.unacked, summary.max_unacked);
}
