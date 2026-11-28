//! デスクトップとパスのコマンド(`src-tauri/src/commands/desktop.rs`、計画書 T42d)を、Tauri の模擬ランタイムで画面から呼ぶ形で確かめる。
//! FR-015(パスのコピー。AC-T03)、FR-016・FR-024(ターミナルへの挿入。AC-T04)、FR-018(Finder で表示・開く)、
//! FR-035(ルートの外に出ない)、設計書 §7.1「パスをターミナルへ」・§12・§15.2 #6。
//!
//! - Finder とアプリは偽のデスクトップのポート(`builder_with_desktop`)。本物の Finder を開かず、アプリを起動しない
//! - ペーストボードは名前付きのもの(`MacPasteboard::named`・`MacFilePasteboard::named`)。利用者のクリップボードに触れない
//! - PTY は偽物。シェルを起動せず、書かれたバイト列をターミナル(作業フォルダ)ごとに記録する
//! - プロジェクトのフォルダと状態ファイルは、すべて一時フォルダの中に作る。ルートは正規化した実パスで登録する
//! - 確認は順に行い、前の確認で作ったプロジェクトとターミナルを次で使う
//!
//! macOS ではメニューの部品をメインスレッドでしか作れないため、`harness = false` で走らせる(`app_builder` と同じ)。

mod support;

use std::cell::Cell;
use std::collections::VecDeque;
use std::fs;
use std::io::{self, Read};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, ThreadId};
use std::time::{Duration, Instant};

use arin_core::fileops::{FileOpOutcome, NotAllowedReason};
use arin_core::paths::shell::escape_shell_word;
use arin_core::ports::desktop::{DesktopError, DesktopPort};
use arin_core::ports::pasteboard::PasteboardPort;
use arin_core::ports::release::{ReleaseFetchError, ReleasePort};
use arin_core::ports::{ChildExit, PtyError, PtyPort, PtySession, PtySize, SpawnedPty};
use arin_core::terminal::ShellLaunchSpec;
use arin_core::update::UpdateNotice;
use arin_core::workspace::{AddProjectOutcome, AppSnapshot};
use arin_lib::TreePorts;
use arin_lib::persistence::PersistencePorts;
use arin_lib::projects::{FolderPicker, FolderPurpose, ProjectPorts};
use arin_lib::sessions::ShellDefaults;
use arin_lib::update::{self, UpdateChecker};
use arin_platform::desktop::{MacFilePasteboard, MacPasteboard};
use arin_platform::fs::LocalFileSystem;
use arin_platform::system::UrandomIdSource;
use arin_platform::watch::FsEventsWatcher;
use serde_json::json;
use support::{request, request_with};
use tauri::ipc::{InvokeBody, InvokeResponseBody};
use tauri::test::{MockRuntime, get_ipc_response};
use tauri::{AppHandle, Manager, RunEvent, WebviewWindow};

const WAIT: Duration = Duration::from_secs(10);

/// 失敗の確認で、ペーストボードに先に置いておく文字列(変わっていないことを確かめる)。
const SENTINEL: &str = "番兵の文字列";

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn wait_until(mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        thread::sleep(Duration::from_millis(5));
    }
    condition()
}

// ---------------------------------------------------------------------------
// 偽のダイアログ・偽の PTY・偽のデスクトップ
// ---------------------------------------------------------------------------

/// 決めた答えを順に返すダイアログ。
#[derive(Default)]
struct FakePicker {
    answers: Mutex<VecDeque<Option<PathBuf>>>,
}

impl FolderPicker for FakePicker {
    fn pick_folder(&self, _purpose: FolderPurpose) -> Result<Option<PathBuf>, String> {
        lock(&self.answers)
            .pop_front()
            .ok_or_else(|| "答えを用意していないのにダイアログが開かれた".to_owned())
    }
}

/// 偽の PTY の 1 セッション(起動した順に 1 つ)。
struct SessionCtl {
    /// 作業フォルダ(どのプロジェクトのターミナルかを見分ける)。
    cwd: PathBuf,
    /// 書かれたバイト列(書かれた順につないだもの)。
    written: Mutex<Vec<u8>>,
    /// 終わったことにする終わり方。
    exit: Mutex<Option<ChildExit>>,
    /// 出力を流す口。落とすと出力が終わる。
    feed: Mutex<Option<Sender<Vec<u8>>>>,
}

impl SessionCtl {
    fn written(&self) -> Vec<u8> {
        lock(&self.written).clone()
    }

    fn clear(&self) {
        lock(&self.written).clear();
    }

    /// シェルが終わったことにする(出力を終え、終了を返す)。
    fn exit(&self) {
        *lock(&self.exit) = Some(ChildExit {
            exit_code: 0,
            signal: None,
        });
        lock(&self.feed).take();
    }
}

#[derive(Default)]
struct Control {
    sessions: Mutex<Vec<Arc<SessionCtl>>>,
}

impl Control {
    fn all(&self) -> Vec<Arc<SessionCtl>> {
        lock(&self.sessions).clone()
    }

    fn last(&self) -> Arc<SessionCtl> {
        lock(&self.sessions)
            .last()
            .cloned()
            .expect("まだ 1 つも起動していない")
    }

    /// どのセッションにも何も書かれていない。
    fn nothing_written(&self) -> bool {
        self.all().iter().all(|s| s.written().is_empty())
    }

    fn clear(&self) {
        for session in self.all() {
            session.clear();
        }
    }
}

struct FakePty(Arc<Control>);

impl PtyPort for FakePty {
    fn spawn(&self, spec: &ShellLaunchSpec, _size: PtySize) -> Result<SpawnedPty, PtyError> {
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        let ctl = Arc::new(SessionCtl {
            cwd: spec.cwd().to_path_buf(),
            written: Mutex::new(Vec::new()),
            exit: Mutex::new(None),
            feed: Mutex::new(Some(tx)),
        });
        lock(&self.0.sessions).push(Arc::clone(&ctl));
        Ok(SpawnedPty {
            session: Box::new(FakeSession(ctl)),
            output: Box::new(FakeOutput(rx)),
        })
    }
}

