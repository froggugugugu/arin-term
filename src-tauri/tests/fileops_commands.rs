//! ファイル操作のコマンド(`src-tauri/src/commands/fileops.rs`、計画書 T42c)を、Tauri の模擬ランタイムで画面から呼ぶ形で確かめる。
//! FR-025〜030(作成・名称変更・コピーとペースト・ゴミ箱・移動とコピー・衝突)、FR-035(ルートは対象にしない)、
//! FR-036(実行の時点で調べ直す)、設計書 §5.2・§7.2・§12(ルートの外に出ない、絶対パスを受け取らず返さない)。
//!
//! - ファイルシステムは本物(`LocalFileSystem`)を記録の包みで囲んだもの。**ゴミ箱だけ**、一時フォルダの中の
//!   偽のゴミ箱フォルダへの移動に差し替える(本物のゴミ箱に触れない。計画書 M3 の【仮定】23)
//! - ファイルのペーストボードは名前付きのもの(`MacFilePasteboard::named`)。利用者のクリップボードに触れない
//! - プロジェクトのフォルダと状態ファイルは、すべて一時フォルダの中に作る
//! - フォルダ選択のダイアログは偽物、PTY は偽物(起動しない)
//! - 確認は順に行い、前の確認で作ったプロジェクトを次で使う
//!
//! macOS ではメニューの部品をメインスレッドでしか作れないため、`harness = false` で走らせる(`app_builder` と同じ)。

mod support;

use std::cell::Cell;
use std::collections::VecDeque;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, ThreadId};

use arin_core::fileops::{FileOpOutcome, NotAllowedReason};
use arin_core::paths::RelPath;
use arin_core::ports::fs::{DirEntry, EntryKind, FileSystemPort, FsError};
use arin_core::ports::pasteboard::{FilePasteboardPort, PasteboardPort};
use arin_core::ports::{PtyError, PtyPort, PtySize, SpawnedPty};
use arin_core::terminal::ShellLaunchSpec;
use arin_core::workspace::{AddProjectOutcome, AppSnapshot};
use arin_lib::TreePorts;
use arin_lib::persistence::PersistencePorts;
use arin_lib::projects::{FolderPicker, FolderPurpose, ProjectPorts};
use arin_lib::sessions::ShellDefaults;
use arin_platform::desktop::{MacFilePasteboard, MacPasteboard};
use arin_platform::fs::LocalFileSystem;
use arin_platform::system::UrandomIdSource;
use arin_platform::watch::FsEventsWatcher;
use serde_json::json;
use support::{request, request_with};
use tauri::ipc::{InvokeBody, InvokeResponseBody};
use tauri::test::{MockRuntime, get_ipc_response};
use tauri::{AppHandle, Manager, RunEvent, WebviewWindow};

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

// ---------------------------------------------------------------------------
// 偽のダイアログ・偽の PTY
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

/// シェルを起動しない PTY(このテストはターミナルにつながない)。
struct NoPty;

impl PtyPort for NoPty {
    fn spawn(&self, _spec: &ShellLaunchSpec, _size: PtySize) -> Result<SpawnedPty, PtyError> {
        Err(PtyError::Spawn("このテストではシェルを起動しない".into()))
    }
}

// ---------------------------------------------------------------------------
// 記録するファイルシステム(ゴミ箱だけ偽物)
// ---------------------------------------------------------------------------

/// 変更する口の呼び出し 1 回。
#[derive(Debug, Clone, PartialEq, Eq)]
enum Op {
    CreateFile(PathBuf),
    CreateDir(PathBuf),
    Move(PathBuf, PathBuf),
    Copy(PathBuf, PathBuf),
    Trash(PathBuf),
}

impl Op {
    fn paths(&self) -> Vec<&Path> {
        match self {
            Self::CreateFile(p) | Self::CreateDir(p) | Self::Trash(p) => vec![p],
            Self::Move(a, b) | Self::Copy(a, b) => vec![a, b],
        }
    }
}

#[derive(Default)]
struct Recorder {
    /// 変更する口の呼び出し(呼ばれた順)。
    ops: Mutex<Vec<Op>>,
    /// 変更する口が呼ばれたスレッド。
    threads: Mutex<Vec<ThreadId>>,
    /// 偽のゴミ箱に入れた項目(元の絶対パス → 偽のゴミ箱の中の絶対パス)。
    trashed: Mutex<Vec<(PathBuf, PathBuf)>>,
    /// ここまでに記録したすべての変更の口の呼び出し(`clear` で消さない。最後に一時フォルダの外を見るため)。
    all_ops: Mutex<Vec<Op>>,
}

impl Recorder {
    fn clear(&self) {
        lock(&self.ops).clear();
        lock(&self.threads).clear();
        lock(&self.trashed).clear();
    }

    fn note(&self, op: Op) {
        lock(&self.all_ops).push(op.clone());
        lock(&self.ops).push(op);
        lock(&self.threads).push(thread::current().id());
    }

    fn ops(&self) -> Vec<Op> {
        lock(&self.ops).clone()
    }
}

struct RecordingFs {
    inner: LocalFileSystem,
    rec: Arc<Recorder>,
    /// 偽のゴミ箱(一時フォルダの中)。
    trash_dir: PathBuf,
}

impl FileSystemPort for RecordingFs {
    fn read_dir(&self, dir: &Path) -> Result<Vec<DirEntry>, FsError> {
        self.inner.read_dir(dir)
    }

