//! エディタのコマンドと ⌘W・前後のタブ・終了とプロジェクトを外すときの確認(ADR-015 の 4・5・6、計画書 T81d・T81e)。
//!
//! - 状態フォルダに 1 プロジェクトの `state.json` を置いて起動する(起動時にタブが 1 枚できる)。プロジェクトのフォルダは
//!   `canonicalize` した一時フォルダに作る(macOS の一時領域は `/var` → `/private/var` のリンクを含む)
//! - PTY は偽物(起動と終了を記録する)。文書の読み書きは本物(`LocalDocuments`)で、一時のプロジェクトのフォルダだけに触れる
//! - ⌘W・前後のタブはメニューの受け口(`menu::on_menu_event`)を直接呼び、終了の要求は `lifecycle::request_quit` を呼ぶ
//! - 1 回の起動の中で、確認を順に行う(前の確認の状態を引き継ぐ。各確認の頭に前提を書く)
//!
//! macOS ではメニューの部品をメインスレッドでしか作れないため、`harness = false` で走らせる(`app_builder` と同じ)。

mod support;

use std::fs;
use std::io::{self, Read};
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use arin_core::editor::{
    CheckDocumentOutcome, DocumentNotAllowedReason, EditorCloseRequest, EditorId, LineEnding,
    OpenDocumentOutcome, ReadOnlyReason, SaveDocumentOutcome,
};
use arin_core::layout::theme::ThemeMode;
use arin_core::paths::RelPath;
use arin_core::ports::fs::{DirEntry, EntryKind};
use arin_core::ports::watch::WatchBatch;
use arin_core::ports::{ChildExit, PtyError, PtyPort, PtySession, PtySize, SpawnedPty};
use arin_core::terminal::ShellLaunchSpec;
use arin_core::tree::exclusion::ExclusionRule;
use arin_core::workspace::{AppSnapshot, CloseEditorOutcome, QuitRequest, RemoveProjectOutcome};
use arin_lib::SNAPSHOT_EVENT;
use arin_lib::events::{EDITOR_CLOSE_REQUESTED_EVENT, QUIT_REQUESTED_EVENT};
use arin_lib::sessions::ShellDefaults;
use serde::de::DeserializeOwned;
use serde_json::json;
use support::{request, request_with};
use tauri::ipc::InvokeBody;
use tauri::test::{MockRuntime, get_ipc_response};
use tauri::{AppHandle, Listener, Manager, RunEvent, WebviewWindow};

const WAIT: Duration = Duration::from_secs(10);
const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

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
// 偽の PTY
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Control {
    spawned: AtomicUsize,
    feed: Mutex<Option<Sender<Vec<u8>>>>,
    killed: AtomicBool,
}

struct FakePty(Arc<Control>);

