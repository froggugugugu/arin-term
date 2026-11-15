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
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use arin_core::persistence::codec::PersistedState;
use arin_core::ports::{ChildExit, Foreground, PtyError, PtyPort, PtySession, PtySize, SpawnedPty};
use arin_core::project::{Project, ProjectColor, ProjectId, RootReference};
use arin_core::terminal::{ShellLaunchSpec, TerminalError, TerminalId, TerminalStatus};
use arin_core::workspace::{
    AppSnapshot, ConnectOutcome, RootResolution, Startup, Workspace, WorkspaceError,
};
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
    /// `true` のあいだ、書き込みは PTY のロックを持ったまま止まる(シェルが入力を読まない状態)。
    hold_writes: Mutex<bool>,
    release: Condvar,
    /// 止まっている書き込みの数。
    held: AtomicUsize,
    /// 前面で動いているもの(`PtySession::foreground` が返す値)。
    foreground: Mutex<Foreground>,
    /// 入力・寸法・終了の操作(`FakeSession`)を捨てたか。
    session_dropped: AtomicBool,
    /// 出力(`FakeOutput`。読み取りスレッドが持つ)を捨てたか。
    output_dropped: AtomicBool,
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
            hold_writes: Mutex::new(false),
            release: Condvar::new(),
            held: AtomicUsize::new(0),
            foreground: Mutex::new(Foreground::Unknown),
            session_dropped: AtomicBool::new(false),
            output_dropped: AtomicBool::new(false),
        })
    }

    /// 書き込みを止めるか。止めるのをやめたら、止まっている書き込みを再開させる。
    fn set_hold_writes(&self, hold: bool) {
        *lock(&self.hold_writes) = hold;
        self.release.notify_all();
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
        spawn_on(&self.0, spec, size)
    }
}