    fn entry_kind(&self, path: &Path) -> Result<EntryKind, FsError> {
        self.inner.entry_kind(path)
    }

    fn real_dir(&self, dir: &Path) -> Result<PathBuf, FsError> {
        self.inner.real_dir(dir)
    }

    fn create_file(&self, path: &Path) -> Result<(), FsError> {
        self.rec.note(Op::CreateFile(path.to_path_buf()));
        self.inner.create_file(path)
    }

    fn create_dir(&self, path: &Path) -> Result<(), FsError> {
        self.rec.note(Op::CreateDir(path.to_path_buf()));
        self.inner.create_dir(path)
    }

    fn move_entry(&self, from: &Path, to: &Path) -> Result<(), FsError> {
        self.rec
            .note(Op::Move(from.to_path_buf(), to.to_path_buf()));
        self.inner.move_entry(from, to)
    }

    fn copy_entry(&self, from: &Path, to: &Path) -> Result<(), FsError> {
        self.rec
            .note(Op::Copy(from.to_path_buf(), to.to_path_buf()));
        self.inner.copy_entry(from, to)
    }

    /// 本物のゴミ箱には入れず、一時フォルダの中の偽のゴミ箱フォルダへ移す。
    fn trash(&self, path: &Path) -> Result<Option<PathBuf>, FsError> {
        self.rec.note(Op::Trash(path.to_path_buf()));
        let name = path
            .file_name()
            .ok_or_else(|| FsError::Io("名前が無い".into()))?;
        let mut trashed = lock(&self.rec.trashed);
        let dest = self
            .trash_dir
            .join(format!("{}-{}", trashed.len(), name.to_string_lossy()));
        self.inner.move_entry(path, &dest)?;
        trashed.push((path.to_path_buf(), dest.clone()));
        Ok(Some(dest))
    }
}

// ---------------------------------------------------------------------------
// 共有するもの
// ---------------------------------------------------------------------------

struct Ctx {
    picker: Arc<FakePicker>,
    rec: Arc<Recorder>,
    /// テスト用の名前付きのペーストボード(アプリに渡したものと同じ)。
    pasteboard: MacFilePasteboard,
    /// 一時フォルダ全体(この外を変えない)。
    temp_root: PathBuf,
    /// プロジェクトのフォルダを置く一時フォルダ(状態フォルダとは別)。
    work: PathBuf,
    /// 模擬のアプリを走らせているスレッド(コマンドの受け口)。
    main_thread: ThreadId,
}

impl Ctx {
    /// 一時フォルダの中のフォルダ(作る)と、その正規化したパス。
    fn folder(&self, name: &str) -> (PathBuf, PathBuf) {
        let path = self.work.join(name);
        fs::create_dir_all(&path).expect("フォルダを作れなかった");
        let canonical = fs::canonicalize(&path).expect("正規化できなかった");
        (path, canonical)
    }

    /// 文字列に、一時フォルダの絶対パス(元の形と正規化した形)が入っていないことを確かめる。
    fn assert_no_absolute_path(&self, text: &str) {
        let canonical = fs::canonicalize(&self.temp_root).expect("正規化できなかった");
        for path in [&self.temp_root, &canonical] {
            let path = path.display().to_string();
            assert!(!text.contains(&path), "応答に絶対パスが入っている: {text}");
        }
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

/// ファイル操作のコマンドを呼び、応答を読む。応答の JSON に絶対パスが無いことも確かめる。
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
    let text = error_text(call(handle, window, cmd, args));
    ctx.assert_no_absolute_path(&text);
    text
}

fn error_text(result: Result<InvokeResponseBody, serde_json::Value>) -> String {
    match result {
        Ok(_) => panic!("失敗するはずが成功した"),
        Err(e) => e
            .as_str()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| e.to_string()),
    }
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

/// ダイアログで `path` を選んだことにして登録し、その ID を返す。
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

fn alpha_id(handle: &AppHandle<MockRuntime>, window: &WebviewWindow<MockRuntime>) -> String {
    project_id_by_name(&snapshot(handle, window), "alpha")
}

fn rel(path: &str) -> RelPath {
    RelPath::parse(path).unwrap_or_else(|e| panic!("{path:?} を相対パスにできなかった: {e}"))
}

fn done(select: Option<&str>) -> FileOpOutcome {
    FileOpOutcome::Done {
        select: select.map(rel),
    }
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{} を読めなかった: {e}", path.display()))
}

/// フォルダの中身(名前と中身。フォルダは中まで)を決まった順で並べる(変わっていないことの比較に使う)。
fn tree_of(dir: &Path) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let mut names: Vec<_> = fs::read_dir(&current)
            .expect("一時フォルダの中を読めなかった")
            .map(|e| e.expect("項目を読めなかった").path())
            .collect();
        names.sort();
        for path in names {
            let label = path
                .strip_prefix(dir)
                .expect("中の項目")
                .display()
                .to_string();
            let meta = fs::symlink_metadata(&path).expect("項目を調べられなかった");
            if meta.is_dir() {
                out.push((label, None));
                stack.push(path);
            } else if meta.file_type().is_symlink() {
                out.push((label, Some("<link>".to_owned())));
            } else {
                out.push((label, Some(read(&path))));
            }
        }
    }
    out.sort();
    out
}

