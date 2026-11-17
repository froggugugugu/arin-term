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
//! OSC 52 を走査し、流量制御の状態と処理済みの通知を集計する(T23)。ロックの順序は「接続の直列化 → 写しの直列化 → Workspace → セッションの一覧」で、
//! PTY の操作(起動・書き込み・寸法・終了)と送り先への送信のあいだは Workspace と一覧のロックを持たない。
//!
//! アテンション(FR-055。計画書 T48c): 読み取りスレッドは検証用の記録の有無によらず、自分だけが持つ `AttentionScanner` で
//! 出力を走査する(ロックを取らない)。契機があったチャンクのあとにだけ Workspace のロックを短く取って
//! `Workspace::note_attention` を呼び、印が立ったときだけ、ロックを手放してから写しを送る。出力を閉じたあと
//! (タブを閉じた・アプリの終了)の契機では何もしない。
//!
//! セッションの後片付け(T33b): タブを閉じたとき(`close_terminal`)とシェルが終わってタブが閉じたとき
//! (`Workspace::mark_exited` が閉じたタブを返したとき)に、セッションを一覧から外し、出力を閉じ(読み取りとまとめのスレッドが抜ける)、シェルを終わらせる。
//! 書き込みが止まっていて PTY のロックが取れなければ印(`PtyCell::closing`)だけを付け、ロックを持っていた側が
//! 手放した時点で終わらせる。終了の処理(`shutdown`)が始まったあとの接続はシェルを起動しない。
//! 複数のセッションを一度に終わらせるとき(アプリの終了、プロジェクトの削除)は、同時に終了を送って待ちを 1 回ぶんにする(T33e)。

use std::collections::HashMap;
use std::fmt;
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError, TryLockError};
use std::thread;
use std::time::{Duration, Instant};

use arin_core::editor::{EditorError, EditorId, EditorTab, FileStamp, ReadOnlyReason};
use arin_core::layout::font_sizes::FontSizeTarget;
use arin_core::layout::tiles::DisplayMode;
use arin_core::layout::translucency::TranslucencyTarget;
use arin_core::paths::RelPath;
use arin_core::persistence::codec::PersistedState;
use arin_core::ports::{Foreground, PtyError, PtyPort, PtySession, SpawnedPty};
use arin_core::project::{ProjectColor, ProjectError, ProjectId, RootReference};
use arin_core::terminal::{
    AttentionScanner, FlowControl, OUTPUT_BATCH_MAX_BYTES, OUTPUT_BATCH_WINDOW_MS,
    OUTPUT_HIGH_WATER, OUTPUT_LOW_WATER, OscScanner, OutputBatch, TerminalError, TerminalId,
    validate_size,
};
use arin_core::workspace::{
    ActiveTab, AddProjectOutcome, AppSnapshot, CloseEditorOutcome, ConnectOutcome, ConnectPlan,
    ProjectRoot, RemoveOutcome, RemoveProjectOutcome, RunningTerminal, UnsavedEditor, Workspace,
    WorkspaceError,
};

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
    /// タブを開けない(プロジェクトを選んでいない、ルートが見つからない)。
    Workspace(WorkspaceError),
    /// その ID のエディタが無い(ADR-015 の 4)。
    Editor(EditorError),
    /// アプリの終了の処理が始まっているので、シェルを起動しない。
    ShuttingDown,
    /// PTY の操作に失敗した。
    Pty(PtyError),
    /// スレッドを起動できなかった。
    Thread(String),
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Terminal(e) => e.fmt(f),
            Self::Workspace(e) => e.fmt(f),
            Self::Editor(e) => e.fmt(f),
            Self::ShuttingDown => f.write_str("アプリを終了しているので、ターミナルを起動しない"),
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

impl From<WorkspaceError> for SessionError {
    fn from(e: WorkspaceError) -> Self {
        Self::Workspace(e)
    }
}

impl From<EditorError> for SessionError {
    fn from(e: EditorError) -> Self {
        Self::Editor(e)
    }
}