/// 偽の PTY を `control` で起動する(`FakePty` と、9 枚の検証画面の `GridPty` が共有する)。
fn spawn_on(
    control: &Arc<Control>,
    spec: &ShellLaunchSpec,
    size: PtySize,
) -> Result<SpawnedPty, PtyError> {
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

impl Drop for FakeOutput {
    fn drop(&mut self) {
        self.control.output_dropped.store(true, Ordering::SeqCst);
    }
}

struct FakeSession(Arc<Control>);

impl Drop for FakeSession {
    fn drop(&mut self) {
        self.0.session_dropped.store(true, Ordering::SeqCst);
    }
}

impl PtySession for FakeSession {
    fn write(&mut self, data: &[u8]) -> Result<(), PtyError> {
        let mut hold = lock(&self.0.hold_writes);
        if *hold {
            self.0.held.fetch_add(1, Ordering::SeqCst);
            while *hold {
                hold = self
                    .0
                    .release
                    .wait(hold)
                    .unwrap_or_else(PoisonError::into_inner);
            }
            self.0.held.fetch_sub(1, Ordering::SeqCst);
        }
        drop(hold);
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

    fn foreground(&mut self) -> Foreground {
        *lock(&self.0.foreground)
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
        // シェルが終わってもタブを残す 9 枚の検証画面の形で作り、1 枚目だけを使う(タブを閉じる形のテストは下の T33b の節)。
        let workspace =
            Workspace::for_verify_grid("0.0.0-test", PathBuf::from(HOME), [("HOME", HOME)]);
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
fn 状態の写しにはまだ起動していないターミナルが載り_接続で_running_になる() {
    let f = Fixture::new(OutputMode::Scripted);
    let snapshot = f.registry.snapshot();
    assert_eq!(snapshot.terminals.len(), 9);
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
        f.registry
            .connect(&id, 24, 80, CollectSink::new())
            .map(|_| ()),
        not_found
    );
    assert_eq!(f.registry.close_terminal(&id), not_found);
    assert_eq!(f.registry.select_terminal(&id), not_found);
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

// ---------------------------------------------------------------------------
// 9 枚の検証画面(T22。NFR-006。M4 の T49 で外す)
// ---------------------------------------------------------------------------

/// 起動のたびに、用意した偽の PTY を順に 1 つずつ使う(1 枚目の起動に 1 つ目、…)。
struct GridPty {
    controls: Vec<Arc<Control>>,
    next: AtomicUsize,
}

impl PtyPort for GridPty {
    fn spawn(&self, spec: &ShellLaunchSpec, size: PtySize) -> Result<SpawnedPty, PtyError> {
        let index = self.next.fetch_add(1, Ordering::SeqCst);
        let control = self
            .controls
            .get(index)
            .ok_or_else(|| PtyError::Spawn("用意した偽の PTY を使い切った".to_owned()))?;
        spawn_on(control, spec, size)
    }
}

struct GridFixture {
    registry: SessionRegistry,
    controls: Vec<Arc<Control>>,
    ids: Vec<TerminalId>,
    sinks: Vec<Arc<CollectSink>>,
}

impl GridFixture {
    /// 9 枚の Workspace を作る。`fail_spawn` の番号(0 から)の偽の PTY は起動に失敗する。
    fn new(fail_spawn: &[usize]) -> Self {
        let controls: Vec<Arc<Control>> =
            (0..9).map(|_| Control::new(OutputMode::Scripted)).collect();
        for &i in fail_spawn {
            controls[i].fail_spawn.store(true, Ordering::SeqCst);
        }
        let workspace =
            Workspace::for_verify_grid("0.0.0-test", PathBuf::from(HOME), [("HOME", HOME)]);
        let registry = SessionRegistry::new(
            workspace,
            Arc::new(GridPty {
                controls: controls.clone(),
                next: AtomicUsize::new(0),
            }),
            Arc::new(|| ShellDefaults {
                shell: PathBuf::from("/bin/zsh"),
                locale: None,
            }),
            Arc::new(|_| {}),
            test_config(),
        );
        let ids: Vec<TerminalId> = registry
            .snapshot()
            .terminals
            .into_iter()
            .map(|t| t.id)
            .collect();
        let sinks = ids.iter().map(|_| CollectSink::new()).collect();
        Self {
            registry,
            controls,
            ids,
            sinks,
        }
    }

    /// 9 枚を順に接続する。失敗した接続の結果も返す。
    fn connect_all(&self) -> Vec<Result<ConnectOutcome, SessionError>> {
        self.ids
            .iter()
            .zip(&self.sinks)
            .map(|(id, sink)| {
                self.registry
                    .connect(id, 12, 40, Arc::clone(sink) as Arc<dyn OutputSink>)
            })
            .collect()
    }

    fn status(&self, i: usize) -> TerminalStatus {
        self.registry.snapshot().terminals[i].status.clone()
    }

    /// `i` 枚目に入力を書き、出力を流し、両方が届くことを確かめる。
    fn assert_io(&self, i: usize, label: &str) {
        let id = &self.ids[i];
        let control = &self.controls[i];
        let sink = &self.sinks[i];
        let input = format!("echo {id}\r");
        self.registry
            .write(id, input.as_bytes())
            .unwrap_or_else(|e| panic!("{label}: {id} に書き込めなかった: {e}"));
        assert!(
            lock(&control.written).ends_with(input.as_bytes()),
            "{label}: {id} の入力が PTY に届かなかった"
        );
        let before = sink.total();
        let output = format!("out-{id}\r\n");
        control.feed(output.as_bytes());
        assert!(
            sink.wait_total(before + output.len()),
            "{label}: {id} の出力が届かなかった"
        );
        assert!(sink.bytes().ends_with(output.as_bytes()));
        self.registry.ack(id, output.len() as u64).unwrap();
    }
}

impl Drop for GridFixture {
    fn drop(&mut self) {
        self.registry.shutdown();
    }
}

#[test]
fn _9_枚はそれぞれ最初の接続で別の_pty_をホームフォルダで起動し_それぞれ入出力できる() {
    let f = GridFixture::new(&[]);
    assert_eq!(f.ids.len(), 9);
    assert!(
        f.controls.iter().all(|c| c.spawn_count() == 0),
        "接続の前に起動した"
    );

    for result in f.connect_all() {
        result.expect("接続できなかった");
    }

    for (i, control) in f.controls.iter().enumerate() {
        assert_eq!(control.spawn_count(), 1, "{} 枚目の起動の回数", i + 1);
        let spawned = lock(&control.spawned);
        assert_eq!(spawned[0].0.cwd(), Path::new(HOME));
        assert_eq!(spawned[0].1, PtySize { rows: 12, cols: 40 });
        drop(spawned);
        assert_eq!(f.status(i), TerminalStatus::Running);
    }
    for i in 0..9 {
        f.assert_io(i, "全部動いているとき");
    }
    // 出力はほかのターミナルの送り先に混ざらない。
    for (i, sink) in f.sinks.iter().enumerate() {
        assert_eq!(sink.bytes(), format!("out-{}\r\n", f.ids[i]).into_bytes());
    }
}

#[test]
fn _9_枚のうち_1_枚のシェルが終わっても_残りの_8_枚は入出力を続ける() {
    let f = GridFixture::new(&[]);
    for result in f.connect_all() {
        result.expect("接続できなかった");
    }

    // 5 枚目のシェルが終わる(出力も終わる)。
    f.controls[4].set_exit(1);
    f.controls[4].close_output();
    assert!(
        wait_until(|| f.status(4) == exited(1)),
        "5 枚目の終了が反映されなかった"
    );

    assert!(matches!(
        f.registry.write(&f.ids[4], b"x"),
        Err(SessionError::Terminal(TerminalError::Exited(_)))
    ));
    for i in (0..9).filter(|&i| i != 4) {
        assert_eq!(f.status(i), TerminalStatus::Running);
        f.assert_io(i, "1 枚が終わったあと");
    }
}

#[test]
fn _9_枚のうち_1_枚の起動に失敗しても_残りの_8_枚は起動して入出力を続ける() {
    let f = GridFixture::new(&[2]);

    let results = f.connect_all();

    for (i, result) in results.iter().enumerate() {
        if i == 2 {
            assert!(
                matches!(result, Err(SessionError::Pty(PtyError::Spawn(_)))),
                "3 枚目の接続が失敗しなかった: {result:?}"
            );
        } else {
            assert!(result.is_ok(), "{} 枚目の接続が失敗した: {result:?}", i + 1);
        }
    }
    assert!(matches!(f.status(2), TerminalStatus::Failed { .. }));
    for i in (0..9).filter(|&i| i != 2) {
        assert_eq!(f.status(i), TerminalStatus::Running);
        f.assert_io(i, "1 枚が起動に失敗したあと");
    }
}

// ---------------------------------------------------------------------------
// 書き込みが止まっているあいだの再接続(S-1)
// ---------------------------------------------------------------------------

/// 別のスレッドで `f` を行い、`WAIT` までに戻れば結果を返す。戻らなければ `None`(スレッドは置いていく)。
fn within_wait<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(WAIT).ok()
}

#[test]
fn 書き込みが止まっていても再接続は戻り_ほかの接続も止まらない() {
    let f = GridFixture::new(&[]);
    for result in f.connect_all() {
        result.expect("接続できなかった");
    }
    let stuck = f.ids[0].clone();
    f.controls[0].set_hold_writes(true);
    let writer = {
        let registry = f.registry.clone();
        let id = stuck.clone();
        thread::spawn(move || registry.write(&id, b"paste"))
    };
    assert!(
        wait_until(|| f.controls[0].held.load(Ordering::SeqCst) == 1),
        "書き込みが止まらなかった"
    );

    // 画面の再読み込み: 止まっているターミナルの再接続と、ほかのターミナルの再接続。
    let reattach = {
        let registry = f.registry.clone();
        let id = stuck.clone();
        let sink = CollectSink::new();
        within_wait(move || registry.connect(&id, 30, 90, sink as Arc<dyn OutputSink>))
    };
    assert!(
        matches!(reattach, Some(Ok(_))),
        "止まっているターミナルの再接続が戻らないか失敗した: {reattach:?}"
    );
    let other = {
        let registry = f.registry.clone();
        let id = f.ids[1].clone();
        let sink = CollectSink::new();
        within_wait(move || registry.connect(&id, 30, 90, sink as Arc<dyn OutputSink>))
    };
    assert!(
        matches!(other, Some(Ok(_))),
        "ほかのターミナルの再接続が止まった: {other:?}"
    );
    assert_eq!(
        lock(&f.controls[1].sizes).last(),
        Some(&PtySize { rows: 30, cols: 90 }),
        "ほかのターミナルには寸法が伝わる"
    );

    // 書き込みが再開すれば、止まっていたターミナルにも次の寸法の変更が届く。
    f.controls[0].set_hold_writes(false);
    writer
        .join()
        .unwrap()
        .expect("止まっていた書き込みが失敗した");
    assert_eq!(lock(&f.controls[0].written).as_slice(), b"paste");
    f.registry.resize(&stuck, 30, 90).unwrap();
    assert_eq!(
        lock(&f.controls[0].sizes).last(),
        Some(&PtySize { rows: 30, cols: 90 })
    );
}

// ---------------------------------------------------------------------------
// プロジェクトのタブとセッションの後片付け(T33b。FR-039・FR-042・FR-044、NFR-006、M1 の見送り)
// ---------------------------------------------------------------------------

const ROOT_A: &str = "/Users/someone/dev/alpha";
const ROOT_B: &str = "/Users/someone/dev/beta";

fn project_id(n: u8) -> ProjectId {
    ProjectId::from_random_bytes([n; 16])
}

/// プロジェクト A(選択中)と B を 1 枚ずつのタブで持つ、通常の起動の Workspace。シェルが終わったタブは閉じる。
fn two_projects() -> Workspace {
    let projects: Vec<(ProjectId, RootReference)> = [(1, ROOT_A), (2, ROOT_B)]
        .into_iter()
        .map(|(n, root)| {
            (
                project_id(n),
                RootReference::new(PathBuf::from(root), Some("Ym9va21hcms=".to_owned())),
            )
        })
        .collect();
    Workspace::restore(
        "0.0.0-test",
        PathBuf::from(HOME),
        [("HOME", HOME)],
        Startup {
            state: PersistedState {
                projects: projects
                    .iter()
                    .map(|(id, root)| {
                        Project::restore(id.clone(), root.clone(), None, ProjectColor::Green)
                    })
                    .collect(),
                selected_project_id: Some(project_id(1)),
                ..PersistedState::default()
            },
            roots: projects
                .into_iter()
                .map(|(id, root)| (id, RootResolution::Found(root)))
                .collect(),
            notice: None,
        },
    )
}

/// 2 つのプロジェクトの Workspace と、起動の順に 1 つずつ使う偽の PTY(`count` 個)。
struct ProjectFixture {
    registry: SessionRegistry,
    controls: Vec<Arc<Control>>,
    snapshots: Arc<Mutex<Vec<AppSnapshot>>>,
}

impl ProjectFixture {
    fn new(count: usize) -> Self {
        let controls: Vec<Arc<Control>> = (0..count)
            .map(|_| Control::new(OutputMode::Scripted))
            .collect();
        let snapshots = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&snapshots);
        let registry = SessionRegistry::new(
            two_projects(),
            Arc::new(GridPty {
                controls: controls.clone(),
                next: AtomicUsize::new(0),
            }),
            Arc::new(|| ShellDefaults {
                shell: PathBuf::from("/bin/zsh"),
                locale: None,
            }),
            Arc::new(move |snapshot| lock(&seen).push(snapshot)),
            test_config(),
        );
        Self {
            registry,
            controls,
            snapshots,
        }
    }

    /// `project` 番目(0 から)のプロジェクトのタブ。
    fn tabs(&self, project: usize) -> Vec<TerminalId> {
        self.registry.snapshot().projects[project]
            .terminal_ids
            .clone()
    }

    fn connect(&self, id: &TerminalId, sink: &Arc<CollectSink>) -> ConnectOutcome {
        self.registry
            .connect(id, 24, 80, Arc::clone(sink) as Arc<dyn OutputSink>)
            .unwrap_or_else(|e| panic!("{id} に接続できなかった: {e}"))
    }

    fn has_terminal(&self, id: &TerminalId) -> bool {
        self.registry
            .snapshot()
            .terminals
            .iter()
            .any(|t| &t.id == id)
    }

    /// 入力を書き、出力を流し、両方が届くことを確かめる。
    fn assert_io(&self, id: &TerminalId, control: &Control, sink: &CollectSink) {
        let input = format!("echo {id}\r");
        self.registry
            .write(id, input.as_bytes())
            .unwrap_or_else(|e| panic!("{id} に書き込めなかった: {e}"));
        assert!(lock(&control.written).ends_with(input.as_bytes()));
        let before = sink.total();
        let output = format!("out-{id}\r\n");
        control.feed(output.as_bytes());
        assert!(
            sink.wait_total(before + output.len()),
            "{id} の出力が届かなかった"
        );
        self.registry.ack(id, output.len() as u64).unwrap();
    }
}

impl Drop for ProjectFixture {
    fn drop(&mut self) {
        self.registry.shutdown();
    }
}

/// セッションの資源(送り先を持つ出力の状態、PTY の操作と出力)がすべて捨てられるまで待つ。
/// 送り先を持つのはセッションと 2 本のスレッドだけなので、テストの持つ 1 つだけになれば、スレッドは抜けている。
fn wait_released(sink: &Arc<CollectSink>, control: &Control) -> bool {
    wait_until(|| {
        Arc::strong_count(sink) == 1
            && control.session_dropped.load(Ordering::SeqCst)
            && control.output_dropped.load(Ordering::SeqCst)
    })
}

#[test]
fn タブを閉じるとそのセッションのスレッドが抜け一覧から消える() {
    let f = ProjectFixture::new(2);
    let a = f.tabs(0)[0].clone();
    let sink = CollectSink::new();
    f.connect(&a, &sink);
    f.controls[0].feed(b"ready");
    assert!(sink.wait_total(5));

    f.registry.close_terminal(&a).expect("閉じられなかった");

    assert!(!f.has_terminal(&a), "写しに残っている");
    assert!(f.tabs(0).is_empty());
    assert_eq!(f.registry.backlog(&a), None, "一覧に残っている");
    assert!(
        f.controls[0].killed.load(Ordering::SeqCst),
        "シェルを終わらせなかった"
    );
    assert!(
        wait_released(&sink, &f.controls[0]),
        "スレッドが抜けないか、資源が残っている"
    );
    assert!(
        lock(&f.snapshots)
            .last()
            .is_some_and(|s| s.terminals.iter().all(|t| t.id != a)),
        "閉じたことを知らせなかった"
    );
    // 閉じたタブには接続できず、シェルも起動しない。
    assert!(matches!(
        f.registry
            .connect(&a, 24, 80, CollectSink::new() as Arc<dyn OutputSink>),
        Err(SessionError::Terminal(TerminalError::NotFound(_)))
    ));
    assert_eq!(f.controls[0].spawn_count(), 1);
    assert_eq!(f.controls[1].spawn_count(), 0);
}

#[test]
fn 起動していないタブも閉じられる() {
    let f = ProjectFixture::new(1);
    let a = f.tabs(0)[0].clone();

    f.registry.close_terminal(&a).expect("閉じられなかった");

    assert!(!f.has_terminal(&a));
    assert!(matches!(
        f.registry.close_terminal(&a),
        Err(SessionError::Terminal(TerminalError::NotFound(_)))
    ));
    assert_eq!(f.controls[0].spawn_count(), 0);
}

#[test]
fn シェルが終わるとタブが閉じセッションが解放される() {
    let f = ProjectFixture::new(2);
    let a = f.tabs(0)[0].clone();
    let b = f.tabs(1)[0].clone();
    let (sink_a, sink_b) = (CollectSink::new(), CollectSink::new());
    f.connect(&a, &sink_a);
    f.connect(&b, &sink_b);

    f.controls[0].set_exit(0);
    f.controls[0].close_output();

    assert!(wait_until(|| !f.has_terminal(&a)), "タブが閉じなかった");
    assert!(f.tabs(0).is_empty());
    assert_eq!(f.registry.backlog(&a), None, "一覧に残っている");
    assert!(
        wait_released(&sink_a, &f.controls[0]),
        "スレッドが抜けないか、資源が残っている"
    );
    assert!(
        wait_until(|| lock(&f.snapshots)
            .last()
            .is_some_and(|s| s.terminals.iter().all(|t| t.id != a))),
        "閉じたことを知らせなかった"
    );
    // もう片方のプロジェクトは巻き込まない(NFR-006)。
    assert_eq!(
        f.registry.snapshot().terminals[0].status,
        TerminalStatus::Running
    );
    f.assert_io(&b, &f.controls[1], &sink_b);
}

#[test]
fn 書き込みが止まったままのタブを閉じても他のコマンドが止まらず_書き込みが戻ったら終わる() {
    let f = ProjectFixture::new(3);
    let a = f.tabs(0)[0].clone();
    let b = f.tabs(1)[0].clone();
    let (sink_a, sink_b) = (CollectSink::new(), CollectSink::new());
    f.connect(&a, &sink_a);
    f.connect(&b, &sink_b);
    f.controls[0].set_hold_writes(true);
    let writer = {
        let registry = f.registry.clone();
        let id = a.clone();
        thread::spawn(move || registry.write(&id, b"paste"))
    };
    assert!(
        wait_until(|| f.controls[0].held.load(Ordering::SeqCst) == 1),
        "書き込みが止まらなかった"
    );

    let closed = {
        let registry = f.registry.clone();
        let id = a.clone();
        within_wait(move || registry.close_terminal(&id))
    };
    assert!(
        matches!(closed, Some(Ok(()))),
        "閉じるのが戻らないか失敗した: {closed:?}"
    );
    assert!(!f.has_terminal(&a));
    assert!(
        !f.controls[0].killed.load(Ordering::SeqCst),
        "書き込み中の PTY を終わらせた"
    );

    // ほかのコマンドは止まらない。
    let reattach = {
        let registry = f.registry.clone();
        let id = b.clone();
        let sink = Arc::clone(&sink_b);
        within_wait(move || registry.connect(&id, 30, 90, sink as Arc<dyn OutputSink>))
    };
    assert!(matches!(reattach, Some(Ok(_))), "{reattach:?}");
    f.assert_io(&b, &f.controls[1], &sink_b);
    let opened = {
        let registry = f.registry.clone();
        within_wait(move || registry.new_terminal())
    };
    assert!(matches!(opened, Some(Ok(_))), "{opened:?}");
    let running = {
        let registry = f.registry.clone();
        within_wait(move || registry.running_terminals())
    };
    assert!(running.is_some(), "動作中の一覧が止まった");

    // 書き込みが戻ったら終わらせる。
    f.controls[0].set_hold_writes(false);
    let _ = writer.join().expect("書き込みのスレッドが panic した");
    assert!(
        wait_until(|| f.controls[0].killed.load(Ordering::SeqCst)),
        "書き込みが戻ってもシェルを終わらせなかった"
    );
    assert!(
        wait_released(&sink_a, &f.controls[0]),
        "スレッドが抜けないか、資源が残っている"
    );
}

#[test]
fn 終了の処理のあとの接続はシェルを起動しない() {
    let f = ProjectFixture::new(2);
    let a = f.tabs(0)[0].clone();

    f.registry.shutdown();
    let result = f
        .registry
        .connect(&a, 24, 80, CollectSink::new() as Arc<dyn OutputSink>);

    assert!(
        matches!(result, Err(SessionError::ShuttingDown)),
        "{result:?}"
    );
    assert!(result.unwrap_err().to_string().contains("終了"));
    assert_eq!(f.controls[0].spawn_count(), 0, "シェルを起動した");
    assert_eq!(f.registry.backlog(&a), None);
}

#[test]
fn 再接続で寸法を反映できなかったら応答でそう返す() {
    let f = ProjectFixture::new(1);
    let a = f.tabs(0)[0].clone();
    let sink = CollectSink::new();

    assert_eq!(
        f.connect(&a, &sink),
        ConnectOutcome { size_applied: true },
        "起動では寸法を反映する"
    );
    let reattached = f
        .registry
        .connect(&a, 30, 100, Arc::clone(&sink) as Arc<dyn OutputSink>)
        .unwrap();
    assert_eq!(reattached, ConnectOutcome { size_applied: true });
    assert_eq!(
        lock(&f.controls[0].sizes).last(),
        Some(&PtySize {
            rows: 30,
            cols: 100
        })
    );

    f.controls[0].set_hold_writes(true);
    let writer = {
        let registry = f.registry.clone();
        let id = a.clone();
        thread::spawn(move || registry.write(&id, b"x"))
    };
    assert!(wait_until(|| f.controls[0].held.load(Ordering::SeqCst) == 1));
    let blocked = {
        let registry = f.registry.clone();
        let id = a.clone();
        let sink = Arc::clone(&sink);
        within_wait(move || registry.connect(&id, 40, 120, sink as Arc<dyn OutputSink>))
    };
    assert_eq!(
        blocked.map(|r| r.map_err(|e| e.to_string())),
        Some(Ok(ConnectOutcome {
            size_applied: false
        }))
    );
    f.controls[0].set_hold_writes(false);
    writer.join().unwrap().unwrap();
    assert_eq!(
        lock(&f.controls[0].sizes).last(),
        Some(&PtySize {
            rows: 30,
            cols: 100
        }),
        "反映していないのに寸法が変わった"
    );
}

#[test]
fn バイト列の入力がそのまま書かれる() {
    let f = ProjectFixture::new(1);
    let a = f.tabs(0)[0].clone();
    f.connect(&a, &CollectSink::new());
    // xterm.js の `onBinary`(古い形式のマウス報告)は UTF-8 にならないバイトを含む。
    let bytes = [0x1b, b'[', b'M', b' ', 0xff, 0x80, 0x00];

    f.registry.write(&a, &bytes).unwrap();

    assert_eq!(lock(&f.controls[0].written).as_slice(), &bytes);
}

#[test]
fn 動作中の一覧は前面がシェル以外のものだけ() {
    let f = ProjectFixture::new(3);
    let a1 = f.tabs(0)[0].clone();
    let a2 = f.registry.new_terminal().unwrap();
    let a3 = f.registry.new_terminal().unwrap(); // 起動しない
    let b = f.tabs(1)[0].clone();
    *lock(&f.controls[0].foreground) = Foreground::Shell;
    *lock(&f.controls[1].foreground) = Foreground::Other;
    *lock(&f.controls[2].foreground) = Foreground::Unknown;
    f.connect(&a1, &CollectSink::new());
    f.connect(&a2, &CollectSink::new());
    f.connect(&b, &CollectSink::new());

    let running = f.registry.running_terminals();

    assert_eq!(
        running
            .iter()
            .map(|t| t.terminal_id.clone())
            .collect::<Vec<_>>(),
        vec![a2.clone()],
        "a3 は起動していない: {a3}"
    );
    assert_eq!(running[0].project_id, Some(project_id(1)));
    assert_eq!(running[0].project_name.as_deref(), Some("alpha"));
    assert_eq!(running[0].label, "ターミナル 2");

    *lock(&f.controls[2].foreground) = Foreground::Other;
    let ids: Vec<TerminalId> = f
        .registry
        .running_terminals()
        .into_iter()
        .map(|t| t.terminal_id)
        .collect();
    assert_eq!(ids, vec![a2, b], "プロジェクトの並び順・タブの順");
}

#[test]
fn _0_枚のプロジェクトに新しいタブを開ける() {
    let f = ProjectFixture::new(1);
    let a = f.tabs(0)[0].clone();
    f.registry.close_terminal(&a).unwrap();
    assert!(f.tabs(0).is_empty());

    let opened = f.registry.new_terminal().expect("開けなかった");

    assert_eq!(f.tabs(0), vec![opened.clone()]);
    assert_eq!(
        f.registry.snapshot().projects[0].active_terminal_id,
        Some(opened.clone())
    );
    assert!(
        lock(&f.snapshots)
            .last()
            .is_some_and(|s| s.terminals.iter().any(|t| t.id == opened)),
        "開いたことを知らせなかった"
    );
    assert_eq!(f.controls[0].spawn_count(), 0, "開いただけで起動した");
    f.connect(&opened, &CollectSink::new());
    assert_eq!(lock(&f.controls[0].spawned)[0].0.cwd(), Path::new(ROOT_A));
}

#[test]
fn プロジェクトを選んでいなければ新しいタブは開けない() {
    let f = Fixture::new(OutputMode::Scripted);

    let result = f.registry.new_terminal();

    assert!(
        matches!(
            result,
            Err(SessionError::Workspace(WorkspaceError::NoProjectSelected))
        ),
        "{result:?}"
    );
    assert_eq!(f.registry.snapshot().terminals.len(), 9);
}

#[test]
fn タブの選択と前後の切替と表示中のタブを閉じる操作() {
    let f = ProjectFixture::new(1);
    let a1 = f.tabs(0)[0].clone();
    let a2 = f.registry.new_terminal().unwrap();
    let active = || f.registry.snapshot().projects[0].active_terminal_id.clone();
    assert_eq!(active(), Some(a2.clone()));

    f.registry.select_terminal(&a1).unwrap();
    assert_eq!(active(), Some(a1.clone()));
    assert!(
        lock(&f.snapshots)
            .last()
            .is_some_and(|s| s.projects[0].active_terminal_id.as_ref() == Some(&a1)),
        "選択を知らせなかった"
    );
    assert!(matches!(
        f.registry.select_terminal(&TerminalId::new("no-such")),
        Err(SessionError::Terminal(TerminalError::NotFound(_)))
    ));

    assert_eq!(f.registry.select_next_terminal(), Some(a2.clone()));
    assert_eq!(
        f.registry.select_next_terminal(),
        Some(a1.clone()),
        "末尾の次は先頭"
    );
    assert_eq!(f.registry.select_previous_terminal(), Some(a2.clone()));

    // 表示中(a2)を閉じると、左隣の a1 が表示中になる。
    assert_eq!(f.registry.close_active_terminal(), Some(a2.clone()));
    assert!(!f.has_terminal(&a2));
    assert_eq!(active(), Some(a1.clone()));
    assert_eq!(f.registry.close_active_terminal(), Some(a1));
    assert_eq!(
        f.registry.close_active_terminal(),
        None,
        "0 枚なら何もしない"
    );
    assert_eq!(f.registry.select_next_terminal(), None);
}