/// alpha のフォルダの中身を作る(最初の確認で 1 回だけ)。
fn make_alpha(alpha: &Path, outside: &Path) {
    for dir in ["src", "docs"] {
        fs::create_dir(alpha.join(dir)).expect("フォルダを作れなかった");
    }
    for (file, text) in [
        ("a.md", "新しい a"),
        ("docs/a.md", "既存の a"),
        ("src/main.rs", "fn main() {}"),
    ] {
        fs::write(alpha.join(file), text).expect("ファイルを作れなかった");
    }
    fs::write(outside.join("secret.txt"), "外の秘密").expect("ファイルを作れなかった");
    fs::create_dir(outside.join("inner")).expect("フォルダを作れなかった");
    // ルートの外を指すリンク(絶対)と、ルートの親を指すリンク(相対)。
    symlink(outside, alpha.join("escape")).expect("リンクを作れなかった");
    symlink("../..", alpha.join("src/up")).expect("リンクを作れなかった");
}

// ---------------------------------------------------------------------------
// 確認
// ---------------------------------------------------------------------------

fn 新規ファイルで空のファイルができ作った相対パスが返る(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let (alpha, _) = ctx.folder("alpha");
    let (outside, _) = ctx.folder("outside");
    make_alpha(&alpha, &outside);
    let id = register(handle, window, ctx, &alpha);
    ctx.rec.clear();

    // AC-T13: `src` で「新規ファイル…」→ `a.ts`。
    let outcome = op(
        handle,
        window,
        ctx,
        "create_entry",
        json!({ "projectId": id, "parent": "src", "name": "a.ts", "kind": "file" }),
    );
    assert_eq!(outcome, done(Some("src/a.ts")));
    let created = alpha.join("src/a.ts");
    assert!(created.is_file(), "ファイルができていない");
    assert_eq!(read(&created), "", "空のファイルでない");

    // フォルダも作れる。ルート直下は親を空の文字列にする。
    let outcome = op(
        handle,
        window,
        ctx,
        "create_entry",
        json!({ "projectId": id, "parent": "", "name": "notes", "kind": "folder" }),
    );
    assert_eq!(outcome, done(Some("notes")));
    assert!(alpha.join("notes").is_dir(), "フォルダができていない");

    // 同じ名前があれば作らずに衝突を返す(上書きしない)。
    fs::write(&created, "書いた").expect("書けなかった");
    let outcome = op(
        handle,
        window,
        ctx,
        "create_entry",
        json!({ "projectId": id, "parent": "src", "name": "a.ts", "kind": "file" }),
    );
    assert_eq!(
        outcome,
        FileOpOutcome::Conflict {
            name: "a.ts".into()
        }
    );
    assert_eq!(read(&created), "書いた", "既存を変えた");

    // 正しくない名前は何も作らない。
    ctx.rec.clear();
    for name in ["", "a/b", "..", "."] {
        let outcome = op(
            handle,
            window,
            ctx,
            "create_entry",
            json!({ "projectId": id, "parent": "src", "name": name, "kind": "file" }),
        );
        assert!(
            matches!(outcome, FileOpOutcome::InvalidName { .. }),
            "{name:?}: {outcome:?}"
        );
    }
    assert!(ctx.rec.ops().is_empty(), "正しくない名前で作ろうとした");

    // 片付け(後の確認のため)。
    fs::remove_file(&created).expect("消せなかった");
    fs::remove_dir(alpha.join("notes")).expect("消せなかった");
}

fn 名称変更でディスクの名前が変わる(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let id = alpha_id(handle, window);
    let alpha = ctx.work.join("alpha");
    ctx.rec.clear();

    let outcome = op(
        handle,
        window,
        ctx,
        "rename_entry",
        json!({ "projectId": id, "path": "src/main.rs", "newName": "lib.rs" }),
    );
    assert_eq!(outcome, done(Some("src/lib.rs")));
    assert!(!alpha.join("src/main.rs").exists(), "元の名前が残った");
    assert_eq!(read(&alpha.join("src/lib.rs")), "fn main() {}");

    // 大文字・小文字だけの変更も衝突にしない(【仮定】15)。
    let outcome = op(
        handle,
        window,
        ctx,
        "rename_entry",
        json!({ "projectId": id, "path": "src/lib.rs", "newName": "Lib.rs" }),
    );
    assert_eq!(outcome, done(Some("src/Lib.rs")));
    let names: Vec<String> = fs::read_dir(alpha.join("src"))
        .expect("読めなかった")
        .map(|e| e.expect("項目").file_name().to_string_lossy().into_owned())
        .collect();
    assert!(names.contains(&"Lib.rs".to_owned()), "{names:?}");

    // 戻す(後の確認のため)。
    let outcome = op(
        handle,
        window,
        ctx,
        "rename_entry",
        json!({ "projectId": id, "path": "src/Lib.rs", "newName": "main.rs" }),
    );
    assert_eq!(outcome, done(Some("src/main.rs")));
    // 変更はコマンドの受け口のスレッド(本物ではメインスレッド)ではなく、待ってよいスレッドで行う。
    let threads = lock(&ctx.rec.threads);
    assert!(!threads.is_empty(), "ファイルシステムを変えていない");
    assert!(
        threads.iter().all(|t| *t != ctx.main_thread),
        "コマンドの受け口のスレッドでファイルを変えた"
    );
}