struct FakeOutput(Receiver<Vec<u8>>);

impl Read for FakeOutput {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.0.recv() {
            Ok(chunk) => {
                buf[..chunk.len()].copy_from_slice(&chunk);
                Ok(chunk.len())
            }
            Err(_) => Ok(0),
        }
    }
}

struct FakeSession(Arc<SessionCtl>);

impl PtySession for FakeSession {
    fn write(&mut self, data: &[u8]) -> Result<(), PtyError> {
        lock(&self.0.written).extend_from_slice(data);
        Ok(())
    }

    fn resize(&mut self, _size: PtySize) -> Result<(), PtyError> {
        Ok(())
    }

    fn kill(&mut self) -> Result<(), PtyError> {
        lock(&self.0.feed).take();
        Ok(())
    }

    fn try_wait(&mut self) -> Result<Option<ChildExit>, PtyError> {
        Ok(lock(&self.0.exit).clone())
    }
}

/// 偽のデスクトップに届いた頼み。
#[derive(Debug, Clone, PartialEq, Eq)]
enum DesktopCall {
    Reveal(PathBuf),
    Open(PathBuf),
    Page(String),
}

/// Finder もアプリも開かず、頼まれたパスと呼ばれたスレッドを記録する。`fail` を入れると次の 1 回をその失敗にする。
#[derive(Default)]
struct FakeDesktop {
    calls: Mutex<Vec<DesktopCall>>,
    threads: Mutex<Vec<ThreadId>>,
    fail: Mutex<Option<DesktopError>>,
}

impl FakeDesktop {
    fn calls(&self) -> Vec<DesktopCall> {
        lock(&self.calls).clone()
    }

    fn clear(&self) {
        lock(&self.calls).clear();
        lock(&self.threads).clear();
    }

    fn record(&self, call: DesktopCall) -> Result<(), DesktopError> {
        lock(&self.calls).push(call);
        lock(&self.threads).push(thread::current().id());
        lock(&self.fail).take().map_or(Ok(()), Err)
    }
}

impl DesktopPort for FakeDesktop {
    fn reveal_in_finder(&self, path: &Path) -> Result<(), DesktopError> {
        self.record(DesktopCall::Reveal(path.to_path_buf()))
    }

    fn open_with_default_app(&self, path: &Path) -> Result<(), DesktopError> {
        self.record(DesktopCall::Open(path.to_path_buf()))
    }

    fn open_release_page(&self, url: &str) -> Result<(), DesktopError> {
        self.record(DesktopCall::Page(url.to_owned()))
    }
}

// ---------------------------------------------------------------------------
// 共有するもの
// ---------------------------------------------------------------------------

struct Ctx {
    picker: Arc<FakePicker>,
    control: Arc<Control>,
    desktop: Arc<FakeDesktop>,
    /// テスト用の名前付きのペーストボード(アプリに渡したものと同じ)。
    pasteboard: MacPasteboard,
    /// 一時フォルダ全体。
    temp_root: PathBuf,
    /// プロジェクトのフォルダを置く一時フォルダ(状態フォルダとは別)。
    work: PathBuf,
    /// 模擬のアプリを走らせているスレッド(コマンドの受け口)。
    main_thread: ThreadId,
}

impl Ctx {
    /// 一時フォルダの中のフォルダ(作る)の、正規化した実パス(`/private/var/...`)。
    fn folder(&self, name: &str) -> PathBuf {
        let path = self.work.join(name);
        fs::create_dir_all(&path).expect("フォルダを作れなかった");
        fs::canonicalize(&path).expect("正規化できなかった")
    }

    /// 文字列に、一時フォルダの絶対パス(元の形と正規化した形)が入っていないことを確かめる。
    fn assert_no_absolute_path(&self, text: &str) {
        let canonical = fs::canonicalize(&self.temp_root).expect("正規化できなかった");
        for path in [&self.temp_root, &canonical] {
            let path = path.display().to_string();
            assert!(!text.contains(&path), "応答に絶対パスが入っている: {text}");
        }
    }

    /// ペーストボードに番兵を置き、偽の PTY とデスクトップの記録を消す(失敗の確認の前に)。
    fn reset(&self) {
        self.pasteboard
            .write_text(SENTINEL)
            .expect("ペーストボードに書けなかった");
        self.control.clear();
        self.desktop.clear();
    }

    /// 何も外へ出ていない(ペーストボードは番兵のまま、PTY に何も書かれず、Finder とアプリに何も渡っていない)。
    fn assert_untouched(&self, what: &str) {
        assert_eq!(
            self.pasteboard.read_text().as_deref(),
            Some(SENTINEL),
            "{what}: ペーストボードが変わった"
        );
        assert!(self.control.nothing_written(), "{what}: PTY に書かれた");
        assert!(
            self.desktop.calls().is_empty(),
            "{what}: Finder かアプリに渡った: {:?}",
            self.desktop.calls()
        );
    }
}

type Check = fn(&AppHandle<MockRuntime>, &WebviewWindow<MockRuntime>, &Ctx);

fn call(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    cmd: &str,
    args: serde_json::Value,
) -> Result<InvokeResponseBody, serde_json::Value> {
    get_ipc_response(window, request_with(handle, cmd, InvokeBody::Json(args)))
}

