//! ターミナルのセッション管理(設計書 §1.3 #2・#3、§5.1 の `src-tauri/sessions`、§7.1「ターミナルの入出力」、ADR-004)。
//!
//! ターミナル 1 枚につき PTY 1 組と、次の 2 本のスレッドを持つ。
//!
//! - 読み取りスレッド: PTY の出力を読む(読み取りは出力が来るまで待つ)。未処理の量が上限を超えたら、
//!   処理済みの通知で下限まで減るまで読まない(子プロセスは書き込みで待たされる)
//! - まとめのスレッド: 読んだ出力を短い時間ぶんまとめて送り先(Tauri のチャネル)へ送る。
//!   読み取りは待つ作りなので、時間で区切って送るには別のスレッドが要る。出力の終わりのあとと一定の間隔で
//!   シェルが終わったかを確かめ、終わったら状態に反映して知らせる
//!
//! 規則(未処理の量の計算、止める・再開の判定、まとめの量、ターミナルの状態)は `arin-core` が持ち、ここは
//! スレッドとロックだけを扱う。検証用の記録(`verify_log`。開発ビルドだけ)を渡したときだけ、読み取りスレッドが
//! OSC 52 を走査し、流量制御の状態と処理済みの通知を集計する(T23)。ロックの順序は「接続の直列化 → Workspace → セッションの一覧」で、
//! PTY の操作(起動・書き込み・寸法・終了)と送り先への送信のあいだは Workspace と一覧のロックを持たない。

use std::collections::HashMap;
use std::fmt;
use std::io::{ErrorKind, Read};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError, TryLockError};
use std::thread;
use std::time::{Duration, Instant};

use arin_core::ports::{PtyError, PtyPort, PtySession, SpawnedPty};
use arin_core::terminal::{
    FlowControl, OUTPUT_BATCH_MAX_BYTES, OUTPUT_BATCH_WINDOW_MS, OUTPUT_HIGH_WATER,
    OUTPUT_LOW_WATER, OscScanner, OutputBatch, TerminalError, TerminalId, validate_size,
};
use arin_core::workspace::{AppSnapshot, ConnectPlan, Workspace};

use crate::verify_log::{FlowRecorder, VerifyConfig, VerifyRecord};

/// PTY の出力の送り先(本物は Tauri のチャネル。`commands/terminal.rs`)。
pub trait OutputSink: Send + Sync {
    /// まとめた出力を 1 回で送る。失敗(画面が無いなど)しても、まとめのスレッドは止めない。
    ///
    /// # Errors
    ///
    /// 送れなかったとき。理由の文字列を返す。
    fn send(&self, data: Vec<u8>) -> Result<(), String>;
}

/// 起動するシェルとシステムの言語設定。最初の接続で 1 回だけ取得する。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellDefaults {
    /// ユーザー情報に登録された既定のシェル。
    pub shell: PathBuf,
    /// システムの言語設定の識別子(例 `ja_JP`)。
    pub locale: Option<String>,
}

/// `ShellDefaults` を取得する関数。本物は `dscl`・`defaults` を起動するので、待ちうる場所(接続のコマンド)で呼ぶ。
pub type ShellProbe = Arc<dyn Fn() -> ShellDefaults + Send + Sync>;

/// 状態が変わったときに、新しい状態の写しを受け取る関数(本物はイベントで画面へ送る)。
pub type SnapshotListener = Arc<dyn Fn(AppSnapshot) + Send + Sync>;

/// 流量制御とまとめの設定。既定値は `arin-core` の仮の値(T26 で決める)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionConfig {
    /// 未処理の量の上限(バイト)。
    pub high_water: u64,
    /// 未処理の量の下限(バイト)。
    pub low_water: u64,
    /// 1 回に送る量の目安(バイト)。
    pub batch_max_bytes: usize,
    /// まとめる時間。
    pub batch_window: Duration,
    /// シェルの終了を確かめる間隔。
    pub exit_poll: Duration,
    /// 1 回の読み取りの大きさ(バイト)。
    pub read_buffer: usize,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            high_water: OUTPUT_HIGH_WATER,
            low_water: OUTPUT_LOW_WATER,
            batch_max_bytes: OUTPUT_BATCH_MAX_BYTES,
            batch_window: Duration::from_millis(OUTPUT_BATCH_WINDOW_MS),
            // 設計書 §7.1「スレッドの分担」の見回りと同じ 1 秒。
            exit_poll: Duration::from_secs(1),
            read_buffer: 16 * 1024,
        }
    }
}