fn 実行の時点で消えていた対象の名称変更は何も変えず見つからないを返す(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let id = alpha_id(handle, window);
    let alpha = ctx.work.join("alpha");
    // メニューを開いたあとに外から消された(AC-T24)。
    fs::write(alpha.join("gone.txt"), "x").expect("作れなかった");
    fs::remove_file(alpha.join("gone.txt")).expect("消せなかった");
    let before = tree_of(&alpha);
    ctx.rec.clear();

    let outcome = op(
        handle,
        window,
        ctx,
        "rename_entry",
        json!({ "projectId": id, "path": "gone.txt", "newName": "renamed.txt" }),
    );

    assert_eq!(outcome, FileOpOutcome::NotFound);
    assert!(ctx.rec.ops().is_empty(), "見つからないのに変えようとした");
    assert_eq!(tree_of(&alpha), before, "何かが変わった");
    // ほかの操作でも、無い対象は見つからない。
    for (cmd, args) in [
        (
            "trash_entry",
            json!({ "projectId": id, "path": "gone.txt" }),
        ),
        (
            "move_entry",
            json!({ "projectId": id, "source": "gone.txt", "destDir": "docs" }),
        ),
        (
            "copy_entry",
            json!({ "projectId": id, "source": "gone.txt", "destDir": "docs" }),
        ),
        (
            "create_entry",
            json!({ "projectId": id, "parent": "nope", "name": "a.ts", "kind": "file" }),
        ),
    ] {
        assert_eq!(
            op(handle, window, ctx, cmd, args),
            FileOpOutcome::NotFound,
            "{cmd}"
        );
    }
    assert!(ctx.rec.ops().is_empty(), "見つからないのに変えようとした");
    assert_eq!(tree_of(&alpha), before);
}

fn 宛先に同名があると衝突を返し両方の中身が変わらない(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let id = alpha_id(handle, window);
    let alpha = ctx.work.join("alpha");
    let before = tree_of(&alpha);

    // AC-T18: `docs/a.md` があるとき別の `a.md` をドロップする。中止=呼び直さない。
    for cmd in ["move_entry", "copy_entry"] {
        let outcome = op(
            handle,
            window,
            ctx,
            cmd,
            json!({ "projectId": id, "source": "a.md", "destDir": "docs" }),
        );
        assert_eq!(
            outcome,
            FileOpOutcome::Conflict {
                name: "a.md".into()
            },
            "{cmd}"
        );
    }
    // 名称変更で既存の名前にしても同じ。
    let outcome = op(
        handle,
        window,
        ctx,
        "rename_entry",
        json!({ "projectId": id, "path": "docs/a.md", "newName": "a.md" }),
    );
    assert_eq!(
        outcome,
        done(Some("docs/a.md")),
        "同じ名前への変更は何もしない"
    );
    fs::write(alpha.join("docs/b.md"), "b").expect("作れなかった");
    let outcome = op(
        handle,
        window,
        ctx,
        "rename_entry",
        json!({ "projectId": id, "path": "docs/b.md", "newName": "a.md" }),
    );
    assert_eq!(
        outcome,
        FileOpOutcome::Conflict {
            name: "a.md".into()
        }
    );
    fs::remove_file(alpha.join("docs/b.md")).expect("消せなかった");

    assert_eq!(read(&alpha.join("a.md")), "新しい a");
    assert_eq!(read(&alpha.join("docs/a.md")), "既存の a");
    assert_eq!(tree_of(&alpha), before, "衝突なのに何かが変わった");
    assert!(
        lock(&ctx.rec.trashed).is_empty(),
        "衝突なのにゴミ箱に入れた"
    );
}

fn 置き換えるを付けて呼び直すと既存がゴミ箱に渡ってから置かれる(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let id = alpha_id(handle, window);
    let alpha = ctx.work.join("alpha");
    let real = fs::canonicalize(&alpha).expect("正規化できなかった");
    ctx.rec.clear();

    let outcome = op(
        handle,
        window,
        ctx,
        "move_entry",
        json!({ "projectId": id, "source": "a.md", "destDir": "docs", "resolution": "replace" }),
    );

    assert_eq!(outcome, done(Some("docs/a.md")));
    assert!(!alpha.join("a.md").exists(), "元が残った");
    assert_eq!(read(&alpha.join("docs/a.md")), "新しい a");
    // 既存は偽のゴミ箱に入り、そのあとで置かれた。
    let ops = ctx.rec.ops();
    let existing = real.join("docs/a.md");
    let trash_at = ops
        .iter()
        .position(|o| *o == Op::Trash(existing.clone()))
        .unwrap_or_else(|| panic!("既存をゴミ箱に入れていない: {ops:?}"));
    let placed_at = ops
        .iter()
        .rposition(|o| *o == Op::Move(real.join("a.md"), existing.clone()))
        .unwrap_or_else(|| panic!("置いていない: {ops:?}"));
    assert!(trash_at < placed_at, "ゴミ箱に入れる前に置いた: {ops:?}");
    let trashed = lock(&ctx.rec.trashed);
    assert_eq!(trashed.len(), 1, "{trashed:?}");
    assert_eq!(read(&trashed[0].1), "既存の a", "ゴミ箱の中身が既存でない");
    drop(trashed);

    // 後の確認のため、ルート直下に a.md を戻す。
    fs::write(alpha.join("a.md"), "新しい a").expect("作れなかった");
}