impl PtyPort for FakePty {
    fn spawn(&self, _spec: &ShellLaunchSpec, _size: PtySize) -> Result<SpawnedPty, PtyError> {
        self.0.spawned.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel();
        *lock(&self.0.feed) = Some(tx);
        Ok(SpawnedPty {
            session: Box::new(FakeSession(Arc::clone(&self.0))),
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

struct FakeSession(Arc<Control>);

impl PtySession for FakeSession {
    fn write(&mut self, _data: &[u8]) -> Result<(), PtyError> {
        Ok(())
    }

    fn resize(&mut self, _size: PtySize) -> Result<(), PtyError> {
        Ok(())
    }

    fn kill(&mut self) -> Result<(), PtyError> {
        self.0.killed.store(true, Ordering::SeqCst);
        lock(&self.0.feed).take();
        Ok(())
    }

    fn try_wait(&mut self) -> Result<Option<ChildExit>, PtyError> {
        Ok(None)
    }
}

// ---------------------------------------------------------------------------
// 組み立て
// ---------------------------------------------------------------------------

struct Ctx<'a> {
    handle: &'a AppHandle<MockRuntime>,
    window: &'a WebviewWindow<MockRuntime>,
    state_dir: &'a Path,
    /// プロジェクトのルート(実パス)。
    root: &'a Path,
    /// ルートの外のフォルダ(実パス)。
    outside: &'a Path,
    control: &'a Control,
    close_requests: &'a Mutex<Vec<EditorCloseRequest>>,
    quit_requests: &'a Mutex<Vec<QuitRequest>>,
}

impl Ctx<'_> {
    fn snapshot(&self) -> AppSnapshot {
        get_ipc_response(self.window, request(self.handle, "get_app_snapshot"))
            .unwrap_or_else(|e| panic!("get_app_snapshot が失敗した: {e}"))
            .deserialize()
            .unwrap_or_else(|e| panic!("AppSnapshot として読めなかった: {e}"))
    }

    fn call<T: DeserializeOwned>(&self, cmd: &str, args: serde_json::Value) -> T {
        let body = get_ipc_response(
            self.window,
            request_with(self.handle, cmd, InvokeBody::Json(args)),
        )
        .unwrap_or_else(|e| panic!("{cmd} が失敗した: {e}"));
        // 応答に絶対パスを載せない(ルートもルートの外も)。
        let text = serde_json::to_string(
            &body
                .deserialize::<serde_json::Value>()
                .unwrap_or_else(|e| panic!("{cmd} の応答を読めなかった: {e}")),
        )
        .expect("JSON にできる");
        for path in [self.root, self.outside] {
            assert!(
                !text.contains(path.to_str().expect("UTF-8 のパス")),
                "{cmd} の応答に絶対パスがある: {text}"
            );
        }
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{cmd} の応答の形が違う: {e}"))
    }

    fn call_err(&self, cmd: &str, args: serde_json::Value) -> String {
        match get_ipc_response(
            self.window,
            request_with(self.handle, cmd, InvokeBody::Json(args)),
        ) {
            Ok(_) => panic!("{cmd} が成功した"),
            Err(e) => e.to_string(),
        }
    }

    fn open(&self, path: &str) -> OpenDocumentOutcome {
        self.call(
            "open_document",
            json!({ "projectId": support::project_id(1).as_str(), "path": path }),
        )
    }

    fn opened(&self, path: &str) -> (EditorId, String, LineEnding, Option<ReadOnlyReason>) {
        match self.open(path) {
            OpenDocumentOutcome::Opened {
                editor_id,
                text,
                line_ending,
                read_only,
            } => (editor_id, text, line_ending, read_only),
            other => panic!("{path} を開けなかった: {other:?}"),
        }
    }

    /// `mode` は `normal`・`overwrite`・`create`。
    fn save(&self, id: &EditorId, text: &str, mode: &str) -> SaveDocumentOutcome {
        self.call(
            "save_document",
            json!({ "editorId": id.as_str(), "text": text, "mode": mode }),
        )
    }

    /// `with_text` は `never`・`if-changed`・`always`。
    fn check(&self, id: &EditorId, with_text: &str) -> CheckDocumentOutcome {
        self.call(
            "check_document",
            json!({ "editorId": id.as_str(), "withText": with_text }),
        )
    }

    /// 本文つきの確認の応答(本文と印)。
    fn check_text(&self, id: &EditorId, with_text: &str) -> (String, u32) {
        match self.check(id, with_text) {
            CheckDocumentOutcome::Changed {
                text: Some(text),
                token: Some(token),
                ..
            } => (text, token),
            other => panic!("本文が返らない: {other:?}"),
        }
    }

    fn adopt(&self, id: &EditorId, token: u32) -> bool {
        self.call(
            "adopt_document",
            json!({ "editorId": id.as_str(), "token": token }),
        )
    }

    fn set_dirty(&self, id: &EditorId, dirty: bool) {
        self.call::<()>(
            "set_document_dirty",
            json!({ "editorId": id.as_str(), "dirty": dirty }),
        );
    }

    fn close(&self, id: &EditorId, discard: bool) -> CloseEditorOutcome {
        self.call(
            "close_editor",
            json!({ "editorId": id.as_str(), "discard": discard }),
        )
    }

    fn file(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    fn shown_editor(&self) -> Option<EditorId> {
        self.snapshot().projects[0].shown_editor_id.clone()
    }

    fn state_json(&self) -> serde_json::Value {
        let text = fs::read_to_string(self.state_dir.join(support::STATE_FILE_NAME))
            .expect("状態ファイルを読めなかった");
        serde_json::from_str(&text).expect("状態ファイルが JSON でない")
    }

    /// 写しの未保存の印。
    fn dirty(&self, id: &EditorId) -> bool {
        self.snapshot()
            .editors
            .iter()
            .find(|e| &e.id == id)
            .unwrap_or_else(|| panic!("写しに {id} が無い"))
            .dirty
    }
}

type Check = fn(&Ctx<'_>);

const CHECKS: &[(&str, Check)] = &[
    (
        "md_を開くと中身が返り写しにタブが載る",
        md_を開くと中身が返り写しにタブが載る,
    ),
    (
        "md_でないものとルートの外は開かない",
        md_でないものとルートの外は開かない,
    ),
    (
        "ハードリンクは読み取り専用で開き保存しない",
        ハードリンクは読み取り専用で開き保存しない,
    ),
    (
        "保存すると中身が変わり未保存の印は画面が送るまで残る",
        保存すると中身が変わり未保存の印は画面が送るまで残る,
    ),
    (
        "外で書き換えたあとの保存は_changed_outside_で中身はそのまま",
        外で書き換えたあとの保存は_changed_outside_で中身はそのまま,
    ),
    ("上書き保存を選ぶと置き換える", 上書き保存を選ぶと置き換える),
    (
        "check_document_は外の変更を見つけ状態を変えない",
        check_document_は外の変更を見つけ状態を変えない,
    ),
    (
        "当てた版を_adopt_document_で伝えると持ち直す",
        当てた版を_adopt_document_で伝えると持ち直す,
    ),
    (
        "読み直すを選ぶと変わっていなくても本文を返す",
        読み直すを選ぶと変わっていなくても本文を返す,
    ),
    (
        "消えたファイルは_missing_で作るを選ぶと作る",
        消えたファイルは_missing_で作るを選ぶと作る,
    ),
    (
        "親のフォルダが無いと作れない理由を返す",
        親のフォルダが無いと作れない理由を返す,
    ),
    (
        "前後のタブはターミナルのあとにエディタへ進む",
        前後のタブはターミナルのあとにエディタへ進む,
    ),
    (
        "未保存の表示中のエディタで_cmd_w_を押すと閉じずに_editor_close_requested",
        未保存の表示中のエディタで_cmd_w_を押すと閉じずに_editor_close_requested,
    ),
    (
        "未保存が無ければ_cmd_w_でエディタだけ閉じターミナルは続く",
        未保存が無ければ_cmd_w_でエディタだけ閉じターミナルは続く,
    ),
    (
        "未保存のエディタは確認してから閉じる",
        未保存のエディタは確認してから閉じる,
    ),
    (
        "未保存があれば終了の要求で確認を求める",
        未保存があれば終了の要求で確認を求める,
    ),
    (
        "未保存があればプロジェクトを外す前に確認を求める",
        未保存があればプロジェクトを外す前に確認を求める,
    ),
    (
        "保存の一時ファイルの名前は_tree_changed_に出ない",
        保存の一時ファイルの名前は_tree_changed_に出ない,
    ),
    (
        "set_editor_vim_で写しが届き保存される",
        set_editor_vim_で写しが届き保存される,
    ),
    (
        "set_theme_で写しが届き保存される",
        set_theme_で写しが届き保存される,
    ),
    ("無いエディタの_id_は失敗する", 無いエディタの_id_は失敗する),
];

/// 前提: エディタは 0 個。`docs/a.md` は BOM つき・CRLF。
fn md_を開くと中身が返り写しにタブが載る(ctx: &Ctx<'_>) {
    assert!(ctx.snapshot().editors.is_empty());
    let (id, text, line_ending, read_only) = ctx.opened("docs/a.md");
    assert_eq!(text, "# A\r\nbody\r\n", "BOM を外して渡す");
    assert_eq!(line_ending, LineEnding::Crlf);
    assert_eq!(read_only, None);

    let snapshot = ctx.snapshot();
    assert_eq!(snapshot.editors.len(), 1);
    let tab = &snapshot.editors[0];
    assert_eq!(tab.id, id);
    assert_eq!(tab.path, RelPath::parse("docs/a.md").unwrap());
    assert_eq!(tab.label, "a.md");
    assert!(!tab.dirty);
    assert_eq!(snapshot.projects[0].editor_ids, vec![id.clone()]);
    assert_eq!(snapshot.projects[0].shown_editor_id, Some(id.clone()));

    // 同じファイルは同じタブを出す(本文は送らない)。
    ctx.call::<()>(
        "select_terminal",
        json!({ "terminalId": snapshot.projects[0].terminal_ids[0].as_str() }),
    );
    assert_eq!(
        ctx.shown_editor(),
        None,
        "ターミナルを選ぶとエディタは隠れる"
    );
    assert_eq!(
        ctx.open("docs/a.md"),
        OpenDocumentOutcome::AlreadyOpen {
            editor_id: id.clone()
        }
    );
    assert_eq!(ctx.shown_editor(), Some(id.clone()));
    assert_eq!(ctx.snapshot().editors.len(), 1);

    // select_editor でも表示に戻る。
    ctx.call::<()>(
        "select_terminal",
        json!({ "terminalId": snapshot.projects[0].terminal_ids[0].as_str() }),
    );
    ctx.call::<()>("select_editor", json!({ "editorId": id.as_str() }));
    assert_eq!(ctx.shown_editor(), Some(id));
}

/// 前提: `docs/a.md` が開いている。
fn md_でないものとルートの外は開かない(ctx: &Ctx<'_>) {
    let not_allowed = |reason| OpenDocumentOutcome::NotAllowed { reason };
    assert_eq!(
        ctx.open("b.txt"),
        not_allowed(DocumentNotAllowedReason::NotMarkdown)
    );
    assert_eq!(
        ctx.open("out/o.md"),
        not_allowed(DocumentNotAllowedReason::OutsideRoot)
    );
    assert_eq!(
        ctx.open("link.md"),
        not_allowed(DocumentNotAllowedReason::Link)
    );
    assert_eq!(
        ctx.open("big.md"),
        not_allowed(DocumentNotAllowedReason::TooLarge)
    );
    assert_eq!(
        ctx.open("nul.md"),
        not_allowed(DocumentNotAllowedReason::NotText)
    );
    assert_eq!(
        ctx.open("latin1.md"),
        not_allowed(DocumentNotAllowedReason::NotText)
    );
    assert_eq!(
        ctx.open("folder.md"),
        not_allowed(DocumentNotAllowedReason::Directory)
    );
    assert_eq!(ctx.open("nothing.md"), OpenDocumentOutcome::NotFound);
    assert_eq!(
        ctx.snapshot().editors.len(),
        1,
        "開かなかったものはタブにしない"
    );
}

/// 前提: `hard.md` と `hard-2.md` は同じファイル。
fn ハードリンクは読み取り専用で開き保存しない(ctx: &Ctx<'_>) {
    let (id, text, _, read_only) = ctx.opened("hard.md");
    assert_eq!(text, "hard\n");
    assert_eq!(read_only, Some(ReadOnlyReason::HardLink));
    assert_eq!(
        ctx.snapshot()
            .editors
            .iter()
            .find(|e| e.id == id)
            .unwrap()
            .read_only,
        Some(ReadOnlyReason::HardLink)
    );
    assert_eq!(
        ctx.save(&id, "changed\n", "normal"),
        SaveDocumentOutcome::ReadOnly
    );
    assert_eq!(
        ctx.save(&id, "changed\n", "overwrite"),
        SaveDocumentOutcome::ReadOnly
    );
    assert_eq!(
        ctx.save(&id, "changed\n", "create"),
        SaveDocumentOutcome::ReadOnly
    );
    assert_eq!(fs::read(ctx.file("hard.md")).unwrap(), b"hard\n");
    assert_eq!(ctx.close(&id, false), CloseEditorOutcome::Closed);
}

fn a_md(ctx: &Ctx<'_>) -> EditorId {
    match ctx.open("docs/a.md") {
        OpenDocumentOutcome::AlreadyOpen { editor_id } => editor_id,
        OpenDocumentOutcome::Opened { editor_id, .. } => editor_id,
        other => panic!("docs/a.md を開けなかった: {other:?}"),
    }
}

/// 前提: `docs/a.md` が開いていて未保存の変更は無い。
fn 保存すると中身が変わり未保存の印は画面が送るまで残る(ctx: &Ctx<'_>) {
    let id = a_md(ctx);
    ctx.set_dirty(&id, true);
    assert!(ctx.dirty(&id));
    assert_eq!(
        ctx.save(&id, "# A2\r\nbody\r\n", "normal"),
        SaveDocumentOutcome::Saved
    );
    let mut expected = UTF8_BOM.to_vec();
    expected.extend_from_slice(b"# A2\r\nbody\r\n");
    assert_eq!(
        fs::read(ctx.file("docs/a.md")).unwrap(),
        expected,
        "BOM を付け直す"
    );
    // 保存中に打った分があるかは画面しか知らない。印は画面が送り直すまで消さない(その間の ⌘Q を確認なしで通さない)。
    assert!(ctx.dirty(&id), "保存で未保存の印を消した");
    ctx.set_dirty(&id, false);
    // 一時ファイルが残らない。
    let names: Vec<String> = fs::read_dir(ctx.file("docs"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["a.md".to_owned()]);
    // 続けて保存しても外で変わったとはみなさない(版を持ち直している)。
    assert_eq!(
        ctx.save(&id, "# A2\r\nbody2\r\n", "normal"),
        SaveDocumentOutcome::Saved
    );
}

/// 前提: `docs/a.md` が開いていて、最後の保存から外では変わっていない。
fn 外で書き換えたあとの保存は_changed_outside_で中身はそのまま(ctx: &Ctx<'_>) {
    let id = a_md(ctx);
    fs::write(ctx.file("docs/a.md"), b"outside\n").unwrap();
    assert_eq!(
        ctx.save(&id, "# mine\n", "normal"),
        SaveDocumentOutcome::ChangedOutside
    );
    assert_eq!(fs::read(ctx.file("docs/a.md")).unwrap(), b"outside\n");
    // 「作る」は同じ名前があれば書かない。
    assert_eq!(
        ctx.save(&id, "# mine\n", "create"),
        SaveDocumentOutcome::ChangedOutside
    );
    assert_eq!(fs::read(ctx.file("docs/a.md")).unwrap(), b"outside\n");
}

/// 前提: 直前の確認で `docs/a.md` が外で書き換わっている。
fn 上書き保存を選ぶと置き換える(ctx: &Ctx<'_>) {
    let id = a_md(ctx);
    assert_eq!(
        ctx.save(&id, "# mine\n", "overwrite"),
        SaveDocumentOutcome::Saved
    );
    let mut expected = UTF8_BOM.to_vec();
    expected.extend_from_slice(b"# mine\n");
    assert_eq!(fs::read(ctx.file("docs/a.md")).unwrap(), expected);
    assert_eq!(
        ctx.save(&id, "# mine 2\n", "normal"),
        SaveDocumentOutcome::Saved
    );
}

/// 前提: `docs/a.md` が開いていて、最後の保存から外では変わっていない。未保存の印は無い。
fn check_document_は外の変更を見つけ状態を変えない(ctx: &Ctx<'_>) {
    let id = a_md(ctx);
    assert_eq!(ctx.check(&id, "never"), CheckDocumentOutcome::Unchanged);
    assert_eq!(
        ctx.check(&id, "if-changed"),
        CheckDocumentOutcome::Unchanged
    );
    fs::write(ctx.file("docs/a.md"), b"line 1\r\nline 2\r\n").unwrap();
    assert_eq!(
        ctx.check(&id, "never"),
        CheckDocumentOutcome::Changed {
            text: None,
            line_ending: LineEnding::Crlf,
            token: None,
        }
    );
    // 本文つきの確認でも版も未保存の印も替えない(応答を待つあいだに打ち始めたら、画面は本文を捨てる)。
    ctx.set_dirty(&id, true);
    let (text, _) = ctx.check_text(&id, "if-changed");
    assert_eq!(text, "line 1\r\nline 2\r\n");
    assert!(ctx.dirty(&id), "確認で未保存の印を消した");
    assert_eq!(
        ctx.save(&id, "mine\n", "normal"),
        SaveDocumentOutcome::ChangedOutside,
        "当てていない版を持ち直した"
    );
    assert_eq!(
        fs::read(ctx.file("docs/a.md")).unwrap(),
        b"line 1\r\nline 2\r\n"
    );
    assert!(ctx.dirty(&id));
    ctx.set_dirty(&id, false);
}

/// 前提: `docs/a.md` は外で `line 1\r\nline 2\r\n`(BOM なし)に書き換わり、まだ当てていない。
fn 当てた版を_adopt_document_で伝えると持ち直す(ctx: &Ctx<'_>) {
    let id = a_md(ctx);
    let (_, old) = ctx.check_text(&id, "if-changed");
    let (_, token) = ctx.check_text(&id, "if-changed");
    assert_ne!(old, token, "読むたびに印が替わる");
    assert!(!ctx.adopt(&id, old), "古い印で持ち直した");
    assert!(
        !ctx.adopt(&id, token.wrapping_add(1)),
        "知らない印で持ち直した"
    );
    assert_eq!(
        ctx.save(&id, "x\n", "normal"),
        SaveDocumentOutcome::ChangedOutside
    );
    assert!(ctx.adopt(&id, token));
    assert!(!ctx.adopt(&id, token), "同じ印で 2 回持ち直した");
    assert!(!ctx.dirty(&id), "持ち直しで未保存の印を付けた");
    assert_eq!(
        ctx.check(&id, "if-changed"),
        CheckDocumentOutcome::Unchanged
    );
    assert_eq!(
        ctx.save(&id, "line 1\nline 3\n", "normal"),
        SaveDocumentOutcome::Saved
    );
    // 読み直した文書には BOM が無かったので付けない。
    assert_eq!(
        fs::read(ctx.file("docs/a.md")).unwrap(),
        b"line 1\nline 3\n"
    );
    // 保存したら、それより前に読んだ版の印は使えない。
    fs::write(ctx.file("docs/a.md"), b"outside again\n").unwrap();
    let (_, before_save) = ctx.check_text(&id, "if-changed");
    assert_eq!(
        ctx.save(&id, "line 1\nline 4\n", "overwrite"),
        SaveDocumentOutcome::Saved
    );
    assert!(!ctx.adopt(&id, before_save), "保存より前の版を持ち直した");
    assert_eq!(
        ctx.save(&id, "line 1\nline 3\n", "normal"),
        SaveDocumentOutcome::Saved
    );
}

/// 前提: `docs/a.md` は手元の最後の保存と同じ。
fn 読み直すを選ぶと変わっていなくても本文を返す(ctx: &Ctx<'_>) {
    let id = a_md(ctx);
    let (text, token) = ctx.check_text(&id, "always");
    assert_eq!(text, "line 1\nline 3\n");
    assert!(ctx.adopt(&id, token));
    assert_eq!(
        ctx.save(&id, "line 1\nline 3\n", "normal"),
        SaveDocumentOutcome::Saved
    );
}

/// 前提: `docs/a.md` が開いている。
fn 消えたファイルは_missing_で作るを選ぶと作る(ctx: &Ctx<'_>) {
    let id = a_md(ctx);
    fs::rename(ctx.file("docs/a.md"), ctx.file("docs/a-moved.md")).unwrap();
    assert_eq!(ctx.check(&id, "if-changed"), CheckDocumentOutcome::Missing);
    assert_eq!(
        ctx.save(&id, "# again\n", "normal"),
        SaveDocumentOutcome::Missing
    );
    assert_eq!(
        ctx.save(&id, "# again\n", "overwrite"),
        SaveDocumentOutcome::Missing,
        "上書き保存では作らない"
    );
    assert!(
        !ctx.file("docs/a.md").exists(),
        "見つからないときは作らない"
    );
    assert_eq!(
        ctx.save(&id, "# again\n", "create"),
        SaveDocumentOutcome::Saved
    );
    assert_eq!(fs::read(ctx.file("docs/a.md")).unwrap(), b"# again\n");
    assert_eq!(
        fs::read(ctx.file("docs/a-moved.md")).unwrap(),
        b"line 1\nline 3\n",
        "移した先には触れない"
    );
    assert_eq!(ctx.check(&id, "never"), CheckDocumentOutcome::Unchanged);
}

/// 前提: `sub/c.md` は開いていない。
fn 親のフォルダが無いと作れない理由を返す(ctx: &Ctx<'_>) {
    let (id, ..) = ctx.opened("sub/c.md");
    fs::rename(ctx.file("sub"), ctx.file("sub-moved")).unwrap();
    assert_eq!(ctx.check(&id, "if-changed"), CheckDocumentOutcome::Missing);
    for mode in ["normal", "overwrite", "create"] {
        match ctx.save(&id, "c2\n", mode) {
            SaveDocumentOutcome::Failed { message } => {
                assert_eq!(message, "保存先のフォルダが見つかりません", "{mode}");
            }
            other => panic!("{mode}: 理由の分かる失敗にならない: {other:?}"),
        }
    }
    assert!(!ctx.file("sub").exists(), "フォルダを作った");
    assert_eq!(ctx.close(&id, true), CloseEditorOutcome::Closed);
    fs::rename(ctx.file("sub-moved"), ctx.file("sub")).unwrap();
}

/// 前提: `docs/a.md` を表示中。タブはターミナル 1 枚とエディタ 1 個。
fn 前後のタブはターミナルのあとにエディタへ進む(ctx: &Ctx<'_>) {
    let id = a_md(ctx);
    assert_eq!(ctx.shown_editor(), Some(id.clone()));
    arin_lib::menu::on_menu_event(ctx.handle, "terminal.next-tab");
    assert_eq!(ctx.shown_editor(), None, "エディタの次は先頭のターミナル");
    arin_lib::menu::on_menu_event(ctx.handle, "terminal.next-tab");
    assert_eq!(
        ctx.shown_editor(),
        Some(id.clone()),
        "ターミナルの次はエディタ"
    );
    arin_lib::menu::on_menu_event(ctx.handle, "terminal.previous-tab");
    assert_eq!(ctx.shown_editor(), None);
    arin_lib::menu::on_menu_event(ctx.handle, "terminal.previous-tab");
    assert_eq!(ctx.shown_editor(), Some(id));
}

/// 前提: `docs/a.md` を表示中。
fn 未保存の表示中のエディタで_cmd_w_を押すと閉じずに_editor_close_requested(
    ctx: &Ctx<'_>,
) {
    let id = a_md(ctx);
    ctx.set_dirty(&id, true);
    let terminals = ctx.snapshot().terminals.len();
    lock(ctx.close_requests).clear();
    arin_lib::menu::on_menu_event(ctx.handle, "terminal.close-tab");
    assert!(
        wait_until(|| !lock(ctx.close_requests).is_empty()),
        "editor-close-requested が届かない"
    );
    assert_eq!(
        lock(ctx.close_requests).as_slice(),
        &[EditorCloseRequest {
            editor_id: id.clone()
        }]
    );
    let snapshot = ctx.snapshot();
    assert_eq!(snapshot.editors.len(), 1, "未保存のエディタを閉じた");
    assert_eq!(
        snapshot.terminals.len(),
        terminals,
        "裏のターミナルを閉じた"
    );
    assert_eq!(ctx.shown_editor(), Some(id));
}

/// 前提: `docs/a.md` を表示中で未保存。
fn 未保存が無ければ_cmd_w_でエディタだけ閉じターミナルは続く(
    ctx: &Ctx<'_>,
) {
    let id = a_md(ctx);
    ctx.set_dirty(&id, false);
    let terminals = ctx.snapshot().terminals.len();
    lock(ctx.close_requests).clear();
    arin_lib::menu::on_menu_event(ctx.handle, "terminal.close-tab");
    assert!(
        wait_until(|| ctx.snapshot().editors.is_empty()),
        "⌘W でエディタが閉じない"
    );
    assert_eq!(ctx.snapshot().terminals.len(), terminals);
    assert!(
        !ctx.control.killed.load(Ordering::SeqCst),
        "シェルを終わらせた"
    );
    assert!(lock(ctx.close_requests).is_empty());
    assert_eq!(ctx.shown_editor(), None);
}

/// 前提: エディタは 0 個。
fn 未保存のエディタは確認してから閉じる(ctx: &Ctx<'_>) {
    let (id, ..) = ctx.opened("docs/a.md");
    ctx.set_dirty(&id, true);
    assert_eq!(ctx.close(&id, false), CloseEditorOutcome::NeedsConfirmation);
    assert_eq!(ctx.snapshot().editors.len(), 1);
    assert_eq!(ctx.close(&id, true), CloseEditorOutcome::Closed);
    assert!(ctx.snapshot().editors.is_empty());
}

/// 前提: エディタは 0 個。動作中のターミナルは無い(偽の PTY の前面は分からない)。
fn 未保存があれば終了の要求で確認を求める(ctx: &Ctx<'_>) {
    let (id, ..) = ctx.opened("docs/a.md");
    ctx.set_dirty(&id, true);
    lock(ctx.quit_requests).clear();
    arin_lib::lifecycle::request_quit(ctx.handle);
    assert!(
        wait_until(|| !lock(ctx.quit_requests).is_empty()),
        "quit-requested が届かない"
    );
    let request = lock(ctx.quit_requests)[0].clone();
    assert!(request.running.is_empty());
    assert_eq!(request.unsaved.len(), 1);
    assert_eq!(request.unsaved[0].editor_id, id);
    assert_eq!(request.unsaved[0].project_name, "project-1");
    assert_eq!(
        request.unsaved[0].path,
        RelPath::parse("docs/a.md").unwrap()
    );
    assert!(
        !ctx.handle.webview_windows().is_empty(),
        "確認を待たずに終わった"
    );
}

/// 前提: `docs/a.md` が未保存。
fn 未保存があればプロジェクトを外す前に確認を求める(ctx: &Ctx<'_>) {
    let outcome: RemoveProjectOutcome = ctx.call(
        "remove_project",
        json!({ "projectId": support::project_id(1).as_str(), "confirmed": false }),
    );
    match outcome {
        RemoveProjectOutcome::NeedsConfirmation { running, unsaved } => {
            assert!(running.is_empty());
            assert_eq!(unsaved.len(), 1);
            assert_eq!(unsaved[0].path, RelPath::parse("docs/a.md").unwrap());
        }
        other => panic!("確認を求めない: {other:?}"),
    }
    assert_eq!(ctx.snapshot().projects.len(), 1, "外してしまった");
    // 後の確認のために未保存を消しておく。
    let id = ctx.snapshot().editors[0].id.clone();
    ctx.set_dirty(&id, false);
}

/// 前提: なし(規則の確認)。
fn 保存の一時ファイルの名前は_tree_changed_に出ない(ctx: &Ctx<'_>) {
    let root = ctx.root;
    let batch = WatchBatch {
        paths: vec![
            root.join(".arin-save-1-0"),
            root.join("docs/.arin-save-1-1"),
            root.join(".arin-copy-1-0/x.md"),
        ],
        rescan: false,
    };
    let changes = arin_lib::tree_watch::summarize(root, &batch, &ExclusionRule::new(false));
    assert!(changes.dirs.is_empty(), "{changes:?}");
    assert!(!changes.full_reload);
    let changes = arin_lib::tree_watch::summarize(root, &batch, &ExclusionRule::new(true));
    assert!(
        changes.dirs.is_empty(),
        "隠しファイル表示でも知らせない: {changes:?}"
    );

    // 一覧は隠しファイル表示が OFF のとき出さない(ON なら残ったものを出す)。
    let entries = vec![
        DirEntry {
            name: ".arin-save-1-2".to_owned(),
            kind: EntryKind::File,
        },
        DirEntry {
            name: "a.md".to_owned(),
            kind: EntryKind::File,
        },
    ];
    let names = |rule| -> Vec<String> {
        arin_lib::tree::visible_entries(entries.clone(), rule)
            .into_iter()
            .map(|e| e.name)
            .collect()
    };
    assert_eq!(names(ExclusionRule::new(false)), vec!["a.md".to_owned()]);
    assert_eq!(names(ExclusionRule::new(true)).len(), 2);
}

/// 前提: `ui.editorVim` は無い。
fn set_editor_vim_で写しが届き保存される(ctx: &Ctx<'_>) {
    assert!(!ctx.snapshot().editor.vim);
    ctx.call::<()>("set_editor_vim", json!({ "enabled": true }));
    assert!(ctx.snapshot().editor.vim);
    arin_lib::lifecycle::on_window_focus_changed(ctx.handle, false);
    assert!(
        wait_until(|| ctx.state_json()["ui"]["editorVim"] == json!(true)),
        "ui.editorVim が書かれない: {}",
        ctx.state_json()
    );
    // 「既定に戻す」では変わらない。
    ctx.call::<()>("reset_appearance", json!({}));
    assert!(ctx.snapshot().editor.vim);

    ctx.call::<()>("set_editor_vim", json!({ "enabled": false }));
    assert!(!ctx.snapshot().editor.vim);
    arin_lib::lifecycle::on_window_focus_changed(ctx.handle, false);
    assert!(
        wait_until(|| ctx.state_json()["ui"].get("editorVim").is_none()),
        "偽を書いた: {}",
        ctx.state_json()
    );
    // 保存する内容にエディタのタブは入らない(再起動で開き直さない)。
    let text = fs::read_to_string(ctx.state_dir.join(support::STATE_FILE_NAME)).unwrap();
    assert!(!text.contains("a.md"), "{text}");
}

/// 前提: `ui.theme` は無い(システム)。
fn set_theme_で写しが届き保存される(ctx: &Ctx<'_>) {
    let theme = || serde_json::to_value(ctx.snapshot()).unwrap()["theme"].clone();
    // アプリの外観へ当てる口に、記録するだけの中身を入れる(模擬ランタイムでは本物は入らない)。
    let applied: Arc<Mutex<Vec<ThemeMode>>> = Arc::default();
    let record = Arc::clone(&applied);
    ctx.handle
        .state::<arin_lib::ThemeApplier>()
        .install(move |theme| record.lock().unwrap().push(theme));

    assert_eq!(theme(), json!("system"));
    ctx.call::<()>("set_theme", json!({ "theme": "dark" }));
    // 同じ値をもう一度送っても、当て直さない。
    ctx.call::<()>("set_theme", json!({ "theme": "dark" }));
    assert_eq!(theme(), json!("dark"));
    arin_lib::lifecycle::on_window_focus_changed(ctx.handle, false);
    assert!(
        wait_until(|| ctx.state_json()["ui"]["theme"] == json!("dark")),
        "ui.theme が書かれない: {}",
        ctx.state_json()
    );
    // 「既定に戻す」では変わらない。
    ctx.call::<()>("reset_appearance", json!({}));
    assert_eq!(theme(), json!("dark"));

    ctx.call::<()>("set_theme", json!({ "theme": "system" }));
    assert_eq!(theme(), json!("system"));
    arin_lib::lifecycle::on_window_focus_changed(ctx.handle, false);
    assert!(
        wait_until(|| ctx.state_json()["ui"].get("theme").is_none()),
        "システムを書いた: {}",
        ctx.state_json()
    );
    assert_eq!(
        *applied.lock().unwrap(),
        vec![ThemeMode::Dark, ThemeMode::System],
        "変わったときだけ外観に当てる"
    );
}

fn 無いエディタの_id_は失敗する(ctx: &Ctx<'_>) {
    for (cmd, args) in [
        (
            "save_document",
            json!({ "editorId": "editor-999", "text": "", "mode": "normal" }),
        ),
        (
            "check_document",
            json!({ "editorId": "editor-999", "withText": "never" }),
        ),
        (
            "adopt_document",
            json!({ "editorId": "editor-999", "token": 1 }),
        ),
        (
            "set_document_dirty",
            json!({ "editorId": "editor-999", "dirty": true }),
        ),
        ("select_editor", json!({ "editorId": "editor-999" })),
        (
            "close_editor",
            json!({ "editorId": "editor-999", "discard": true }),
        ),
    ] {
        let message = ctx.call_err(cmd, args);
        assert!(message.contains("editor-999"), "{cmd}: {message}");
    }
}

/// プロジェクトのフォルダに見本のファイルを置く。
fn prepare_files(root: &Path, outside: &Path) {
    fs::create_dir(root.join("docs")).unwrap();
    let mut a = UTF8_BOM.to_vec();
    a.extend_from_slice(b"# A\r\nbody\r\n");
    fs::write(root.join("docs/a.md"), a).unwrap();
    fs::write(root.join("b.txt"), b"text\n").unwrap();
    fs::create_dir(root.join("sub")).unwrap();
    fs::write(root.join("sub/c.md"), b"c\n").unwrap();
    fs::write(root.join("big.md"), vec![b'a'; 1024 * 1024 + 1]).unwrap();
    fs::write(root.join("nul.md"), b"a\0b").unwrap();
    fs::write(root.join("latin1.md"), [0x63, 0x61, 0x66, 0xE9]).unwrap();
    fs::create_dir(root.join("folder.md")).unwrap();
    symlink(root.join("docs/a.md"), root.join("link.md")).unwrap();
    fs::write(root.join("hard.md"), b"hard\n").unwrap();
    fs::hard_link(root.join("hard.md"), root.join("hard-2.md")).unwrap();
    fs::write(outside.join("o.md"), b"outside\n").unwrap();
    symlink(outside, root.join("out")).unwrap();
}

fn main() {
    let state = support::TempDir::new("editor-commands");
    let work = support::TempDir::new("editor-commands-work");
    let base = fs::canonicalize(work.path()).expect("一時フォルダの実パスを取れなかった");
    let root = base.join("project-1");
    let outside = base.join("outside");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&outside).unwrap();
    prepare_files(&root, &outside);

    let launch = support::launch_with_projects_in(&state, std::slice::from_ref(&root), false)
        .with_terminal_env(arin_lib::launch::TerminalEnv {
            home: Some(PathBuf::from("/Users/someone")),
            user: None,
            vars: vec![("HOME".into(), "/Users/someone".into())],
        });
    let control = Arc::new(Control::default());
    let app = arin_lib::builder_with_pty::<MockRuntime>(
        launch,
        Arc::new(FakePty(Arc::clone(&control))),
        Arc::new(|| ShellDefaults {
            shell: PathBuf::from("/bin/zsh"),
            locale: None,
        }),
    )
    .channel_interceptor(|_webview, _callback, _index, _body| true)
    .build(tauri::generate_context!())
    .unwrap_or_else(|e| panic!("組み立てに失敗した: {e}"));

    let passed = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&passed);
    let state_dir = state.path().to_path_buf();
    app.run_return(move |handle, event| {
        if let RunEvent::Ready = event {
            let window = handle
                .get_webview_window("main")
                .expect("設定にあるウィンドウ main が作られなかった");
            handle.listen(SNAPSHOT_EVENT, |_| {});
            let close_requests: Arc<Mutex<Vec<EditorCloseRequest>>> = Arc::default();
            let sink = Arc::clone(&close_requests);
            handle.listen(EDITOR_CLOSE_REQUESTED_EVENT, move |event| {
                lock(&sink).push(serde_json::from_str(event.payload()).expect("中身の形が違う"));
            });
            let quit_requests: Arc<Mutex<Vec<QuitRequest>>> = Arc::default();
            let sink = Arc::clone(&quit_requests);
            handle.listen(QUIT_REQUESTED_EVENT, move |event| {
                lock(&sink).push(serde_json::from_str(event.payload()).expect("中身の形が違う"));
            });
            let ctx = Ctx {
                handle,
                window: &window,
                state_dir: &state_dir,
                root: &root,
                outside: &outside,
                control: &control,
                close_requests: &close_requests,
                quit_requests: &quit_requests,
            };
            for (name, check) in CHECKS {
                check(&ctx);
                println!("test {name} ... ok");
                count.fetch_add(1, Ordering::SeqCst);
            }
            for window in handle.webview_windows().into_values() {
                window.destroy().expect("ウィンドウを閉じられなかった");
            }
        }
        arin_lib::lifecycle::on_run_event(handle, event);
    });

    let passed = passed.load(Ordering::SeqCst);
    assert_eq!(passed, CHECKS.len(), "確認が途中で止まった");
    println!("\ntest result: ok. {passed} passed; 0 failed");
    drop(work);
}