/// 未処理の量と、読み取りを止めているか。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backlog {
    /// 未処理の量(バイト)。
    pub unacked: u64,
    /// 読み取りを止めているか。
    pub paused: bool,
}

/// セッションの操作の失敗。画面へは文言(`Display`)で返す。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionError {
    /// 状態の規則で受け付けられない(ID が無い、寸法が 0、起動前、終了後、起動の仕様の誤り)。
    Terminal(TerminalError),
    /// PTY の操作に失敗した。
    Pty(PtyError),
    /// スレッドを起動できなかった。
    Thread(String),
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Terminal(e) => e.fmt(f),
            Self::Pty(e) => e.fmt(f),
            Self::Thread(e) => write!(f, "ターミナルのスレッドを起動できなかった: {e}"),
        }
    }
}

impl std::error::Error for SessionError {}

impl From<TerminalError> for SessionError {
    fn from(e: TerminalError) -> Self {
        Self::Terminal(e)
    }
}

impl From<PtyError> for SessionError {
    fn from(e: PtyError) -> Self {
        Self::Pty(e)
    }
}

/// 全ターミナルのセッション。Tauri の管理状態に登録する。複製しても同じものを指す。
#[derive(Clone)]
pub struct SessionRegistry {
    inner: Arc<Inner>,
}

struct Inner {
    workspace: Mutex<Workspace>,
    pty: Arc<dyn PtyPort>,
    probe: ShellProbe,
    defaults: OnceLock<ShellDefaults>,
    listener: SnapshotListener,
    config: SessionConfig,
    sessions: Mutex<HashMap<TerminalId, Arc<Session>>>,
    /// 接続を 1 つずつ行う(同じターミナルを二重に起動しない)。入力・寸法・通知はこれを取らない。
    connect_serial: Mutex<()>,
    /// 検証用の記録。`None` なら走査も集計もしない(リリースビルド)。
    verify: Option<VerifyConfig>,
}

/// 起動した PTY 1 組。
struct Session {
    /// 入力・寸法・終了の操作。終了の処理で取り出して `None` にする。
    pty: Arc<Mutex<Option<Box<dyn PtySession>>>>,
    output: Arc<OutputState>,
}

/// 読み取りスレッドとまとめのスレッドが共有する、流量制御と送り先。
struct OutputState {
    state: Mutex<OutputInner>,
    /// 読み取りの再開と、終了の処理を読み取りスレッドへ知らせる。
    wake: Condvar,
    /// 検証用の記録(`Inner::verify` の写し)。
    verify: Option<VerifyConfig>,
}

struct OutputInner {
    flow: FlowControl,
    sink: Arc<dyn OutputSink>,
    closed: bool,
    /// 検証用の集計。`verify` があるときだけ持つ。
    recorder: Option<FlowRecorder>,
}