fn 両方残すを付けて呼び直すと番号付きの名前で置かれる(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let id = alpha_id(handle, window);
    let alpha = ctx.work.join("alpha");
    ctx.rec.clear();

    let outcome = op(
        handle,
        window,
        ctx,
        "copy_entry",
        json!({ "projectId": id, "source": "a.md", "destDir": "docs", "resolution": "keepBoth" }),
    );

    assert_eq!(outcome, done(Some("docs/a 2.md")));
    assert_eq!(
        read(&alpha.join("a.md")),
        "新しい a",
        "コピーの元が変わった"
    );
    assert_eq!(read(&alpha.join("docs/a.md")), "新しい a", "既存が変わった");
    assert_eq!(read(&alpha.join("docs/a 2.md")), "新しい a");
    assert!(
        lock(&ctx.rec.trashed).is_empty(),
        "両方残すでゴミ箱に入れた"
    );

    // 作成と名称変更でも両方残すを使える。
    let outcome = op(
        handle,
        window,
        ctx,
        "create_entry",
        json!({ "projectId": id, "parent": "docs", "name": "a.md", "kind": "file", "resolution": "keepBoth" }),
    );
    assert_eq!(outcome, done(Some("docs/a 3.md")));
    fs::remove_file(alpha.join("docs/a 2.md")).expect("消せなかった");
    fs::remove_file(alpha.join("docs/a 3.md")).expect("消せなかった");

    // 対処の値でないものは受け取らない。
    op_error(
        handle,
        window,
        ctx,
        "copy_entry",
        json!({ "projectId": id, "source": "a.md", "destDir": "docs", "resolution": "overwrite" }),
    );
    assert!(!alpha.join("docs/a 2.md").exists());
}

fn ゴミ箱はゴミ箱の操作だけを呼ぶ(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let id = alpha_id(handle, window);
    let alpha = ctx.work.join("alpha");
    let real = fs::canonicalize(&alpha).expect("正規化できなかった");
    fs::create_dir(alpha.join("old")).expect("作れなかった");
    fs::write(alpha.join("old/x.txt"), "x").expect("作れなかった");
    ctx.rec.clear();

    // AC-T16(Rust 側)。ファイルとフォルダ。
    for path in ["old/x.txt", "old"] {
        let outcome = op(
            handle,
            window,
            ctx,
            "trash_entry",
            json!({ "projectId": id, "path": path }),
        );
        assert_eq!(outcome, done(None), "{path}");
    }

    assert_eq!(
        ctx.rec.ops(),
        vec![
            Op::Trash(real.join("old/x.txt")),
            Op::Trash(real.join("old"))
        ],
        "ゴミ箱以外の操作を呼んだ"
    );
    assert!(!alpha.join("old").exists(), "消えていない");
    let trashed = lock(&ctx.rec.trashed);
    assert_eq!(trashed.len(), 2);
    assert!(
        trashed.iter().all(|(_, at)| at.starts_with(&ctx.temp_root)),
        "偽のゴミ箱の外へ移した"
    );
}

fn ルート自体とルートの外は拒否し何も変えない(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let id = alpha_id(handle, window);
    let alpha = ctx.work.join("alpha");
    let outside = ctx.work.join("outside");
    let before = (tree_of(&alpha), tree_of(&outside));
    let pasteboard_before = ctx.pasteboard.change_count();
    ctx.rec.clear();

    // FR-035: ルート自体(相対パスが空)は名称変更・移動・切り取り・ゴミ箱の対象にしない。
    let root_itself = FileOpOutcome::NotAllowed {
        reason: NotAllowedReason::RootItself,
    };
    for (cmd, args) in [
        (
            "rename_entry",
            json!({ "projectId": id, "path": "", "newName": "x" }),
        ),
        ("trash_entry", json!({ "projectId": id, "path": "" })),
        (
            "move_entry",
            json!({ "projectId": id, "source": "", "destDir": "docs" }),
        ),
        ("cut_entries", json!({ "projectId": id, "paths": [""] })),
        ("copy_entries", json!({ "projectId": id, "paths": [""] })),
    ] {
        assert_eq!(op(handle, window, ctx, cmd, args), root_itself, "{cmd}");
    }

    // 知らないプロジェクトは失敗にする(何も変えない)。
    let unknown = op_error(
        handle,
        window,
        ctx,
        "trash_entry",
        json!({ "projectId": "00000000-0000-4000-8000-000000000000", "path": "a.md" }),
    );
    assert!(unknown.contains("見つからない"), "{unknown}");

    assert!(
        ctx.rec.ops().is_empty(),
        "拒否したのに変えようとした: {:?}",
        ctx.rec.ops()
    );
    assert_eq!(
        ctx.pasteboard.change_count(),
        pasteboard_before,
        "拒否したのにペーストボードに書いた"
    );
    assert_eq!((tree_of(&alpha), tree_of(&outside)), before);
}