/// コマンドを呼び、応答を読む。応答の JSON に絶対パスが無いことも確かめる。
fn op(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
    cmd: &str,
    args: serde_json::Value,
) -> FileOpOutcome {
    let body = call(handle, window, cmd, args.clone())
        .unwrap_or_else(|e| panic!("{cmd}({args}) が失敗した: {e}"));
    let value: serde_json::Value = body
        .deserialize()
        .unwrap_or_else(|e| panic!("{cmd} の応答を JSON として読めなかった: {e}"));
    ctx.assert_no_absolute_path(&value.to_string());
    serde_json::from_value(value)
        .unwrap_or_else(|e| panic!("{cmd} の応答を FileOpOutcome として読めなかった: {e}"))
}

/// コマンドが失敗することを確かめ、失敗の文言を返す。文言に絶対パスが無いことも確かめる。
fn op_error(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
    cmd: &str,
    args: serde_json::Value,
) -> String {
    let text = match call(handle, window, cmd, args.clone()) {
        Ok(_) => panic!("{cmd}({args}) は失敗するはずが成功した"),
        Err(e) => e
            .as_str()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| e.to_string()),
    };
    ctx.assert_no_absolute_path(&text);
    text
}

fn snapshot(handle: &AppHandle<MockRuntime>, window: &WebviewWindow<MockRuntime>) -> AppSnapshot {
    get_ipc_response(window, request(handle, "get_app_snapshot"))
        .unwrap_or_else(|e| panic!("get_app_snapshot が失敗した: {e}"))
        .deserialize()
        .unwrap_or_else(|e| panic!("AppSnapshot として読めなかった: {e}"))
}

fn project_id_by_name(snapshot: &AppSnapshot, name: &str) -> String {
    snapshot
        .projects
        .iter()
        .find(|p| p.name == name)
        .unwrap_or_else(|| panic!("{name} のプロジェクトが無い"))
        .id
        .as_str()
        .to_owned()
}

/// プロジェクトの表示中のタブ。
fn active_terminal(snapshot: &AppSnapshot, name: &str) -> String {
    snapshot
        .projects
        .iter()
        .find(|p| p.name == name)
        .unwrap_or_else(|| panic!("{name} のプロジェクトが無い"))
        .active_terminal_id
        .as_ref()
        .unwrap_or_else(|| panic!("{name} に表示中のタブが無い"))
        .as_str()
        .to_owned()
}

/// ダイアログで `path` を選んだことにして登録し、その ID を返す(タブが 1 枚開き、そのプロジェクトが選ばれる)。
fn register(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
    path: &Path,
) -> String {
    lock(&ctx.picker.answers).push_back(Some(path.to_path_buf()));
    let outcome: AddProjectOutcome = call(handle, window, "add_project_by_dialog", json!({}))
        .unwrap_or_else(|e| panic!("add_project_by_dialog が失敗した: {e}"))
        .deserialize()
        .expect("AddProjectOutcome として読めなかった");
    match outcome {
        AddProjectOutcome::Added { project_id }
        | AddProjectOutcome::AlreadyRegistered { project_id } => project_id.as_str().to_owned(),
        AddProjectOutcome::Cancelled => panic!("登録されなかった"),
    }
}

/// ターミナルをつなぐ(偽の PTY が起動する)。
fn connect(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    terminal: &str,
    channel: u32,
) {
    call(
        handle,
        window,
        "connect_terminal",
        json!({ "terminalId": terminal, "rows": 24, "cols": 80, "onOutput": format!("__CHANNEL__:{channel}") }),
    )
    .unwrap_or_else(|e| panic!("connect_terminal が失敗した: {e}"));
}

/// 作業フォルダが `root` のセッション(1 つだけのはず)。
fn session_in(ctx: &Ctx, root: &Path) -> Arc<SessionCtl> {
    let found: Vec<_> = ctx
        .control
        .all()
        .into_iter()
        .filter(|s| s.cwd == root)
        .collect();
    assert_eq!(
        found.len(),
        1,
        "{} のセッションが 1 つでない",
        root.display()
    );
    Arc::clone(&found[0])
}

fn alpha_root(ctx: &Ctx) -> PathBuf {
    fs::canonicalize(ctx.work.join("alpha")).expect("正規化できなかった")
}

fn beta_root(ctx: &Ctx) -> PathBuf {
    fs::canonicalize(ctx.work.join("beta")).expect("正規化できなかった")
}

fn failed(message: &str) -> FileOpOutcome {
    FileOpOutcome::Failed {
        message: message.to_owned(),
    }
}

fn done() -> FileOpOutcome {
    FileOpOutcome::Done { select: None }
}

/// 改行・復帰などの制御のバイト(C0 と DEL)を含まない。
fn has_no_control_bytes(bytes: &[u8]) -> bool {
    bytes.iter().all(|&b| b >= 0x20 && b != 0x7f)
}

// ---------------------------------------------------------------------------
// 確認
// ---------------------------------------------------------------------------