/// 毒されたロックも取る(1 本のスレッドの異常で全体を止めない。NFR-006)。
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl SessionRegistry {
    /// 状態の正本 `workspace` と PTY のポートからセッション管理を作る。検証用の記録はしない。
    #[must_use]
    pub fn new(
        workspace: Workspace,
        pty: Arc<dyn PtyPort>,
        probe: ShellProbe,
        listener: SnapshotListener,
        config: SessionConfig,
    ) -> Self {
        Self::with_verify(workspace, pty, probe, listener, config, None)
    }

    /// `new` に検証用の記録(`verify`)を足して作る。`None` なら `new` と同じ。
    #[must_use]
    pub fn with_verify(
        workspace: Workspace,
        pty: Arc<dyn PtyPort>,
        probe: ShellProbe,
        listener: SnapshotListener,
        config: SessionConfig,
        verify: Option<VerifyConfig>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                workspace: Mutex::new(workspace),
                pty,
                probe,
                defaults: OnceLock::new(),
                listener,
                config,
                sessions: Mutex::new(HashMap::new()),
                connect_serial: Mutex::new(()),
                verify,
            }),
        }
    }

    /// いまの状態の写し。
    #[must_use]
    pub fn snapshot(&self) -> AppSnapshot {
        lock(&self.inner.workspace).snapshot()
    }

    /// 出力の送り先をつなぐ。最初の接続(または起動の失敗のあと)なら `rows` × `cols` の PTY でシェルを起動する。
    /// 起動済みなら送り先を付け替え、未処理の量を捨てて、寸法を伝える(画面の再読み込み)。
    ///
    /// 待つことがある(シェルの取得、PTY の起動)。メインスレッドから呼ばない。
    ///
    /// # Errors
    ///
    /// ID が無い、寸法が 0、シェルが終わっている、起動できないとき。
    pub fn connect(
        &self,
        id: &TerminalId,
        rows: u16,
        cols: u16,
        sink: Arc<dyn OutputSink>,
    ) -> Result<(), SessionError> {
        let size = validate_size(rows, cols)?;
        let inner = &self.inner;
        let _serial = lock(&inner.connect_serial);
        let defaults = inner.defaults.get_or_init(|| (inner.probe)());

        let planned =
            lock(&inner.workspace).plan_connect(id, &defaults.shell, defaults.locale.as_deref());
        let plan = match planned {
            Ok(plan) => plan,
            Err(e @ TerminalError::Launch(_)) => {
                // 状態は Failed になっている。
                self.notify();
                return Err(e.into());
            }
            Err(e) => return Err(e.into()),
        };

        match plan {
            ConnectPlan::Reattach => {
                let session = self
                    .session(id)
                    .ok_or_else(|| TerminalError::NotStarted(id.clone()))?;
                session.output.replace_sink(sink);
                // 接続の直列化のロックを持ったまま PTY のロックを待たない。書き込みが止まったまま
                // (シェルが入力を読まない)PTY のロックを持っていると、ほかの全ターミナルの接続まで止まるため。
                // 取れなければ寸法の変更は諦め、画面が次に送る `resize_terminal` に任せる。
                let mut pty = match session.pty.try_lock() {
                    Ok(pty) => pty,
                    Err(TryLockError::Poisoned(p)) => p.into_inner(),
                    Err(TryLockError::WouldBlock) => return Ok(()),
                };
                if let Some(pty) = pty.as_mut() {
                    pty.resize(size)?;
                }
                Ok(())
            }
            ConnectPlan::Spawn(spec) => match inner.pty.spawn(&spec, size) {
                Ok(spawned) => self.start(id, spawned, sink),
                Err(e) => {
                    let _ = lock(&inner.workspace).mark_failed(id, e.to_string());
                    self.notify();
                    Err(e.into())
                }
            },
        }
    }

    /// 起動した PTY を登録し、状態を `Running` にしてからスレッドを始める
    /// (先にスレッドを始めると、すぐ終わったシェルの終了の反映が `Running` で上書きされる)。
    fn start(
        &self,
        id: &TerminalId,
        spawned: SpawnedPty,
        sink: Arc<dyn OutputSink>,
    ) -> Result<(), SessionError> {
        let inner = &self.inner;
        let SpawnedPty { session, output } = spawned;
        let state = Arc::new(OutputState {
            state: Mutex::new(OutputInner {
                flow: FlowControl::new(inner.config.high_water, inner.config.low_water),
                sink,
                closed: false,
                recorder: inner
                    .verify
                    .as_ref()
                    .map(|v| FlowRecorder::new(id.clone(), (v.clock)())),
            }),
            wake: Condvar::new(),
            verify: inner.verify.clone(),
        });
        let session = Arc::new(Session {
            pty: Arc::new(Mutex::new(Some(session))),
            output: Arc::clone(&state),
        });
        lock(&inner.sessions).insert(id.clone(), Arc::clone(&session));
        lock(&inner.workspace).mark_running(id)?;
        self.notify();

        let (tx, rx) = mpsc::channel();
        let reader = {
            let state = Arc::clone(&state);
            let buffer = inner.config.read_buffer;
            let id = id.clone();
            thread::Builder::new()
                .name(format!("arin-pty-read-{id}"))
                .spawn(move || read_loop(output, &state, &tx, buffer, &id))
        };
        let pump = reader.and_then(|_| {
            let pump = Pump {
                id: id.clone(),
                registry: self.clone(),
                pty: Arc::clone(&session.pty),
                output: state,
                rx,
            };
            thread::Builder::new()
                .name(format!("arin-pty-pump-{id}"))
                .spawn(move || pump.run())
        });
        if let Err(e) = pump {
            // 起こせたスレッドは、終了の処理で読み取りが終わって抜ける。
            self.close(id);
            let _ = lock(&inner.workspace).mark_failed(id, e.to_string());
            self.notify();
            return Err(SessionError::Thread(e.to_string()));
        }
        Ok(())
    }

    /// 入力をそのまま PTY に書き込む。シェルが入力を読まないと待つことがある。メインスレッドから呼ばない。
    ///
    /// 順序の前提: 同じターミナルへの書き込みを並行に呼ぶと、PTY のロック(`std::sync::Mutex`。来た順を保証しない)を
    /// 取った順に書くので、呼んだ順とは限らない。画面(`src/terminal/TerminalManager.ts`)はターミナルごとに、
    /// 前の `write_terminal` の応答(この関数が全部を書き終えて戻ったあと)を待ってから次を送るので、来た順に書かれる。
    /// 寸法(`resize`)も同じく画面が直列に送る。そのため書き込み専用のスレッドは置かない。
    ///
    /// # Errors
    ///
    /// ID が無い、起動前、シェルが終わっている、書き込めないとき。
    pub fn write(&self, id: &TerminalId, data: &[u8]) -> Result<(), SessionError> {
        lock(&self.inner.workspace).check_input(id)?;
        let session = self
            .session(id)
            .ok_or_else(|| TerminalError::Exited(id.clone()))?;
        let mut pty = lock(&session.pty);
        let pty = pty
            .as_mut()
            .ok_or_else(|| TerminalError::Exited(id.clone()))?;
        pty.write(data)?;
        Ok(())
    }

    /// 画面が `bytes` バイトを取り込んだ。下限まで減ったら読み取りを再開する。
    /// 起動前・終了後は何もしない。未処理の量を超える通知は 0 に留める。
    ///
    /// # Errors
    ///
    /// ID が無いとき。
    pub fn ack(&self, id: &TerminalId, bytes: u64) -> Result<(), SessionError> {
        lock(&self.inner.workspace).check_exists(id)?;
        if let Some(session) = self.session(id) {
            session.output.ack(bytes);
        }
        Ok(())
    }

    /// 寸法を変える。シェルが終わっていれば何もしない。
    ///
    /// # Errors
    ///
    /// ID が無い、寸法が 0、起動前、変えられないとき。
    pub fn resize(&self, id: &TerminalId, rows: u16, cols: u16) -> Result<(), SessionError> {
        let size = validate_size(rows, cols)?;
        if !lock(&self.inner.workspace).check_resize(id)? {
            return Ok(());
        }
        let Some(session) = self.session(id) else {
            return Ok(());
        };
        if let Some(pty) = lock(&session.pty).as_mut() {
            pty.resize(size)?;
        }
        Ok(())
    }

    /// 未処理の量と、読み取りを止めているか。セッションが無ければ `None`。
    #[must_use]
    pub fn backlog(&self, id: &TerminalId) -> Option<Backlog> {
        let session = self.session(id)?;
        let state = lock(&session.output.state);
        Some(Backlog {
            unacked: state.flow.unacked(),
            paused: state.flow.is_paused(),
        })
    }

    /// アプリの終了時に呼ぶ。全セッションのシェルを終わらせ、読み取りスレッドを抜けさせる。2 回目以降は何もしない。
    pub fn shutdown(&self) {
        let ids: Vec<TerminalId> = lock(&self.inner.sessions).keys().cloned().collect();
        for id in ids {
            self.close(&id);
        }
    }

    /// 1 つのセッションを一覧から外して終わらせる。PTY を捨てる(最大でおよそ 200ms 待つ)のは一覧のロックの外。
    fn close(&self, id: &TerminalId) {
        let Some(session) = lock(&self.inner.sessions).remove(id) else {
            return;
        };
        session.output.close();
        // 書き込みが止まったまま PTY のロックを持っていると待ち続けてしまうので、取れなければ諦める。
        // その場合もアプリの終了で PTY の親側が閉じ、シェルには SIGHUP が届く。
        let taken = match session.pty.try_lock() {
            Ok(mut pty) => pty.take(),
            Err(TryLockError::Poisoned(p)) => p.into_inner().take(),
            Err(TryLockError::WouldBlock) => None,
        };
        if let Some(mut pty) = taken {
            let _ = pty.kill();
            drop(pty);
        }
    }

    fn session(&self, id: &TerminalId) -> Option<Arc<Session>> {
        lock(&self.inner.sessions).get(id).cloned()
    }

    /// いまの写しを知らせる。Workspace のロックの外で呼ぶ。
    fn notify(&self) {
        let snapshot = self.snapshot();
        (self.inner.listener)(snapshot);
    }
}