fn ドットドットと絶対パスとシンボリックリンクでの脱出を拒否する(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let id = alpha_id(handle, window);
    let alpha = ctx.work.join("alpha");
    let outside = ctx.work.join("outside");
    let absolute = outside.join("secret.txt").display().to_string();
    // ペーストの宛先の検査まで進むよう、ペーストボードにファイルを置いておく。
    let source = alpha.join("a.md");
    ctx.pasteboard
        .write_file_urls(&[source.as_path()])
        .expect("ペーストボードに書けなかった");
    let before = (tree_of(&alpha), tree_of(&outside));
    ctx.rec.clear();

    // 相対パスにできないものは、コマンドに届く前に拒否する。
    for path in [
        "..",
        "../outside",
        "src/../..",
        ".",
        "./a.md",
        "/etc",
        absolute.as_str(),
        "src//main.rs",
        "src/",
        "a\0b",
    ] {
        for (cmd, args) in [
            ("trash_entry", json!({ "projectId": id, "path": path })),
            (
                "rename_entry",
                json!({ "projectId": id, "path": path, "newName": "x" }),
            ),
            (
                "move_entry",
                json!({ "projectId": id, "source": "a.md", "destDir": path }),
            ),
            (
                "copy_entry",
                json!({ "projectId": id, "source": path, "destDir": "docs" }),
            ),
            (
                "create_entry",
                json!({ "projectId": id, "parent": path, "name": "x", "kind": "file" }),
            ),
            ("copy_entries", json!({ "projectId": id, "paths": [path] })),
            (
                "cut_entries",
                json!({ "projectId": id, "paths": ["a.md", path] }),
            ),
            ("paste_entries", json!({ "projectId": id, "destDir": path })),
        ] {
            // 拒否の文言は受け取った値をそのまま引用する。画面が送った絶対パスを引用するのは漏れではないので、
            // その値のときだけ絶対パスの検査を外す。
            let error = if path == absolute {
                error_text(call(handle, window, cmd, args))
            } else {
                op_error(handle, window, ctx, cmd, args)
            };
            assert!(
                error.contains("相対パスにできない"),
                "{cmd} {path:?}: {error}"
            );
        }
    }
    // 名前に `/` や `..` を入れても外へは出られない。
    let outcome = op(
        handle,
        window,
        ctx,
        "rename_entry",
        json!({ "projectId": id, "path": "a.md", "newName": "../a.md" }),
    );
    assert!(
        matches!(outcome, FileOpOutcome::InvalidName { .. }),
        "{outcome:?}"
    );

    // シンボリックリンクでルートの外へ出る経路は拒否する。
    let outside_root = FileOpOutcome::NotAllowed {
        reason: NotAllowedReason::OutsideRoot,
    };
    for (cmd, args) in [
        (
            "trash_entry",
            json!({ "projectId": id, "path": "escape/secret.txt" }),
        ),
        (
            "rename_entry",
            json!({ "projectId": id, "path": "escape/secret.txt", "newName": "x" }),
        ),
        (
            "create_entry",
            json!({ "projectId": id, "parent": "escape", "name": "x", "kind": "file" }),
        ),
        (
            "create_entry",
            json!({ "projectId": id, "parent": "src/up", "name": "x", "kind": "folder" }),
        ),
        (
            "move_entry",
            json!({ "projectId": id, "source": "a.md", "destDir": "escape/inner" }),
        ),
        (
            "copy_entry",
            json!({ "projectId": id, "source": "escape/secret.txt", "destDir": "docs" }),
        ),
        (
            "move_entry",
            json!({ "projectId": id, "source": "src/up/outside", "destDir": "docs" }),
        ),
        (
            "copy_entries",
            json!({ "projectId": id, "paths": ["escape/secret.txt"] }),
        ),
        (
            "paste_entries",
            json!({ "projectId": id, "destDir": "escape" }),
        ),
    ] {
        assert_eq!(
            op(handle, window, ctx, cmd, args.clone()),
            outside_root,
            "{cmd} {args}"
        );
    }

    assert!(
        ctx.rec.ops().is_empty(),
        "拒否したのに変えようとした: {:?}",
        ctx.rec.ops()
    );
    assert_eq!(
        (tree_of(&alpha), tree_of(&outside)),
        before,
        "何かが変わった"
    );
}

fn ツリーでコピーするとペーストボードにファイルの_url_が載る(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let id = alpha_id(handle, window);
    let alpha = ctx.work.join("alpha");
    let real = fs::canonicalize(&alpha).expect("正規化できなかった");
    let before = tree_of(&alpha);
    ctx.rec.clear();

    let outcome = op(
        handle,
        window,
        ctx,
        "copy_entries",
        json!({ "projectId": id, "paths": ["a.md", "src"] }),
    );

    assert_eq!(outcome, done(None));
    let urls = ctx
        .pasteboard
        .read_file_urls()
        .expect("ペーストボードを読めなかった");
    assert_eq!(urls, vec![real.join("a.md"), real.join("src")]);
    assert!(ctx.rec.ops().is_empty(), "コピーでファイルを変えた");
    assert_eq!(tree_of(&alpha), before);

    // 空の一覧は何もしない(ペーストボードを消さない)。
    let count = ctx.pasteboard.change_count();
    let outcome = op(
        handle,
        window,
        ctx,
        "copy_entries",
        json!({ "projectId": id, "paths": [] }),
    );
    assert_eq!(outcome, done(None));
    assert_eq!(
        ctx.pasteboard.change_count(),
        count,
        "空の一覧でペーストボードを変えた"
    );
}