/// alpha と beta を登録し、それぞれのタブをつなぐ(最初の確認)。中身を作る。
fn 相対パスのコピーで_src_a_b_ts_がペーストボードに入る(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let alpha = ctx.folder("alpha");
    let beta = ctx.folder("beta");
    let outside = ctx.folder("outside");
    fs::create_dir(alpha.join("src")).expect("フォルダを作れなかった");
    fs::write(alpha.join("src/a b.ts"), "").expect("ファイルを作れなかった");
    fs::write(alpha.join("a.md"), "# a").expect("ファイルを作れなかった");
    fs::write(alpha.join("a\nb.txt"), "").expect("改行を含む名前のファイルを作れなかった");
    fs::write(alpha.join("-rf"), "").expect("ファイルを作れなかった");
    fs::write(outside.join("secret.txt"), "外の秘密").expect("ファイルを作れなかった");
    // ルートの外を指すリンクと、ルートの中のファイルを指すリンク。
    symlink(&outside, alpha.join("escape")).expect("リンクを作れなかった");
    symlink("a.md", alpha.join("link.md")).expect("リンクを作れなかった");
    // 実行できるファイル。
    let run = alpha.join("run.sh");
    fs::write(&run, "#!/bin/sh\necho hi\n").expect("ファイルを作れなかった");
    fs::set_permissions(&run, fs::Permissions::from_mode(0o755)).expect("権限を変えられなかった");

    let alpha_id = register(handle, window, ctx, &alpha);
    let beta_id = register(handle, window, ctx, &beta);
    let s = snapshot(handle, window);
    connect(handle, window, &active_terminal(&s, "alpha"), 9001);
    connect(handle, window, &active_terminal(&s, "beta"), 9002);
    assert_eq!(ctx.control.all().len(), 2, "2 つのシェルが起動していない");
    assert_ne!(alpha_id, beta_id);

    // AC-T03: `src/a b.ts` の相対パスをコピーすると、エスケープせずにそのまま入る。
    ctx.reset();
    let outcome = op(
        handle,
        window,
        ctx,
        "copy_entry_path",
        json!({ "projectId": alpha_id, "path": "src/a b.ts", "kind": "relative" }),
    );
    assert_eq!(outcome, done());
    assert_eq!(ctx.pasteboard.read_text().as_deref(), Some("src/a b.ts"));
    // フォルダもコピーできる(末尾に `/` を付けない)。
    let outcome = op(
        handle,
        window,
        ctx,
        "copy_entry_path",
        json!({ "projectId": alpha_id, "path": "src", "kind": "relative" }),
    );
    assert_eq!(outcome, done());
    assert_eq!(ctx.pasteboard.read_text().as_deref(), Some("src"));
    assert!(ctx.control.nothing_written(), "PTY に書かれた");
    assert!(ctx.desktop.calls().is_empty(), "Finder かアプリに渡った");
}

fn 絶対パスのコピーでルートとつないだパスが入る(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let alpha = alpha_root(ctx);
    let id = project_id_by_name(&snapshot(handle, window), "alpha");
    ctx.reset();
    // 応答には載らない(`op` が確かめる)。ペーストボードにだけ書く。
    let outcome = op(
        handle,
        window,
        ctx,
        "copy_entry_path",
        json!({ "projectId": id, "path": "src/a b.ts", "kind": "absolute" }),
    );
    assert_eq!(outcome, done());
    let expected = alpha.join("src/a b.ts").display().to_string();
    assert_eq!(
        ctx.pasteboard.read_text().as_deref(),
        Some(expected.as_str())
    );
}

fn ルートの絶対パスをコピーできる(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let alpha = alpha_root(ctx);
    let id = project_id_by_name(&snapshot(handle, window), "alpha");
    ctx.reset();
    // ルートは相対パスを空にして呼ぶ(余白のメニューの「ルートの絶対パスをコピー」)。
    let outcome = op(
        handle,
        window,
        ctx,
        "copy_entry_path",
        json!({ "projectId": id, "path": "", "kind": "absolute" }),
    );
    assert_eq!(outcome, done());
    let expected = alpha.display().to_string();
    assert_eq!(
        ctx.pasteboard.read_text().as_deref(),
        Some(expected.as_str())
    );

    // ルートの相対パスは空の文字列になるので、ペーストボードを空にせず拒否する【仮定】。
    ctx.reset();
    let outcome = op(
        handle,
        window,
        ctx,
        "copy_entry_path",
        json!({ "projectId": id, "path": "", "kind": "relative" }),
    );
    assert_eq!(
        outcome,
        FileOpOutcome::NotAllowed {
            reason: NotAllowedReason::RootItself
        }
    );
    ctx.assert_untouched("ルートの相対パスのコピー");
}

fn 挿入で偽_pty_に_src_a_b_ts_と空白が書かれ改行が書かれない(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let s = snapshot(handle, window);
    let id = project_id_by_name(&s, "alpha");
    let terminal = active_terminal(&s, "alpha");
    let session = session_in(ctx, &alpha_root(ctx));
    ctx.reset();

    // AC-T04 の Rust 側: 同じプロジェクトのターミナルへは相対パスをエスケープし、末尾に空白を 1 つ付けて書く。
    let outcome = op(
        handle,
        window,
        ctx,
        "insert_entry_path",
        json!({ "projectId": id, "paths": ["src/a b.ts"], "terminalId": terminal }),
    );
    assert_eq!(outcome, done());
    assert_eq!(session.written(), b"src/a\\ b.ts ".to_vec());
    assert!(
        !session.written().contains(&b'\r') && !session.written().contains(&b'\n'),
        "Enter が送られた"
    );

    // 改行を含む名前でも、改行のバイトを書かない(`$'…'` で包む。【仮定】17)。
    session.clear();
    let outcome = op(
        handle,
        window,
        ctx,
        "insert_entry_path",
        json!({ "projectId": id, "paths": ["a\nb.txt"], "terminalId": terminal }),
    );
    assert_eq!(outcome, done());
    let written = session.written();
    assert!(
        has_no_control_bytes(&written),
        "制御のバイトが書かれた: {written:?}"
    );
    assert_eq!(
        String::from_utf8(written).expect("UTF-8 でない"),
        format!("{} ", escape_shell_word("a\nb.txt"))
    );

    // 複数のパスは空白でつなぐ。ルートは `.`、`-` で始まる名前は `./` を付ける。
    session.clear();
    let outcome = op(
        handle,
        window,
        ctx,
        "insert_entry_path",
        json!({ "projectId": id, "paths": ["", "-rf", "a.md"], "terminalId": terminal }),
    );
    assert_eq!(outcome, done());
    assert_eq!(session.written(), b". ./-rf a.md ".to_vec());

    // パスが 1 つも無ければ何も書かない。
    session.clear();
    let outcome = op(
        handle,
        window,
        ctx,
        "insert_entry_path",
        json!({ "projectId": id, "paths": [], "terminalId": terminal }),
    );
    assert_eq!(outcome, done());
    assert!(session.written().is_empty(), "空の挿入で何か書かれた");

    // 別のターミナル(beta)には書かれない。ペーストボード・Finder にも触れない。
    assert!(session_in(ctx, &beta_root(ctx)).written().is_empty());
    assert_eq!(ctx.pasteboard.read_text().as_deref(), Some(SENTINEL));
    assert!(ctx.desktop.calls().is_empty());
}