impl From<ProjectError> for SessionError {
    fn from(e: ProjectError) -> Self {
        Self::Workspace(WorkspaceError::Project(e))
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
    /// 写しを 1 本ずつ送る(`notify`)。写しを作ってから受け口を呼び終えるまで持つので、受け口に届く写しは
    /// Workspace が変わった順に並ぶ(古い写しがあとから届いて、画面が古い状態に戻ることが無い)。
    notify_serial: Mutex<()>,
    /// 検証用の記録。`None` なら走査も集計もしない(リリースビルド)。
    verify: Option<VerifyConfig>,
    /// 終了の処理が始まった。セッションの一覧のロックを持って読み書きする(接続の登録と終了の処理の取りこぼしを防ぐ)。
    shutting_down: AtomicBool,
}

/// 起動した PTY 1 組。
struct Session {
    pty: Arc<PtyCell>,
    output: Arc<OutputState>,
}

/// 入力・寸法・終了の操作と、終わらせる印。セッションとまとめのスレッドが共有する。
///
/// PTY の操作はすべて `with`・`try_with` を通す。どちらもロックを手放したあとに印を確かめ、付いていれば終わらせるので、
/// 書き込みが止まっているあいだに閉じたタブも、書き込みが戻った時点で終わる。
struct PtyCell {
    /// 入力・寸法・終了の操作。終わらせるときに取り出して `None` にする。
    pty: Mutex<Option<Box<dyn PtySession>>>,
    /// 一覧から外した(タブを閉じた・アプリの終了)。以後の操作はせず、ロックが取れた時点で終わらせる。
    closing: AtomicBool,
}

/// PTY のロックをほかの操作(止まった書き込みなど)が持っている。
struct Busy;

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
                notify_serial: Mutex::new(()),
                verify,
                shutting_down: AtomicBool::new(false),
            }),
        }
    }

    /// いまの状態の写し。
    #[must_use]
    pub fn snapshot(&self) -> AppSnapshot {
        lock(&self.inner.workspace).snapshot()
    }

    /// 状態ファイルに保存する内容(`Workspace::persisted_state`)。
    #[must_use]
    pub fn persisted_state(&self) -> PersistedState {
        lock(&self.inner.workspace).persisted_state()
    }

    /// 出力の送り先をつなぐ。最初の接続(または起動の失敗のあと)なら `rows` × `cols` の PTY でシェルを起動する。
    /// 起動済みなら送り先を付け替え、未処理の量を捨てて、寸法を伝える(画面の再読み込み)。
    /// 応答の `size_applied` は寸法を PTY に反映したか(再接続で PTY が塞がっていたら `false`。画面が送り直す)。
    ///
    /// 待つことがある(シェルの取得、PTY の起動)。メインスレッドから呼ばない。
    ///
    /// # Errors
    ///
    /// ID が無い、寸法が 0、シェルが終わっている、起動できない、終了の処理が始まっているとき。
    pub fn connect(
        &self,
        id: &TerminalId,
        rows: u16,
        cols: u16,
        sink: Arc<dyn OutputSink>,
    ) -> Result<ConnectOutcome, SessionError> {
        let size = validate_size(rows, cols)?;
        let inner = &self.inner;
        if self.is_shutting_down() {
            return Err(SessionError::ShuttingDown);
        }
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
                // 取れなければ寸法の変更は諦め、反映していないと応答で返す(画面が `resize_terminal` で送り直す)。
                let size_applied = match session.pty.try_with(|pty| pty.resize(size)) {
                    Ok(Some(result)) => {
                        result?;
                        true
                    }
                    Ok(None) | Err(Busy) => false,
                };
                Ok(ConnectOutcome { size_applied })
            }
            ConnectPlan::Spawn(spec) => match inner.pty.spawn(&spec, size) {
                Ok(spawned) => self
                    .start(id, spawned, sink)
                    .map(|()| ConnectOutcome { size_applied: true }),
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
            pty: Arc::new(PtyCell {
                pty: Mutex::new(Some(session)),
                closing: AtomicBool::new(false),
            }),
            output: Arc::clone(&state),
        });
        {
            // 終了の処理は一覧のロックを持って印を付けてから全部を取り出すので、ここで印が無ければ必ず後片付けに載る。
            let mut sessions = lock(&inner.sessions);
            if self.is_shutting_down() {
                drop(sessions);
                end_session(&session);
                return Err(SessionError::ShuttingDown);
            }
            sessions.insert(id.clone(), Arc::clone(&session));
        }
        if let Err(e) = lock(&inner.workspace).mark_running(id) {
            // 起動しているあいだにタブが閉じられた。
            self.close(id);
            return Err(e.into());
        }
        self.notify();

        let (tx, rx) = mpsc::channel();
        let reader = {
            let state = Arc::clone(&state);
            let buffer = inner.config.read_buffer;
            let id = id.clone();
            let mut attention = AttentionWatch {
                scanner: AttentionScanner::new(),
                registry: self.clone(),
            };
            thread::Builder::new()
                .name(format!("arin-pty-read-{id}"))
                .spawn(move || read_loop(output, &state, &tx, buffer, &id, &mut attention))
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
        session
            .pty
            .with(|pty| pty.write(data))
            .ok_or_else(|| TerminalError::Exited(id.clone()))??;
        Ok(())
    }

    /// 画面が `bytes` バイトを取り込んだ。下限まで減ったら読み取りを再開する。
    /// 起動前・終了後は何もしない。未処理の量を超える通知は 0 に留める。
    /// ID が無いときも何もしない: シェルが終わる・タブを閉じると、最後の出力を画面が取り込み終える前に
    /// タブが一覧から消えるので、遅れて届く通知は普通に起きる(失敗として画面に返すと、診断に毎回出てしまう)。
    ///
    /// # Errors
    ///
    /// いまは失敗しない(コマンドの形をほかとそろえるため `Result` のまま)。
    pub fn ack(&self, id: &TerminalId, bytes: u64) -> Result<(), SessionError> {
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
        if let Some(result) = session.pty.with(|pty| pty.resize(size)) {
            result?;
        }
        Ok(())
    }

    /// 選択中のプロジェクトの末尾に新しいタブを開いて表示中にし、その ID を返す(FR-039・FR-044。⌘T のメニューも呼ぶ)。
    /// シェルは画面が接続したときに起動する。cwd はプロジェクトのルート。0 枚のプロジェクトにも開ける。
    ///
    /// # Errors
    ///
    /// プロジェクトを選んでいないとき、ルートが見つからないとき。
    pub fn new_terminal(&self) -> Result<TerminalId, SessionError> {
        let id = lock(&self.inner.workspace).open_terminal_in_selected()?;
        self.notify();
        Ok(id)
    }

    /// タブを閉じ、そのセッションを終わらせる(FR-044)。動作中のプロセスがあっても確認しない(計画書 M2 の【仮定】14)。
    /// 表示中だったなら右隣(無ければ左)が表示中になる。
    ///
    /// 書き込みが止まっていて PTY のロックが取れなくても待たない(印を付け、書き込みが戻った時点で終わらせる)。
    /// シェルを終わらせるのに最大でおよそ 200ms 待つことがある。メインスレッドから呼ばない。
    ///
    /// # Errors
    ///
    /// その ID のタブが無いとき。
    pub fn close_terminal(&self, id: &TerminalId) -> Result<(), SessionError> {
        lock(&self.inner.workspace).close_terminal(id)?;
        let session = self.detach(id);
        self.notify();
        if let Some(session) = session {
            end_session(&session);
        }
        Ok(())
    }

    /// 選択中のプロジェクトの表示中のタブを閉じ、閉じたタブを返す(⌘W のメニュー)。
    /// 0 枚なら何もせず `None`。待つことは `close_terminal` と同じ。
    pub fn close_active_terminal(&self) -> Option<TerminalId> {
        let id = lock(&self.inner.workspace).active_terminal()?;
        self.close_terminal(&id).ok().map(|()| id)
    }

    /// そのタブを表示中にする(FR-044)。セッションには触れない(FR-042)。
    ///
    /// # Errors
    ///
    /// その ID のタブが無いとき。
    pub fn select_terminal(&self, id: &TerminalId) -> Result<(), SessionError> {
        lock(&self.inner.workspace).activate_terminal(id)?;
        self.notify();
        Ok(())
    }

    /// 選択中のプロジェクトの次のタブを表示中にし、その ID を返す(⌘⇧] のメニュー)。末尾の次は先頭。0 枚なら `None`。
    pub fn select_next_terminal(&self) -> Option<TerminalId> {
        let id = lock(&self.inner.workspace).select_next_terminal()?;
        self.notify();
        Some(id)
    }

    /// 選択中のプロジェクトの前のタブを表示中にし、その ID を返す(⌘⇧[ のメニュー)。先頭の前は末尾。0 枚なら `None`。
    pub fn select_previous_terminal(&self) -> Option<TerminalId> {
        let id = lock(&self.inner.workspace).select_previous_terminal()?;
        self.notify();
        Some(id)
    }

    /// 動作中のターミナル(前面がシェル以外。`Foreground::Other`)を、プロジェクト名とタブ名つきで返す
    /// (FR-003・FR-063 の確認の材料。プロジェクトの削除=T33c とアプリの終了=T33e が使う)。
    ///
    /// 待たない。前面を調べられないもの(`Foreground::Unknown`)と、書き込みが止まっていて PTY のロックが取れないものは
    /// 動作中に数えない。順はプロジェクトの並び順・タブの順。
    #[must_use]
    pub fn running_terminals(&self) -> Vec<RunningTerminal> {
        let sessions: Vec<(TerminalId, Arc<Session>)> = lock(&self.inner.sessions)
            .iter()
            .map(|(id, session)| (id.clone(), Arc::clone(session)))
            .collect();
        let running: Vec<TerminalId> = sessions
            .into_iter()
            .filter(|(_, session)| {
                matches!(
                    session.pty.try_with(|pty| pty.foreground()),
                    Ok(Some(Foreground::Other))
                )
            })
            .map(|(id, _)| id)
            .collect();
        lock(&self.inner.workspace).running_terminals(&running)
    }

    /// フォルダをプロジェクトとして登録し、選んでタブを 1 枚開く(FR-001。T33c)。同じフォルダが登録済みなら増やさず
    /// それを選ぶ(FR-010)。`root` のパスは正規化したもの、`id` は乱数から作ったものを渡す。
    ///
    /// # Errors
    ///
    /// パスが使えないとき、ゴミ箱の中のとき、同じ ID があるとき(`ProjectError::DuplicateId`。呼び出し側が取り直す)。
    pub fn register_project(
        &self,
        id: ProjectId,
        root: RootReference,
    ) -> Result<AddProjectOutcome, SessionError> {
        let outcome = lock(&self.inner.workspace).register_project(id, root)?;
        self.notify();
        Ok(outcome)
    }

    /// プロジェクトを選ぶ(FR-002)。ターミナルとセッションには触れない(FR-042)。
    ///
    /// # Errors
    ///
    /// その ID のプロジェクトが無いとき。
    pub fn select_project(&self, id: &ProjectId) -> Result<(), SessionError> {
        lock(&self.inner.workspace).select_project(id)?;
        self.notify();
        Ok(())
    }

    /// 色を変える(FR-059)。
    ///
    /// # Errors
    ///
    /// その ID のプロジェクトが無いとき。
    pub fn set_project_color(
        &self,
        id: &ProjectId,
        color: ProjectColor,
    ) -> Result<(), SessionError> {
        lock(&self.inner.workspace).set_project_color(id, color)?;
        self.notify();
        Ok(())
    }

    /// ツリーの隠しファイル表示を切り替える(FR-017。計画書 T42a)。写しを送り、保存が予約される。
    ///
    /// # Errors
    ///
    /// その ID のプロジェクトが無いとき。
    pub fn set_project_show_hidden(
        &self,
        id: &ProjectId,
        show_hidden: bool,
    ) -> Result<(), SessionError> {
        lock(&self.inner.workspace).set_project_show_hidden(id, show_hidden)?;
        self.notify();
        Ok(())
    }

    /// 本人が選んだフォルダにつなぎ直す(FR-007)。タブは開かない。
    ///
    /// # Errors
    ///
    /// その ID が無いとき、パスが使えないとき、別のプロジェクトのフォルダと同じとき。何も変えない。
    pub fn relink_project(&self, id: &ProjectId, root: RootReference) -> Result<(), SessionError> {
        lock(&self.inner.workspace).relink_project(id, root)?;
        self.notify();
        Ok(())
    }

    /// ルートの絶対パス(FR-064 のコピー用)。選択は変えない。
    ///
    /// # Errors
    ///
    /// その ID のプロジェクトが無いとき。
    pub fn project_root_path(&self, id: &ProjectId) -> Result<PathBuf, SessionError> {
        Ok(lock(&self.inner.workspace).project_root_path(id)?)
    }

    /// プロジェクトを一覧から外す(FR-003。フォルダには触れない)。そのプロジェクトに動作中のターミナル
    /// (`running_terminals`)があり `confirmed` が偽なら、何も変えずに確認を求める。外したらそのタブのセッションを終わらせる。
    ///
    /// シェルを終わらせるのに最大でおよそ 200ms 待つことがある(複数のタブは同時に終わらせる)。メインスレッドから呼ばない。
    ///
    /// # Errors
    ///
    /// その ID のプロジェクトが無いとき。
    pub fn remove_project(
        &self,
        id: &ProjectId,
        confirmed: bool,
    ) -> Result<RemoveProjectOutcome, SessionError> {
        let running: Vec<TerminalId> = self
            .running_terminals()
            .into_iter()
            .map(|t| t.terminal_id)
            .collect();
        let outcome = lock(&self.inner.workspace).remove_project(id, &running, confirmed)?;
        let reply = RemoveProjectOutcome::from(&outcome);
        if let RemoveOutcome::Removed { terminals } = outcome {
            self.end_sessions(&terminals);
        }
        Ok(reply)
    }

    /// 見回りの対象(全プロジェクトのルートと、見つからないかどうか。T33d)。
    #[must_use]
    pub fn project_roots(&self) -> Vec<ProjectRoot> {
        lock(&self.inner.workspace).project_roots()
    }

    /// 見回りで分かったいまの位置に追従する(FR-007。`Workspace::follow_project_root`)。変わったときだけ写しを送る。
    /// `expected` は調べたときのルートのパス(調べているあいだに再リンクされていたら何もしない)。
    ///
    /// # Errors
    ///
    /// その ID が無いとき、別のプロジェクトのフォルダと同じときなど。何も変えない。
    pub fn follow_project_root(
        &self,
        id: &ProjectId,
        expected: &Path,
        root: RootReference,
    ) -> Result<bool, SessionError> {
        let changed = lock(&self.inner.workspace).follow_project_root(id, expected, root)?;
        if changed {
            self.notify();
        }
        Ok(changed)
    }

    /// 見回りでルートが取れなくなったプロジェクトを見つからないにする(FR-007。カードとタブは残す)。
    /// 変わったときだけ写しを送る。
    ///
    /// # Errors
    ///
    /// その ID が無いとき。
    pub fn mark_project_missing(
        &self,
        id: &ProjectId,
        expected: &Path,
    ) -> Result<bool, SessionError> {
        let changed = lock(&self.inner.workspace).mark_project_missing(id, expected)?;
        if changed {
            self.notify();
        }
        Ok(changed)
    }

    /// 左ペインの幅とカード領域の高さを変える(FR-009。不正な値は `PaneSizes` の規則で直す)。変わったときだけ写しを送る
    /// (写しを送ると保存が予約される)。
    pub fn set_pane_sizes(&self, sidebar_width: f64, card_area_height: f64) {
        let changed = lock(&self.inner.workspace).set_pane_sizes(sidebar_width, card_area_height);
        if changed {
            self.notify();
        }
    }

    /// 左ペインを折りたたむ・戻す(FR-009)。変わったときだけ写しを送る。
    pub fn set_sidebar_collapsed(&self, collapsed: bool) {
        let changed = lock(&self.inner.workspace).set_sidebar_collapsed(collapsed);
        if changed {
            self.notify();
        }
    }

    /// 右ペインの表示モードを変える(FR-048。計画書 T47b)。変わったときだけ写しを送り `true` を返す
    /// (写しを送ると保存が予約され、状態ファイルの `ui.displayMode` が書かれる)。タブ表示にすると拡大は解ける。
    pub fn set_display_mode(&self, mode: DisplayMode) -> bool {
        let changed = lock(&self.inner.workspace).set_display_mode(mode);
        if changed {
            self.notify();
        }
        changed
    }

    /// 表示モードをもう一方にする(メニューバーの「表示」の切替。T47c が使う)。必ず変わるので写しを送り `true` を返す。
    pub fn toggle_display_mode(&self) -> bool {
        let changed = lock(&self.inner.workspace).toggle_display_mode();
        if changed {
            self.notify();
        }
        changed
    }

    /// 文字の大きさを `delta` だけ動かす(ADR-013 の 2。計画書 T68c)。範囲の端で留め、変わったときだけ写しを送り `true` を返す
    /// (写しを送ると保存が予約され、状態ファイルの `ui.terminalFontSize`・`ui.interfaceFontSize` が書かれる)。
    /// 画面が「今の値 + 1」を送る形にしないのは、写しが届く前に続けて押したときに回数どおり上がるようにするため
    /// (ロックの中で順に足す)。シェルには触れない(寸法は画面が測り直して `resize` で送る)。
    pub fn step_font_size(&self, target: FontSizeTarget, delta: i32) -> bool {
        let changed = lock(&self.inner.workspace).step_font_size(target, delta);
        if changed {
            self.notify();
        }
        changed
    }

    /// 透過を `delta`(% の差分)だけ動かす(ADR-014 の 1)。近い 5 の倍数に丸めて範囲の端で留め、変わったときだけ写しを送り
    /// `true` を返す(写しを送ると保存が予約され、状態ファイルの `ui.windowTranslucency`・`ui.terminalTranslucency` が書かれる)。
    /// 差分で受ける理由は `step_font_size` と同じ。シェルには触れない(画面が CSS の変数を替えるだけ)。
    pub fn step_translucency(&self, target: TranslucencyTarget, delta: i32) -> bool {
        let changed = lock(&self.inner.workspace).step_translucency(target, delta);
        if changed {
            self.notify();
        }
        changed
    }

    /// 文字の大きさ 2 つと透過 2 つを既定に戻す(ADR-014 の 1・AC-G05。ADR-013 の `reset_font_sizes` を替えたもの)。
    /// 変わったときだけ写しを送り `true` を返す(4 つとも既定のときは何もしない)。
    pub fn reset_appearance(&self) -> bool {
        let changed = lock(&self.inner.workspace).reset_appearance();
        if changed {
            self.notify();
        }
        changed
    }

    /// タイルにフォーカスする(FR-053、AC-M15)。そのタブのプロジェクトを選び、そのタブを表示中にする。
    /// 選択か表示中のタブが変わったときだけ写しを送り `true` を返す(プロジェクトが替わればツリーの監視は写しで張り替わる)。
    ///
    /// その ID のタブが無いとき(持ち主のプロジェクトが無いタブを含む)は、何も変えず写しも送らず `false` を返す
    /// (失敗にしない。M4 のレビューの S6)。シェルが終わってタブが閉じるのと同時にそのタイルをクリックすると、
    /// 閉じたタイルへのフォーカスは普通に起きる。失敗として画面に返すと診断に出てしまう(`ack` と同じ扱い)。
    ///
    /// # Errors
    ///
    /// いまは失敗しない(コマンドの形をほかとそろえるため `Result` のまま)。
    pub fn focus_terminal(&self, id: &TerminalId) -> Result<bool, SessionError> {
        let focused = lock(&self.inner.workspace).focus_terminal(id);
        let changed = ignore_closed(focused)?;
        if changed {
            self.notify();
        }
        Ok(changed)
    }

    /// 次のページへ送り、その先頭のタイルにフォーカスする(FR-052)。送れたときだけ写しを送り `true` を返す。
    /// タブ表示・拡大中・最後のページでは何もしない。
    pub fn next_tile_page(&self) -> bool {
        let changed = lock(&self.inner.workspace).next_page();
        if changed {
            self.notify();
        }
        changed
    }

    /// 前のページへ送り、その先頭のタイルにフォーカスする(FR-052)。送れたときだけ写しを送り `true` を返す。
    /// タブ表示・拡大中・最初のページでは何もしない。
    pub fn previous_tile_page(&self) -> bool {
        let changed = lock(&self.inner.workspace).previous_page();
        if changed {
            self.notify();
        }
        changed
    }

    /// 表示中のページの `position` 番目(1 始まり)のタイルにフォーカスする(⌘1〜⌘9。ADR-014 の 4)。
    /// 移ったときだけ写しを送り `true` を返す(プロジェクトが替わればツリーの監視は写しで張り替わる)。
    /// タブ表示・拡大中・その番号のタイルが無いときは何もしない。
    pub fn focus_tile_at(&self, position: usize) -> bool {
        let changed = lock(&self.inner.workspace).focus_tile_at(position);
        if changed {
            self.notify();
        }
        changed
    }

    /// フォーカス中のタイルを拡大する、または拡大を解く(FR-054)。変わったときだけ写しを送り `true` を返す。
    /// タブ表示のとき・フォーカス中のタイルが無いときは何もしない。
    pub fn toggle_tile_maximize(&self) -> bool {
        let changed = lock(&self.inner.workspace).toggle_maximize();
        if changed {
            self.notify();
        }
        changed
    }

    /// 渡したタイルにフォーカスしてから、拡大する・拡大を解く(タイトルバーのダブルクリック。FR-054、M4 のレビューの S3)。
    /// 1 回のロックの中で行うので、フォーカスのコマンドの失敗や届く順の前後で別のタイルを拡大しない。
    /// 変わったときだけ写しを送り `true` を返す。タブ表示では何もしない。
    /// その ID のタブが無いときは `focus_terminal` と同じく、何も変えず `false` を返す(失敗にしない。S6)。
    ///
    /// # Errors
    ///
    /// いまは失敗しない(コマンドの形をほかとそろえるため `Result` のまま)。
    pub fn toggle_tile_maximize_of(&self, id: &TerminalId) -> Result<bool, SessionError> {
        let toggled = lock(&self.inner.workspace).toggle_maximize_of(id);
        let changed = ignore_closed(toggled)?;
        if changed {
            self.notify();
        }
        Ok(changed)
    }

    /// 選択中のプロジェクトでいま表示しているタブを閉じる(⌘W のメニュー。ADR-015 の 4)。
    /// 表示中がエディタなら、未保存の変更が無ければ閉じ、あれば閉じずに `EditorNeedsConfirmation`
    /// (呼び出し側が `editor-close-requested` を送る)。表示中がターミナルなら `close_terminal` と同じ。
    /// タブが無ければ何もせず `None`。待つことは `close_terminal` と同じ(メインスレッドから呼ばない)。
    pub fn close_active_tab(&self) -> Option<ClosedTab> {
        let active = lock(&self.inner.workspace).active_tab()?;
        match active {
            ActiveTab::Terminal(id) => self
                .close_terminal(&id)
                .ok()
                .map(|()| ClosedTab::Terminal(id)),
            ActiveTab::Editor(id) => match self.close_editor(&id, false).ok()? {
                CloseEditorOutcome::Closed => Some(ClosedTab::Editor(id)),
                CloseEditorOutcome::NeedsConfirmation => {
                    Some(ClosedTab::EditorNeedsConfirmation(id))
                }
            },
        }
    }

    /// 選択中のプロジェクトの次のタブを表示し、表示したタブを返す(⌘⇧] のメニュー)。タブ表示ではターミナル → エディタの順で
    /// 1 周し、タイル表示ではターミナルだけを回る(ADR-015 の 3)。タブが無ければ `None`。
    pub fn select_next_tab(&self) -> Option<ActiveTab> {
        let tab = lock(&self.inner.workspace).select_next_tab()?;
        self.notify();
        Some(tab)
    }

    /// 選択中のプロジェクトの前のタブを表示し、表示したタブを返す(⌘⇧[ のメニュー。`select_next_tab` の逆順)。
    pub fn select_previous_tab(&self) -> Option<ActiveTab> {
        let tab = lock(&self.inner.workspace).select_previous_tab()?;
        self.notify();
        Some(tab)
    }

    /// `project` の `path` をエディタで開いて表示する(ADR-015 の 3)。新しく作ったら `(ID, true)`、既に開いていれば
    /// それを表示して `(ID, false)`。写しを送る。`stamp`・`read_only` は呼び出し側が読んだときのもの。
    ///
    /// # Errors
    ///
    /// プロジェクトが無い・ルートが見つからない・Markdown でないとき。何も変えない。
    pub fn open_editor(
        &self,
        project: &ProjectId,
        path: RelPath,
        stamp: FileStamp,
        read_only: Option<ReadOnlyReason>,
    ) -> Result<(EditorId, bool), SessionError> {
        let opened = lock(&self.inner.workspace).open_editor(project, path, stamp, read_only)?;
        self.notify();
        Ok(opened)
    }

    /// エディタのタブを表示する(タブバーのクリック)。写しを送る。
    ///
    /// # Errors
    ///
    /// その ID のエディタが無いとき。
    pub fn select_editor(&self, id: &EditorId) -> Result<(), SessionError> {
        lock(&self.inner.workspace).select_editor(id)?;
        self.notify();
        Ok(())
    }

    /// エディタを閉じる。未保存の変更があり `discard` が偽なら閉じずに `NeedsConfirmation`(写しは送らない)。
    ///
    /// # Errors
    ///
    /// その ID のエディタが無いとき。
    pub fn close_editor(
        &self,
        id: &EditorId,
        discard: bool,
    ) -> Result<CloseEditorOutcome, SessionError> {
        let outcome = lock(&self.inner.workspace).close_editor(id, discard)?;
        if outcome == CloseEditorOutcome::Closed {
            self.notify();
        }
        Ok(outcome)
    }

    /// 未保存の印を替える。変わったときだけ写しを送り `true` を返す。
    ///
    /// # Errors
    ///
    /// その ID のエディタが無いとき。
    pub fn set_editor_dirty(&self, id: &EditorId, dirty: bool) -> Result<bool, SessionError> {
        let changed = lock(&self.inner.workspace).set_editor_dirty(id, dirty)?;
        if changed {
            self.notify();
        }
        Ok(changed)
    }

    /// 版を持ち直す(保存した・画面が読み直した本文を当てた)。未保存の印は替えない。写しは送らない(版は写しに載らない)。
    ///
    /// # Errors
    ///
    /// その ID のエディタが無いとき。
    pub fn set_editor_stamp(&self, id: &EditorId, stamp: FileStamp) -> Result<(), SessionError> {
        Ok(lock(&self.inner.workspace).set_editor_stamp(id, stamp)?)
    }

    /// その ID のエディタの写し(持ち主のプロジェクト・相対パス・読み取り専用・版)。無ければ `None`。
    #[must_use]
    pub fn editor(&self, id: &EditorId) -> Option<EditorTab> {
        lock(&self.inner.workspace).editor(id).cloned()
    }

    /// 未保存のエディタ(終了の確認の材料。ADR-015 の 4)。
    #[must_use]
    pub fn unsaved_editors(&self) -> Vec<UnsavedEditor> {
        lock(&self.inner.workspace).unsaved_editors()
    }

    /// エディタの Vim のキー操作を切り替える(ADR-015 の 6)。変わったときだけ写しを送り `true` を返す
    /// (写しを送ると保存が予約され、真のときだけ `ui.editorVim` が書かれる)。
    pub fn set_editor_vim(&self, enabled: bool) -> bool {
        let changed = lock(&self.inner.workspace).set_editor_vim(enabled);
        if changed {
            self.notify();
        }
        changed
    }

    /// Workspace から外したタブのセッションを一覧から外し、写しを送ってから終わらせる(待つのはロックの外)。
    fn end_sessions(&self, terminals: &[TerminalId]) {
        let detached: Vec<Arc<Session>> = terminals.iter().filter_map(|t| self.detach(t)).collect();
        self.notify();
        end_sessions_together(&detached);
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

    /// アプリの終了時に呼ぶ。全セッションのシェルを終わらせ、読み取りスレッドを抜けさせる。以後の接続はシェルを起動しない。
    /// 2 回目以降は何もしない。
    ///
    /// 書き込みが止まっているセッションは待たない(アプリの終了で PTY の親側が閉じ、シェルには SIGHUP が届く)。
    /// 全セッションへ同時に終了を送り、待ちはまとめて 1 回ぶんにする(9 枚でもおよそ 1 枚ぶん。計画書 T33e)。
    pub fn shutdown(&self) {
        let drained: Vec<Arc<Session>> = {
            let mut sessions = lock(&self.inner.sessions);
            self.inner.shutting_down.store(true, Ordering::SeqCst);
            sessions.drain().map(|(_, session)| session).collect()
        };
        end_sessions_together(&drained);
    }

    fn is_shutting_down(&self) -> bool {
        self.inner.shutting_down.load(Ordering::SeqCst)
    }

    /// 1 つのセッションを一覧から外して終わらせる。
    fn close(&self, id: &TerminalId) {
        if let Some(session) = self.detach(id) {
            end_session(&session);
        }
    }

    /// セッションを一覧から外す(終わらせるのは呼び出し側。一覧のロックの外で行う)。
    fn detach(&self, id: &TerminalId) -> Option<Arc<Session>> {
        lock(&self.inner.sessions).remove(id)
    }

    fn session(&self, id: &TerminalId) -> Option<Arc<Session>> {
        lock(&self.inner.sessions).get(id).cloned()
    }

    /// いまの写しを知らせる。Workspace のロックの外で呼ぶ(持ったまま呼ぶと、ロックの順が逆になる)。
    ///
    /// 「写しを作る → 受け口を呼ぶ」を `notify_serial` の中で 1 本ずつ行う(M4 のレビューの S1)。送り手は
    /// コマンド・メニュー・読み取りスレッド(アテンション)・まとめのスレッド(シェルの終了)・見回りと複数あり、
    /// 直列にしないと、先に作った古い写しが新しい写しのあとに届きうる。写しを作るあいだだけ Workspace のロックを取る
    /// (ロックの順は notify_serial → Workspace。受け口を呼ぶあいだは Workspace のロックを持たない)。
    ///
    /// 受け口(`lib.rs`)は待たない処理だけにする: emit(メインスレッド以外からはイベントループへ送るだけ)、
    /// 保存の予約(保存のロックを短く取る)、ツリーの監視の張り替えの依頼(チャネルへ送るだけ)。受け口の中から
    /// Workspace のロックを取るのはよいが、`notify` を呼び直すと自分を待って止まる。
    fn notify(&self) {
        let _serial = lock(&self.inner.notify_serial);
        let snapshot = self.snapshot();
        (self.inner.listener)(snapshot);
    }
}