impl OutputState {
    /// 読んでよくなるまで待つ。終了の処理が始まっていたら `false`。
    fn wait_until_readable(&self) -> bool {
        let mut state = lock(&self.state);
        while state.flow.is_paused() && !state.closed {
            state = self
                .wake
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
        !state.closed
    }

    fn on_read(&self, n: usize) {
        let mut guard = lock(&self.state);
        let state = &mut *guard;
        state.flow.on_read(n);
        if let Some(recorder) = state.recorder.as_mut() {
            recorder.on_read(n, state.flow.unacked(), state.flow.is_paused());
        }
    }

    fn ack(&self, bytes: u64) {
        let now = self.verify.as_ref().map(|v| (v.clock)());
        let mut guard = lock(&self.state);
        let state = &mut *guard;
        let resumed = state.flow.on_acked(bytes);
        if let (Some(recorder), Some(now)) = (state.recorder.as_mut(), now) {
            recorder.on_ack(bytes, now, state.flow.unacked(), state.flow.is_paused());
        }
        drop(guard);
        if resumed {
            self.wake.notify_all();
        }
    }

    fn replace_sink(&self, sink: Arc<dyn OutputSink>) {
        let mut state = lock(&self.state);
        state.sink = sink;
        state.flow.reset();
        if let Some(recorder) = state.recorder.as_mut() {
            recorder.on_reset();
        }
        drop(state);
        self.wake.notify_all();
    }

    /// 検証用の記録: 間隔に達していれば、まとめを記録の口へ渡す。記録の口へ渡すあいだはロックを持たない。
    fn tick_verify(&self) {
        let Some(verify) = &self.verify else {
            return;
        };
        let now = (verify.clock)();
        let summary = {
            let mut guard = lock(&self.state);
            let state = &mut *guard;
            let (unacked, paused) = (state.flow.unacked(), state.flow.is_paused());
            state
                .recorder
                .as_mut()
                .and_then(|r| r.tick(now, verify.interval, unacked, paused))
        };
        if let Some(summary) = summary {
            verify.sink.record(VerifyRecord::Flow(summary));
        }
    }

    fn close(&self) {
        lock(&self.state).closed = true;
        self.wake.notify_all();
    }

    fn is_closed(&self) -> bool {
        lock(&self.state).closed
    }

    /// 送り先へ送る。送っているあいだはロックを持たない。
    fn send(&self, data: Vec<u8>) {
        if data.is_empty() {
            return;
        }
        let sink = Arc::clone(&lock(&self.state).sink);
        // 画面が無いなどで送れなくても続ける。画面が通知を返さないので、やがて読み取りが止まる。
        let _ = sink.send(data);
    }
}

/// 読み取りスレッドの本体。出力の終わりかエラーで抜け、`tx` を閉じてまとめのスレッドに終わりを伝える。
fn read_loop(
    mut output: Box<dyn Read + Send>,
    state: &OutputState,
    tx: &Sender<Vec<u8>>,
    buffer: usize,
    id: &TerminalId,
) {
    let mut buf = vec![0u8; buffer.max(1)];
    // 検証用の記録があるときだけ OSC 52 を走査する。走査器はこのスレッドだけが持つ(ロックを取らない)。
    let mut clipboard = state.verify.as_ref().map(|v| ClipboardCounter {
        scanner: OscScanner::new(),
        total: 0,
        verify: v.clone(),
    });
    while state.wait_until_readable() {
        match output.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                state.on_read(n);
                if let Some(counter) = clipboard.as_mut() {
                    counter.scan(id, &buf[..n]);
                }
                if tx.send(buf[..n].to_vec()).is_err() {
                    break;
                }
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
}

/// 検証用の記録: OSC 52 の回数を数えて記録の口へ渡す。本文は渡さない。
struct ClipboardCounter {
    scanner: OscScanner,
    total: u64,
    verify: VerifyConfig,
}

impl ClipboardCounter {
    fn scan(&mut self, id: &TerminalId, data: &[u8]) {
        let seen = self.scanner.count_clipboard(data);
        if seen == 0 {
            return;
        }
        self.total = self.total.saturating_add(seen);
        self.verify.sink.record(VerifyRecord::Clipboard {
            terminal: id.clone(),
            seen,
            total: self.total,
        });
    }
}

/// まとめのスレッド。
struct Pump {
    id: TerminalId,
    registry: SessionRegistry,
    pty: Arc<Mutex<Option<Box<dyn PtySession>>>>,
    output: Arc<OutputState>,
    rx: Receiver<Vec<u8>>,
}

impl Pump {
    fn run(self) {
        let config = self.registry.inner.config.clone();
        let mut batch = OutputBatch::new(config.batch_max_bytes);
        let mut last_poll = Instant::now();
        let mut exited = false;
        let mut ended = false;

        while !ended {
            match self.rx.recv_timeout(config.exit_poll) {
                Ok(chunk) => {
                    batch.push(&chunk);
                    ended = self.gather(&mut batch, config.batch_window);
                    self.output.send(batch.take());
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => ended = true,
            }
            // 出力が無くても見回りの間隔で起きるので、止まったままのときもまとめを出せる。
            self.output.tick_verify();
            if !exited && (ended || last_poll.elapsed() >= config.exit_poll) {
                exited = self.check_exit();
                last_poll = Instant::now();
            }
        }

        // 出力が終わってもシェルがまだ終わっていなければ、終わるか終了の処理が始まるまで見回る。
        while !exited && !self.output.is_closed() {
            thread::sleep(config.exit_poll);
            exited = self.check_exit();
        }
    }

    /// 最初の出力から `window` のあいだ、満杯になるまで続きを集める。出力が終わったら `true`。
    fn gather(&self, batch: &mut OutputBatch, window: Duration) -> bool {
        let deadline = Instant::now() + window;
        while !batch.is_full() {
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                break;
            };
            match self.rx.recv_timeout(left) {
                Ok(chunk) => batch.push(&chunk),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return true,
            }
        }
        false
    }

    /// シェルが終わったかを確かめ、終わっていれば状態に反映して知らせる。見回りを終えてよければ `true`。
    ///
    /// 書き込みが PTY のロックを持っているときは待たずに次の機会に回す。
    fn check_exit(&self) -> bool {
        let status = {
            let mut pty = match self.pty.try_lock() {
                Ok(pty) => pty,
                Err(TryLockError::Poisoned(p)) => p.into_inner(),
                Err(TryLockError::WouldBlock) => return false,
            };
            let Some(pty) = pty.as_mut() else {
                // 終了の処理で取り出された。
                return true;
            };
            match pty.try_wait() {
                Ok(Some(exit)) => exit,
                Ok(None) | Err(_) => return false,
            }
        };
        let changed = lock(&self.registry.inner.workspace).mark_exited(&self.id, Some(status));
        if changed {
            self.registry.notify();
        }
        true
    }
}