fn ペーストボードのファイルをフォルダへペーストできる(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let id = alpha_id(handle, window);
    let alpha = ctx.work.join("alpha");
    let (finder, _) = ctx.folder("finder");
    fs::write(finder.join("from-finder.txt"), "Finder から").expect("作れなかった");
    // Finder でコピーしたことにする(ルートの外のファイル)。
    let source = finder.join("from-finder.txt");
    ctx.pasteboard
        .write_file_urls(&[source.as_path()])
        .expect("ペーストボードに書けなかった");
    ctx.rec.clear();

    // フォルダの上でペーストするとその中に置かれる。
    let outcome = op(
        handle,
        window,
        ctx,
        "paste_entries",
        json!({ "projectId": id, "destDir": "docs" }),
    );
    assert_eq!(outcome, done(Some("docs/from-finder.txt")));
    assert_eq!(read(&alpha.join("docs/from-finder.txt")), "Finder から");
    assert_eq!(read(&source), "Finder から", "コピーの元が変わった");
    assert!(
        ctx.rec.ops().iter().all(|o| matches!(o, Op::Copy(..))),
        "コピー以外の操作を呼んだ: {:?}",
        ctx.rec.ops()
    );

    // もう一度ペーストすると衝突になり、対処を付けて呼び直せる。
    let outcome = op(
        handle,
        window,
        ctx,
        "paste_entries",
        json!({ "projectId": id, "destDir": "docs" }),
    );
    assert_eq!(
        outcome,
        FileOpOutcome::Conflict {
            name: "from-finder.txt".into()
        }
    );
    let outcome = op(
        handle,
        window,
        ctx,
        "paste_entries",
        json!({ "projectId": id, "destDir": "docs", "resolution": "keepBoth" }),
    );
    assert_eq!(outcome, done(Some("docs/from-finder 2.txt")));

    // ツリーでコピーしたものを、同じルートの別のフォルダへペーストする。
    let outcome = op(
        handle,
        window,
        ctx,
        "copy_entries",
        json!({ "projectId": id, "paths": ["src/main.rs"] }),
    );
    assert_eq!(outcome, done(None));
    let outcome = op(
        handle,
        window,
        ctx,
        "paste_entries",
        json!({ "projectId": id, "destDir": "" }),
    );
    assert_eq!(outcome, done(Some("main.rs")));
    assert_eq!(read(&alpha.join("main.rs")), "fn main() {}");
    assert!(alpha.join("src/main.rs").exists(), "コピーの元が消えた");

    // ペーストボードにファイルが無ければ何もしない(【仮定】8)。
    MacPasteboard::named(pasteboard_name())
        .write_text("文字列だけ")
        .expect("書けなかった");
    ctx.rec.clear();
    let outcome = op(
        handle,
        window,
        ctx,
        "paste_entries",
        json!({ "projectId": id, "destDir": "docs" }),
    );
    assert_eq!(outcome, done(None));
    assert!(ctx.rec.ops().is_empty(), "ファイルが無いのに変えた");

    for file in ["docs/from-finder.txt", "docs/from-finder 2.txt", "main.rs"] {
        fs::remove_file(alpha.join(file)).expect("消せなかった");
    }
}

fn 切り取ってペーストすると元の場所から無くなる(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let id = alpha_id(handle, window);
    let alpha = ctx.work.join("alpha");
    let real = fs::canonicalize(&alpha).expect("正規化できなかった");
    fs::write(alpha.join("cut.txt"), "切り取り").expect("作れなかった");
    ctx.rec.clear();

    let outcome = op(
        handle,
        window,
        ctx,
        "cut_entries",
        json!({ "projectId": id, "paths": ["cut.txt"] }),
    );
    assert_eq!(outcome, done(None));
    assert!(alpha.join("cut.txt").exists(), "切り取りだけで動かした");
    assert_eq!(
        ctx.pasteboard.read_file_urls().expect("読めなかった"),
        vec![real.join("cut.txt")],
        "ペーストボードにファイルの URL が無い"
    );

    let outcome = op(
        handle,
        window,
        ctx,
        "paste_entries",
        json!({ "projectId": id, "destDir": "docs" }),
    );
    assert_eq!(outcome, done(Some("docs/cut.txt")));
    assert!(!alpha.join("cut.txt").exists(), "元の場所に残った");
    assert_eq!(read(&alpha.join("docs/cut.txt")), "切り取り");
    assert_eq!(
        ctx.rec.ops(),
        vec![Op::Move(real.join("cut.txt"), real.join("docs/cut.txt"))],
        "移動 1 回で行っていない"
    );

    // 別のプロジェクトのルートへも、切り取って移せる(【仮定】13)。
    let (beta, beta_real) = ctx.folder("beta");
    let beta_id = register(handle, window, ctx, &beta);
    ctx.rec.clear();
    let outcome = op(
        handle,
        window,
        ctx,
        "cut_entries",
        json!({ "projectId": id, "paths": ["docs/cut.txt"] }),
    );
    assert_eq!(outcome, done(None));
    let outcome = op(
        handle,
        window,
        ctx,
        "paste_entries",
        json!({ "projectId": beta_id, "destDir": "" }),
    );
    assert_eq!(outcome, done(Some("cut.txt")));
    assert!(!alpha.join("docs/cut.txt").exists());
    assert_eq!(read(&beta.join("cut.txt")), "切り取り");
    assert_eq!(
        ctx.rec.ops(),
        vec![Op::Move(
            real.join("docs/cut.txt"),
            beta_real.join("cut.txt")
        )]
    );

    // 移し終えたら切り取りは終わり、次のペーストはペーストボードの中身のコピーになる。
    let outcome = op(
        handle,
        window,
        ctx,
        "paste_entries",
        json!({ "projectId": beta_id, "destDir": "", "resolution": "keepBoth" }),
    );
    assert_eq!(outcome, FileOpOutcome::NotFound, "移したあとの元を探した");
    fs::remove_file(beta.join("cut.txt")).expect("消せなかった");
}