/// ⌘W で閉じたもの(`SessionRegistry::close_active_tab`)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClosedTab {
    /// ターミナルを閉じた。
    Terminal(TerminalId),
    /// エディタを閉じた(未保存の変更は無かった)。
    Editor(EditorId),
    /// エディタに未保存の変更があるので閉じていない(画面に確認を求める)。
    EditorNeedsConfirmation(EditorId),
}

/// タイルへの操作で、閉じたタイル(ID が無い)を「何もしなかった」とみなす(M4 のレビューの S6)。
fn ignore_closed(result: Result<bool, TerminalError>) -> Result<bool, SessionError> {
    match result {
        Ok(changed) => Ok(changed),
        Err(TerminalError::NotFound(_)) => Ok(false),
        Err(e) => Err(e.into()),
    }
}

/// 一覧から外したセッションを、まとめて終わらせる。シェルの終了の待ち(1 枚につき最大でおよそ 200ms)を重ねるため、
/// 2 つ目からは 1 枚につき 1 本の短いスレッドで同時に終わらせ、全部が終わるまで待つ(全体でおよそ 1 枚ぶん)。
/// スレッドを起こせなければ、そのセッションはこのスレッドで順に終わらせる。
fn end_sessions_together(sessions: &[Arc<Session>]) {
    let Some((first, rest)) = sessions.split_first() else {
        return;
    };
    thread::scope(|scope| {
        for session in rest {
            let spawned = thread::Builder::new()
                .name("arin-pty-end".to_owned())
                .spawn_scoped(scope, move || end_session(session));
            if spawned.is_err() {
                end_session(session);
            }
        }
        end_session(first);
    });
}