fn 別のプロジェクトのターミナルへは絶対パスが書かれる(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let s = snapshot(handle, window);
    let alpha_id = project_id_by_name(&s, "alpha");
    let beta_terminal = active_terminal(&s, "beta");
    ctx.reset();

    // §15.2 #6 の A: alpha の項目を beta のターミナルへ挿入すると、エスケープした絶対パスになる。
    let outcome = op(
        handle,
        window,
        ctx,
        "insert_entry_path",
        json!({ "projectId": alpha_id, "paths": ["src/a b.ts"], "terminalId": beta_terminal }),
    );
    assert_eq!(outcome, done());
    let absolute = alpha_root(ctx).join("src/a b.ts").display().to_string();
    let written = session_in(ctx, &beta_root(ctx)).written();
    assert_eq!(
        String::from_utf8(written.clone()).expect("UTF-8 でない"),
        format!("{} ", escape_shell_word(&absolute))
    );
    assert!(has_no_control_bytes(&written), "制御のバイトが書かれた");
    assert!(
        session_in(ctx, &alpha_root(ctx)).written().is_empty(),
        "alpha のターミナルに書かれた"
    );
}

fn 挿入先のターミナルが無い_起動前_終了済みなら何も書かず失敗を返す(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let s = snapshot(handle, window);
    let alpha_id = project_id_by_name(&s, "alpha");

    // 無い ID。
    ctx.reset();
    let outcome = op(
        handle,
        window,
        ctx,
        "insert_entry_path",
        json!({ "projectId": alpha_id, "paths": ["a.md"], "terminalId": "no-such-terminal" }),
    );
    assert_eq!(outcome, failed("挿入先のターミナルが見つかりません"));
    ctx.assert_untouched("無いターミナルへの挿入");

    // 起動前(つないでいない新しいタブ)。alpha を選んでタブを開く。
    call(
        handle,
        window,
        "select_project",
        json!({ "projectId": alpha_id }),
    )
    .unwrap_or_else(|e| panic!("select_project が失敗した: {e}"));
    let opened: String = call(handle, window, "new_terminal", json!({}))
        .unwrap_or_else(|e| panic!("new_terminal が失敗した: {e}"))
        .deserialize()
        .expect("TerminalId として読めなかった");
    ctx.reset();
    let outcome = op(
        handle,
        window,
        ctx,
        "insert_entry_path",
        json!({ "projectId": alpha_id, "paths": ["a.md"], "terminalId": opened }),
    );
    assert_eq!(outcome, failed("挿入先のターミナルが動いていません"));
    ctx.assert_untouched("起動前のターミナルへの挿入");

    // 終了済み: つないで起動させてからシェルを終わらせる。通常の起動ではタブが閉じる(C-21)ので、見つからないになる。
    connect(handle, window, &opened, 9003);
    let session = ctx.control.last();
    session.exit();
    assert!(
        wait_until(|| !snapshot(handle, window)
            .terminals
            .iter()
            .any(|t| t.id.as_str() == opened)),
        "シェルが終わってもタブが閉じなかった"
    );
    ctx.reset();
    let outcome = op(
        handle,
        window,
        ctx,
        "insert_entry_path",
        json!({ "projectId": alpha_id, "paths": ["a.md"], "terminalId": opened }),
    );
    assert_eq!(outcome, failed("挿入先のターミナルが見つかりません"));
    ctx.assert_untouched("終了したターミナルへの挿入");
}

fn finder_で表示と開くは偽のポートに正しいパスで届く(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let alpha = alpha_root(ctx);
    let id = project_id_by_name(&snapshot(handle, window), "alpha");
    ctx.reset();

    // ファイル・フォルダ・ルート(余白のメニュー)・リンク自体を Finder で表示できる。
    for path in ["src/a b.ts", "src", "", "escape"] {
        let outcome = op(
            handle,
            window,
            ctx,
            "reveal_entry",
            json!({ "projectId": id, "path": path }),
        );
        assert_eq!(outcome, done(), "{path:?} を Finder で表示できなかった");
    }
    // 開くのはファイルだけ。
    let outcome = op(
        handle,
        window,
        ctx,
        "open_entry",
        json!({ "projectId": id, "path": "src/a b.ts" }),
    );
    assert_eq!(outcome, done());
    assert_eq!(
        ctx.desktop.calls(),
        vec![
            DesktopCall::Reveal(alpha.join("src/a b.ts")),
            DesktopCall::Reveal(alpha.join("src")),
            DesktopCall::Reveal(alpha.clone()),
            DesktopCall::Reveal(alpha.join("escape")),
            DesktopCall::Open(alpha.join("src/a b.ts")),
        ]
    );
    // `openURL:` は少し待つことがあるので、メインスレッドの外で呼ぶ。
    for thread in lock(&ctx.desktop.threads).iter() {
        assert_ne!(
            *thread, ctx.main_thread,
            "メインスレッドで Finder・アプリに渡した"
        );
    }
    assert_eq!(ctx.pasteboard.read_text().as_deref(), Some(SENTINEL));
    assert!(ctx.control.nothing_written());

    // 対象が無ければ偽のポートに渡さず見つからない。
    ctx.reset();
    for cmd in ["reveal_entry", "open_entry"] {
        let outcome = op(
            handle,
            window,
            ctx,
            cmd,
            json!({ "projectId": id, "path": "src/gone.ts" }),
        );
        assert_eq!(outcome, FileOpOutcome::NotFound, "{cmd}");
    }
    ctx.assert_untouched("無い項目");

    // ポートの失敗の文言(絶対パスを含む)は画面へ返さない。
    ctx.reset();
    *lock(&ctx.desktop.fail) = Some(DesktopError::Failed(format!(
        "既定のアプリで開けなかった: {}",
        alpha.join("a.md").display()
    )));
    let outcome = op(
        handle,
        window,
        ctx,
        "open_entry",
        json!({ "projectId": id, "path": "a.md" }),
    );
    assert_eq!(outcome, failed("既定のアプリで開けませんでした"));
    *lock(&ctx.desktop.fail) = Some(DesktopError::NotFound(
        alpha.join("a.md").display().to_string(),
    ));
    let outcome = op(
        handle,
        window,
        ctx,
        "reveal_entry",
        json!({ "projectId": id, "path": "a.md" }),
    );
    assert_eq!(outcome, FileOpOutcome::NotFound);
}