fn どの操作の前後でも一時フォルダの外が変わらない(
    _handle: &AppHandle<MockRuntime>,
    _window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    // ここまでの確認で呼ばれた変更の口は、どれも一時フォルダの中だけを指していた。
    let temp_real = fs::canonicalize(&ctx.temp_root).expect("正規化できなかった");
    let all = lock(&ctx.rec.all_ops);
    assert!(!all.is_empty(), "変更の口が 1 回も呼ばれていない");
    for op in all.iter() {
        for path in op.paths() {
            assert!(
                path.starts_with(&temp_real) || path.starts_with(&ctx.temp_root),
                "一時フォルダの外を変えようとした: {op:?}"
            );
        }
    }
    // ルートの外に置いたフォルダ(リンクの指す先)は作ったときのまま。
    assert_eq!(
        tree_of(&ctx.work.join("outside")),
        vec![
            ("inner".to_owned(), None),
            ("secret.txt".to_owned(), Some("外の秘密".to_owned())),
        ]
    );
}

/// テスト用の名前付きのペーストボードの名前(文字列もファイルの URL も同じもの)。
fn pasteboard_name() -> String {
    format!("dev.arin.test.fileops-commands.{}", std::process::id())
}

const CHECKS: &[(&str, Check)] = &[
    (
        "新規ファイルで空のファイルができ作った相対パスが返る",
        新規ファイルで空のファイルができ作った相対パスが返る,
    ),
    (
        "名称変更でディスクの名前が変わる",
        名称変更でディスクの名前が変わる,
    ),
    (
        "実行の時点で消えていた対象の名称変更は何も変えず見つからないを返す",
        実行の時点で消えていた対象の名称変更は何も変えず見つからないを返す,
    ),
    (
        "宛先に同名があると衝突を返し両方の中身が変わらない",
        宛先に同名があると衝突を返し両方の中身が変わらない,
    ),
    (
        "置き換えるを付けて呼び直すと既存がゴミ箱に渡ってから置かれる",
        置き換えるを付けて呼び直すと既存がゴミ箱に渡ってから置かれる,
    ),
    (
        "両方残すを付けて呼び直すと番号付きの名前で置かれる",
        両方残すを付けて呼び直すと番号付きの名前で置かれる,
    ),
    (
        "ゴミ箱はゴミ箱の操作だけを呼ぶ",
        ゴミ箱はゴミ箱の操作だけを呼ぶ,
    ),
    (
        "ルート自体とルートの外は拒否し何も変えない",
        ルート自体とルートの外は拒否し何も変えない,
    ),
    (
        "ドットドットと絶対パスとシンボリックリンクでの脱出を拒否する",
        ドットドットと絶対パスとシンボリックリンクでの脱出を拒否する,
    ),
    (
        "ツリーでコピーするとペーストボードにファイルの_url_が載る",
        ツリーでコピーするとペーストボードにファイルの_url_が載る,
    ),
    (
        "ペーストボードのファイルをフォルダへペーストできる",
        ペーストボードのファイルをフォルダへペーストできる,
    ),
    (
        "切り取ってペーストすると元の場所から無くなる",
        切り取ってペーストすると元の場所から無くなる,
    ),
    (
        "どの操作の前後でも一時フォルダの外が変わらない",
        どの操作の前後でも一時フォルダの外が変わらない,
    ),
];

fn main() {
    let temp = support::TempDir::new("fileops-commands");
    let temp_root = temp.path().to_path_buf();
    let state_dir = temp_root.join("state");
    let work = temp_root.join("work");
    let trash_dir = temp_root.join("fake-trash");
    for dir in [&state_dir, &work, &trash_dir] {
        fs::create_dir(dir).expect("一時フォルダの中にフォルダを作れなかった");
    }

    // ペーストボードは名前付きのもの(文字列もファイルの URL も同じ名前)。利用者のクリップボードを渡さない。
    let text_pasteboard = MacPasteboard::named(pasteboard_name());
    let file_pasteboard = MacFilePasteboard::named(pasteboard_name());
    assert!(
        !text_pasteboard.is_general() && !file_pasteboard.is_general(),
        "ユーザーのクリップボードを使おうとした"
    );

    let picker = Arc::new(FakePicker::default());
    let rec = Arc::new(Recorder::default());
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
    let app = arin_lib::builder_with_tree_ports::<MockRuntime>(
        launch,
        Arc::new(NoPty),
        Arc::new(|| ShellDefaults {
            shell: PathBuf::from("/bin/zsh"),
            locale: None,
        }),
        PersistencePorts::native(state_dir.clone()),
        project_ports,
        TreePorts {
            fs: Arc::new(RecordingFs {
                inner: LocalFileSystem::new(),
                rec: Arc::clone(&rec),
                trash_dir,
            }),
            watcher: Arc::new(FsEventsWatcher::new()),
            file_pasteboard: Arc::new(file_pasteboard.clone()),
        },
    )
    .build(tauri::generate_context!())
    .unwrap_or_else(|e| panic!("組み立てに失敗した: {e}"));

    let ctx = Ctx {
        picker,
        rec,
        pasteboard: file_pasteboard.clone(),
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
    file_pasteboard.release();
    assert_eq!(passed.get(), CHECKS.len(), "起動の完了まで進まなかった");
    println!("test result: ok. {} passed", CHECKS.len());
}