/// 一覧から外したセッションを終わらせる。出力を閉じ(読み取りとまとめのスレッドが抜ける)、印を付けてシェルを終わらせる。
/// PTY のロックが取れなければ、持っていた側が手放した時点で終わらせる(`PtyCell::finish_if_closing`)。
/// シェルを終わらせるのに最大でおよそ 200ms 待つことがある。
fn end_session(session: &Session) {
    session.output.close();
    session.pty.closing.store(true, Ordering::SeqCst);
    session.pty.finish_if_closing();
}

impl PtyCell {
    /// ロックを取って `f` を行う(書き込みが止まっていれば待つ)。終わらせた・終わらせる印が付いていれば何もせず `None`。
    fn with<T>(&self, f: impl FnOnce(&mut dyn PtySession) -> T) -> Option<T> {
        let result = {
            let mut pty = lock(&self.pty);
            self.run_open(&mut pty, f)
        };
        self.finish_if_closing();
        result
    }

    /// 待たずにロックを取って `f` を行う。ほかが持っていれば `Err(Busy)`。印が付いていれば `Ok(None)`。
    fn try_with<T>(&self, f: impl FnOnce(&mut dyn PtySession) -> T) -> Result<Option<T>, Busy> {
        let result = {
            let mut pty = match self.pty.try_lock() {
                Ok(pty) => pty,
                Err(TryLockError::Poisoned(p)) => p.into_inner(),
                Err(TryLockError::WouldBlock) => return Err(Busy),
            };
            self.run_open(&mut pty, f)
        };
        self.finish_if_closing();
        Ok(result)
    }