fn 開くはファイルだけでフォルダ_リンク_実行できるファイルは渡さない(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let id = project_id_by_name(&snapshot(handle, window), "alpha");
    for (path, expected) in [
        // フォルダは画面が開閉する(`.app` もフォルダなので起動しない)。
        ("src", failed("フォルダは開けません")),
        // リンクは先がルートの中でも開かない【仮定】。
        ("link.md", failed("リンクは開けません")),
        ("escape", failed("リンクは開けません")),
        // 実行できるファイルは既定のアプリ(ターミナル)で実行されうるので開かない【仮定】。
        ("run.sh", failed("実行できるファイルは開けません")),
        // ルート自体。
        (
            "",
            FileOpOutcome::NotAllowed {
                reason: NotAllowedReason::RootItself,
            },
        ),
    ] {
        ctx.reset();
        let outcome = op(
            handle,
            window,
            ctx,
            "open_entry",
            json!({ "projectId": id, "path": path }),
        );
        assert_eq!(outcome, expected, "{path:?}");
        ctx.assert_untouched(path);
    }
}

fn 開くは実行につながる種類の拡張子のファイルを渡さない(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let id = project_id_by_name(&snapshot(handle, window), "alpha");
    let alpha = alpha_root(ctx);
    // 実行の権限が無くても、既定のアプリが実行・起動・インストールするもの。大文字・小文字は区別しない。
    let names = [
        "a.terminal",
        "b.command",
        "c.tool",
        "d.jar",
        "e.app",
        "f.pkg",
        "g.mpkg",
        "h.dmg",
        "i.scpt",
        "j.scptd",
        "k.applescript",
        "l.workflow",
        "m.action",
        "n.webloc",
        "o.inetloc",
        "p.fileloc",
        "q.url",
        "Run.COMMAND",
        "Site.WebLoc",
    ];
    for name in names {
        let file = alpha.join(name);
        fs::write(&file, "").expect("ファイルを作れなかった");
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644))
            .expect("権限を変えられなかった");
    }
    for name in names {
        ctx.reset();
        let outcome = op(
            handle,
            window,
            ctx,
            "open_entry",
            json!({ "projectId": id, "path": name }),
        );
        assert_eq!(
            outcome,
            failed("この種類のファイルは開けません。Finder で表示から開いてください"),
            "{name}"
        );
        ctx.assert_untouched(name);
    }
    // 似ているが一覧に無い拡張子は開く。
    fs::write(alpha.join("notes.command.md"), "").expect("ファイルを作れなかった");
    ctx.reset();
    let outcome = op(
        handle,
        window,
        ctx,
        "open_entry",
        json!({ "projectId": id, "path": "notes.command.md" }),
    );
    assert_eq!(outcome, done());
    for name in names.iter().chain(&["notes.command.md"]) {
        fs::remove_file(alpha.join(name)).expect("消せなかった");
    }
}

fn ルートの外とドットドットは拒否する(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let s = snapshot(handle, window);
    let id = project_id_by_name(&s, "alpha");
    let terminal = active_terminal(&s, "alpha");

    // 4 つのコマンドの引数(項目だけを差し替える)。
    let args = |path: &str| {
        vec![
            ("reveal_entry", json!({ "projectId": id, "path": path })),
            ("open_entry", json!({ "projectId": id, "path": path })),
            (
                "copy_entry_path",
                json!({ "projectId": id, "path": path, "kind": "absolute" }),
            ),
            (
                "insert_entry_path",
                json!({ "projectId": id, "paths": [path], "terminalId": terminal }),
            ),
        ]
    };

    // `..`・絶対パス・`.`・空の要素は、コマンドに届く前(引数の読み込み)で拒否される
    // (失敗の文言は受け取った文字列を含むので、絶対パスは一時フォルダの外のものを使う)。
    for path in [
        "..",
        "../outside/secret.txt",
        "src/../..",
        "/etc/passwd",
        "./a.md",
        "src//a b.ts",
    ] {
        for (cmd, arg) in args(path) {
            ctx.reset();
            let _ = op_error(handle, window, ctx, cmd, arg);
            ctx.assert_untouched(&format!("{cmd}({path:?})"));
        }
    }

    // シンボリックリンクでルートの外へ出る経路は、ルートの外として拒否する(リンクの先を開かない・書かない)。
    for (cmd, arg) in args("escape/secret.txt") {
        ctx.reset();
        let outcome = op(handle, window, ctx, cmd, arg);
        assert_eq!(
            outcome,
            FileOpOutcome::NotAllowed {
                reason: NotAllowedReason::OutsideRoot
            },
            "{cmd}"
        );
        ctx.assert_untouched(cmd);
    }

    // 見つからないプロジェクトは失敗(文言)にする。
    for (cmd, mut arg) in args("a.md") {
        arg["projectId"] = json!("no-such-project");
        ctx.reset();
        let _ = op_error(handle, window, ctx, cmd, arg);
        ctx.assert_untouched(cmd);
    }
}