    fn run_open<T>(
        &self,
        pty: &mut Option<Box<dyn PtySession>>,
        f: impl FnOnce(&mut dyn PtySession) -> T,
    ) -> Option<T> {
        if self.closing.load(Ordering::SeqCst) {
            return None;
        }
        pty.as_mut().map(|pty| f(pty.as_mut()))
    }

    /// 印が付いていて、ロックが取れれば、PTY を取り出して終わらせる。取れなければ、持っている側が手放したあとに
    /// ここを呼ぶ(印はロックを試す前に付け、手放した側はロックの外で印を見るので、取りこぼさない)。
    fn finish_if_closing(&self) {
        if !self.closing.load(Ordering::SeqCst) {
            return;
        }
        let taken = match self.pty.try_lock() {
            Ok(mut pty) => pty.take(),
            Err(TryLockError::Poisoned(p)) => p.into_inner().take(),
            Err(TryLockError::WouldBlock) => None,
        };
        if let Some(mut pty) = taken {
            let _ = pty.kill();
            drop(pty);
        }
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

    /// 送り先へ送る。送っているあいだはロックを持たない。閉じたあと(タブを閉じた)は送らない。
    fn send(&self, data: Vec<u8>) {
        if data.is_empty() {
            return;
        }
        let sink = {
            let state = lock(&self.state);
            if state.closed {
                return;
            }
            Arc::clone(&state.sink)
        };
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
    attention: &mut AttentionWatch,
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
                // 出力をまとめのスレッドへ渡してから見る(画面へ送る順序と遅れを変えない)。
                attention.scan(id, state, &buf[..n]);
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
}

/// アテンションの走査と印(FR-055。計画書 T48c)。読み取りスレッドだけが持つ。本文は保持しない・送らない。
///
/// Workspace のロックは契機があったチャンクのあとにだけ取る(出力の塊ごとには取らない)。印が既に立っているあいだの
/// 契機でもロックは取るが、`note_attention` が `false` を返すので写しは送らない(連続するベルで写しが連発しない)。
struct AttentionWatch {
    scanner: AttentionScanner,
    registry: SessionRegistry,
}

impl AttentionWatch {
    fn scan(&mut self, id: &TerminalId, state: &OutputState, data: &[u8]) {
        if !self.scanner.scan(data) {
            return;
        }
        // 閉じたタブ・終了の処理のあとの契機では、印を立てず写しも送らない。閉じたタブは Workspace に無いので
        // `note_attention` も `false` を返すが、終了の処理ではタブが残るので、ここで止める。
        if state.is_closed() {
            return;
        }
        let marked = lock(&self.registry.inner.workspace).note_attention(id);
        if marked {
            // ロックを手放してから送る(`notify` が自分でロックを取り直して写しを作る)。
            self.registry.notify();
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
    pty: Arc<PtyCell>,
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
            if self.output.is_closed() {
                // 一覧から外された(タブを閉じた・アプリの終了)。読み取りスレッドは送り先が無くなったことで抜ける。
                return;
            }
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
    /// 終わったらタブが閉じるので、セッションを一覧から外して資源を返す。
    fn check_exit(&self) -> bool {
        let status = match self.pty.try_with(|pty| pty.try_wait()) {
            Err(Busy) | Ok(Some(Ok(None) | Err(_))) => return false,
            // 終わらせた(タブを閉じた・アプリの終了)。
            Ok(None) => return true,
            Ok(Some(Ok(Some(exit)))) => exit,
        };
        let closed = lock(&self.registry.inner.workspace).mark_exited(&self.id, Some(status));
        if closed.is_some() {
            let session = self.registry.detach(&self.id);
            self.registry.notify();
            if let Some(session) = session {
                end_session(&session);
            }
        }
        true
    }
}