/// 偽の問い合わせの口(ADR-018)。決めた本文を返し、呼ばれた回数を数える。
struct FakeReleases {
    body: Result<String, String>,
    calls: Mutex<usize>,
}

impl ReleasePort for FakeReleases {
    fn fetch_latest(&self) -> Result<String, ReleaseFetchError> {
        *lock(&self.calls) += 1;
        self.body.clone().map_err(ReleaseFetchError)
    }
}

fn use_releases(handle: &AppHandle<MockRuntime>, body: Result<&str, &str>) -> Arc<FakeReleases> {
    let fake = Arc::new(FakeReleases {
        body: body.map(ToOwned::to_owned).map_err(ToOwned::to_owned),
        calls: Mutex::new(0),
    });
    handle.state::<UpdateChecker>().use_port(fake.clone());
    fake
}

fn update_notice(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
) -> Option<UpdateNotice> {
    call(handle, window, "get_update_notice", json!({}))
        .expect("get_update_notice が失敗した")
        .deserialize()
        .expect("知らせを読めなかった")
}

fn wait_check(handle: &AppHandle<MockRuntime>) {
    assert!(
        wait_until(|| !update::is_running(handle)),
        "確認が終わらなかった"
    );
}

fn 新しい版の確認は知らせを残し_ページは版から作った_url_で開く(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    ctx.reset();
    // 既定はオフなので、起動時には問い合わせていない
    assert_eq!(update_notice(handle, window), None);

    // メニューから: 最新なら「最新です」、失敗なら理由
    let version = handle.package_info().version.to_string();
    use_releases(handle, Ok(&format!(r#"{{"tag_name":"v{version}"}}"#)));
    update::check(handle, true);
    wait_check(handle);
    assert_eq!(
        update_notice(handle, window),
        Some(UpdateNotice::UpToDate { version })
    );
    use_releases(handle, Err("通信できない"));
    update::check(handle, true);
    wait_check(handle);
    assert_eq!(
        update_notice(handle, window),
        Some(UpdateNotice::Failed {
            reason: "通信できない".to_owned()
        })
    );
    call(handle, window, "dismiss_update_notice", json!({})).expect("閉じられなかった");
    assert_eq!(update_notice(handle, window), None);

    // 設定でオンにすると、その場で確かめる。新しい版があれば知らせる(応答の html_url は使わない)
    let fake = use_releases(
        handle,
        Ok(r#"{"tag_name":"v99.0.0","html_url":"https://evil.example/"}"#),
    );
    call(
        handle,
        window,
        "set_check_updates",
        json!({ "enabled": true }),
    )
    .expect("切り替えられなかった");
    wait_check(handle);
    assert_eq!(*lock(&fake.calls), 1);
    assert_eq!(
        update_notice(handle, window),
        Some(UpdateNotice::Available {
            version: "99.0.0".to_owned()
        })
    );
    let snapshot: AppSnapshot = call(handle, window, "get_app_snapshot", json!({}))
        .expect("写しを取れなかった")
        .deserialize()
        .expect("写しを読めなかった");
    assert!(snapshot.check_updates);

    call(handle, window, "open_release_page", json!({})).expect("開けなかった");
    assert_eq!(
        ctx.desktop.calls(),
        vec![DesktopCall::Page(
            "https://github.com/froggugugugu/arin-term/releases/tag/v99.0.0".to_owned()
        )]
    );

    // 新しい版を出しているあいだは、確かめられなかった結果で上書きしない
    use_releases(handle, Err("通信できない"));
    update::check(handle, true);
    wait_check(handle);
    assert_eq!(
        update_notice(handle, window),
        Some(UpdateNotice::Available {
            version: "99.0.0".to_owned()
        })
    );

    // オンのまま同じ値を送っても問い合わせない。オフに戻すと知らせを消す
    call(
        handle,
        window,
        "set_check_updates",
        json!({ "enabled": true }),
    )
    .expect("切り替えられなかった");
    call(
        handle,
        window,
        "set_check_updates",
        json!({ "enabled": false }),
    )
    .expect("切り替えられなかった");
    wait_check(handle);
    assert_eq!(*lock(&fake.calls), 1);
    assert_eq!(update_notice(handle, window), None);
    call(handle, window, "dismiss_update_notice", json!({})).expect("閉じられなかった");
    ctx.desktop.clear();
}

/// テスト用の名前付きのペーストボードの名前。
fn pasteboard_name(kind: &str) -> String {
    format!(
        "dev.arin.test.desktop-commands.{kind}.{}",
        std::process::id()
    )
}

/// ターミナルの右クリックの「コピー」「ペースト」(ADR-020)と、真ん中クリックの貼り付けの切替。
fn クリップボードの文字列を読み書きし_真ん中クリックの切替は写しに出る(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    call(
        handle,
        window,
        "write_clipboard_text",
        json!({ "text": "echo 日本語\nls" }),
    )
    .expect("書けなかった");
    assert_eq!(
        ctx.pasteboard.read_text().as_deref(),
        Some("echo 日本語\nls")
    );
    // 空の文字列は書かない(中身を消さない)
    call(
        handle,
        window,
        "write_clipboard_text",
        json!({ "text": "" }),
    )
    .expect("書けなかった");
    assert_eq!(
        ctx.pasteboard.read_text().as_deref(),
        Some("echo 日本語\nls")
    );
    let read: String = call(handle, window, "read_clipboard_text", json!({}))
        .expect("読めなかった")
        .deserialize()
        .expect("文字列でなかった");
    assert_eq!(read, "echo 日本語\nls");

    let snapshot = |handle, window| -> AppSnapshot {
        call(handle, window, "get_app_snapshot", json!({}))
            .expect("写しを取れなかった")
            .deserialize()
            .expect("写しを読めなかった")
    };
    assert!(snapshot(handle, window).middle_click_paste, "既定は真");
    call(
        handle,
        window,
        "set_middle_click_paste",
        json!({ "enabled": false }),
    )
    .expect("切り替えられなかった");
    assert!(!snapshot(handle, window).middle_click_paste);
    call(
        handle,
        window,
        "set_middle_click_paste",
        json!({ "enabled": true }),
    )
    .expect("切り替えられなかった");
    assert!(snapshot(handle, window).middle_click_paste);
}

const CHECKS: &[(&str, Check)] = &[
    (
        "相対パスのコピーで_src/a_b.ts_がペーストボードに入る",
        相対パスのコピーで_src_a_b_ts_がペーストボードに入る,
    ),
    (
        "絶対パスのコピーでルートとつないだパスが入る",
        絶対パスのコピーでルートとつないだパスが入る,
    ),
    (
        "ルートの絶対パスをコピーできる",
        ルートの絶対パスをコピーできる,
    ),
    (
        "挿入で偽_pty_に_src/a\\_b.ts_と空白が書かれ改行が書かれない",
        挿入で偽_pty_に_src_a_b_ts_と空白が書かれ改行が書かれない,
    ),
    (
        "別のプロジェクトのターミナルへは絶対パスが書かれる",
        別のプロジェクトのターミナルへは絶対パスが書かれる,
    ),
    (
        "挿入先のターミナルが無い_起動前_終了済みなら何も書かず失敗を返す",
        挿入先のターミナルが無い_起動前_終了済みなら何も書かず失敗を返す,
    ),
    (
        "finder_で表示と開くは偽のポートに正しいパスで届く",
        finder_で表示と開くは偽のポートに正しいパスで届く,
    ),
    (
        "開くはファイルだけでフォルダ_リンク_実行できるファイルは渡さない",
        開くはファイルだけでフォルダ_リンク_実行できるファイルは渡さない,
    ),
    (
        "開くは実行につながる種類の拡張子のファイルを渡さない",
        開くは実行につながる種類の拡張子のファイルを渡さない,
    ),
    (
        "ルートの外とドットドットは拒否する",
        ルートの外とドットドットは拒否する,
    ),
    (
        "新しい版の確認は知らせを残し_ページは版から作った_url_で開く",
        新しい版の確認は知らせを残し_ページは版から作った_url_で開く,
    ),
    (
        "クリップボードの文字列を読み書きし_真ん中クリックの切替は写しに出る",
        クリップボードの文字列を読み書きし_真ん中クリックの切替は写しに出る,
    ),
];

fn main() {
    let temp = support::TempDir::new("desktop-commands");
    let temp_root = temp.path().to_path_buf();
    let state_dir = temp_root.join("state");
    let work = temp_root.join("work");
    for dir in [&state_dir, &work] {
        fs::create_dir(dir).expect("一時フォルダの中にフォルダを作れなかった");
    }

    // ペーストボードは名前付きのもの。利用者のクリップボードを渡さない。
    let text_pasteboard = MacPasteboard::named(pasteboard_name("text"));
    let file_pasteboard = MacFilePasteboard::named(pasteboard_name("files"));
    assert!(
        !text_pasteboard.is_general() && !file_pasteboard.is_general(),
        "ユーザーのクリップボードを使おうとした"
    );

    let picker = Arc::new(FakePicker::default());
    let control = Arc::new(Control::default());
    let desktop = Arc::new(FakeDesktop::default());
    let launch =
        arin_lib::launch::LaunchConfig::from_inputs(None, Some(state_dir.as_os_str()), None)
            .unwrap_or_else(|e| panic!("組み立ての設定を作れなかった: {e}"))
            .with_terminal_env(arin_lib::launch::TerminalEnv {
                home: Some(PathBuf::from("/Users/someone")),
                user: None,
                vars: vec![("HOME".into(), "/Users/someone".into())],
            });
    let project_ports = ProjectPorts {
        ids: Arc::new(UrandomIdSource::new()),
        pasteboard: Arc::new(text_pasteboard.clone()),
        picker: Some(Arc::clone(&picker) as Arc<dyn FolderPicker>),
    };
    let app = arin_lib::builder_with_desktop::<MockRuntime>(
        launch,
        Arc::new(FakePty(Arc::clone(&control))),
        Arc::new(|| ShellDefaults {
            shell: PathBuf::from("/bin/zsh"),
            locale: None,
        }),
        PersistencePorts::native(state_dir.clone()),
        project_ports,
        TreePorts {
            fs: Arc::new(LocalFileSystem::new()),
            watcher: Arc::new(FsEventsWatcher::new()),
            file_pasteboard: Arc::new(file_pasteboard.clone()),
        },
        Arc::clone(&desktop) as Arc<dyn DesktopPort>,
    )
    .build(tauri::generate_context!())
    .unwrap_or_else(|e| panic!("組み立てに失敗した: {e}"));

    let ctx = Ctx {
        picker,
        control,
        desktop,
        pasteboard: text_pasteboard.clone(),
        temp_root,
        work,
        main_thread: thread::current().id(),
    };
    let passed = Rc::new(Cell::new(0_usize));
    let count = Rc::clone(&passed);

    app.run_return(move |handle, event| {
        if let RunEvent::Ready = event {
            let window = handle
                .get_webview_window("main")
                .expect("設定にあるウィンドウ main が作られなかった");
            for (name, check) in CHECKS {
                check(handle, &window, &ctx);
                println!("test {name} ... ok");
                count.set(count.get() + 1);
            }
            for window in handle.webview_windows().into_values() {
                window.destroy().expect("ウィンドウを閉じられなかった");
            }
        }
        arin_lib::lifecycle::on_run_event(handle, event);
    });
    text_pasteboard.release();
    file_pasteboard.release();
    assert_eq!(passed.get(), CHECKS.len(), "起動の完了まで進まなかった");
    println!("test result: ok. {} passed", CHECKS.len());
}
