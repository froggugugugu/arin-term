//! ツリーのコマンド(`src-tauri/src/commands/tree.rs`、計画書 T42a)を、Tauri の模擬ランタイムで画面から呼ぶ形で確かめる。
//! FR-013(フォルダの中身)、FR-017(隠しファイル表示)、設計書 §7.2・§12(ルートの外に出ない、絶対パスを返さない)。
//!
//! - ファイルシステムは本物(`LocalFileSystem`)を記録の包みで囲んだもの。読んだ場所とスレッドを記録する
//! - プロジェクトのフォルダと状態ファイルは、すべて一時フォルダの中に作る
//! - フォルダ選択のダイアログは偽物(決めた答えを順に返す)、PTY は偽物(起動しない)
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
use std::time::{Duration, Instant};

use arin_core::ports::fs::{DirEntry, EntryKind, FileSystemPort, FsError};
use arin_core::ports::{PtyError, PtyPort, PtySize, SpawnedPty};
use arin_core::terminal::ShellLaunchSpec;
use arin_core::tree::DirListing;
use arin_core::workspace::{AddProjectOutcome, AppSnapshot};
use arin_lib::persistence::PersistencePorts;
use arin_lib::projects::{FolderPicker, FolderPurpose, ProjectPorts};
use arin_lib::sessions::ShellDefaults;
use arin_platform::desktop::MacPasteboard;
use arin_platform::fs::{LocalFileSystem, STAGING_PREFIX};
use arin_platform::system::UrandomIdSource;
use serde_json::json;
use support::{request, request_with};
use tauri::ipc::{InvokeBody, InvokeResponseBody};
use tauri::test::{MockRuntime, get_ipc_response};
use tauri::{AppHandle, Manager, RunEvent, WebviewWindow};

const WAIT: Duration = Duration::from_secs(10);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn wait_until(mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        thread::sleep(Duration::from_millis(10));
    }
    condition()
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
// 記録するファイルシステム
// ---------------------------------------------------------------------------

/// 本物のファイルシステムに読んだ場所とスレッドを記録する包み。読む口(`read_dir`・`real_dir`・`entry_kind`)だけを記録する。
#[derive(Default)]
struct Recorder {
    /// 読んだ・解決した絶対パス(呼ばれた順)。
    touched: Mutex<Vec<PathBuf>>,
    /// `read_dir` の中身を返した実パス。
    listed: Mutex<Vec<PathBuf>>,
    /// 読む口が呼ばれたスレッド。
    threads: Mutex<Vec<ThreadId>>,
}

impl Recorder {
    fn clear(&self) {
        lock(&self.touched).clear();
        lock(&self.listed).clear();
        lock(&self.threads).clear();
    }

    fn note(&self, path: &Path) {
        lock(&self.touched).push(path.to_path_buf());
        lock(&self.threads).push(thread::current().id());
    }
}

struct RecordingFs {
    inner: LocalFileSystem,
    rec: Arc<Recorder>,
}

impl FileSystemPort for RecordingFs {
    fn read_dir(&self, dir: &Path) -> Result<Vec<DirEntry>, FsError> {
        self.rec.note(dir);
        let entries = self.inner.read_dir(dir)?;
        lock(&self.rec.listed).push(dir.to_path_buf());
        Ok(entries)
    }

    fn entry_kind(&self, path: &Path) -> Result<EntryKind, FsError> {
        self.rec.note(path);
        self.inner.entry_kind(path)
    }

    fn real_dir(&self, dir: &Path) -> Result<PathBuf, FsError> {
        self.rec.note(dir);
        self.inner.real_dir(dir)
    }

    fn create_file(&self, _path: &Path) -> Result<(), FsError> {
        panic!("ツリーの読み込みでファイルを作ろうとした")
    }

    fn create_dir(&self, _path: &Path) -> Result<(), FsError> {
        panic!("ツリーの読み込みでフォルダを作ろうとした")
    }

    fn move_entry(&self, _from: &Path, _to: &Path) -> Result<(), FsError> {
        panic!("ツリーの読み込みで移動しようとした")
    }

    fn copy_entry(&self, _from: &Path, _to: &Path) -> Result<(), FsError> {
        panic!("ツリーの読み込みでコピーしようとした")
    }

    fn trash(&self, _path: &Path) -> Result<Option<PathBuf>, FsError> {
        panic!("ツリーの読み込みでゴミ箱に入れようとした")
    }
}

// ---------------------------------------------------------------------------
// 共有するもの
// ---------------------------------------------------------------------------

struct Ctx {
    picker: Arc<FakePicker>,
    rec: Arc<Recorder>,
    /// プロジェクトのフォルダを置く一時フォルダ(状態フォルダとは別)。
    work: PathBuf,
    state_dir: PathBuf,
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

    /// 応答の文字列に、一時フォルダの絶対パス(元の形と正規化した形)が入っていないことを確かめる。
    fn assert_no_absolute_path(&self, text: &str) {
        let canonical = fs::canonicalize(&self.work).expect("正規化できなかった");
        for path in [&self.work, &canonical] {
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

/// `list_directory` を呼び、応答を読む。応答の文字列に絶対パスが無いことも確かめる。
fn list(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
    project_id: &str,
    path: &str,
) -> DirListing {
    let listing: DirListing = call(
        handle,
        window,
        "list_directory",
        json!({ "projectId": project_id, "path": path }),
    )
    .unwrap_or_else(|e| panic!("list_directory({path:?}) が失敗した: {e}"))
    .deserialize()
    .unwrap_or_else(|e| panic!("DirListing として読めなかった: {e}"));
    ctx.assert_no_absolute_path(&serde_json::to_string(&listing).expect("JSON にできなかった"));
    listing
}

/// 中身の名前と種類の並び。読めていなければ失敗。
fn entries(listing: DirListing) -> Vec<(String, EntryKind)> {
    match listing {
        DirListing::Listed { entries } => entries.into_iter().map(|e| (e.name, e.kind)).collect(),
        other => panic!("読めなかった: {other:?}"),
    }
}

fn names(listing: DirListing) -> Vec<String> {
    entries(listing).into_iter().map(|(name, _)| name).collect()
}

/// alpha のフォルダの中身を作る。
fn make_alpha(alpha: &Path) {
    for dir in [".git", "node_modules", ".claude", "src", "Zeta"] {
        fs::create_dir(alpha.join(dir)).expect("フォルダを作れなかった");
    }
    // コピーの作業用フォルダ(作りかけ)。どの設定でも出さない。
    fs::create_dir(alpha.join(format!("{STAGING_PREFIX}123-0"))).expect("フォルダを作れなかった");
    for file in [".DS_Store", "B.md", "a.txt", "src/main.rs"] {
        fs::write(alpha.join(file), "x").expect("ファイルを作れなかった");
    }
    symlink("src", alpha.join("inside-link")).expect("リンクを作れなかった");
}

// ---------------------------------------------------------------------------
// 確認
// ---------------------------------------------------------------------------

fn ルートの中身が除外と並び順のとおりに返る(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let (alpha, canonical) = ctx.folder("alpha");
    make_alpha(&alpha);
    let id = register(handle, window, ctx, &alpha);
    ctx.rec.clear();

    let root = entries(list(handle, window, ctx, &id, ""));

    assert_eq!(
        root,
        vec![
            (".claude".to_owned(), EntryKind::Directory),
            ("inside-link".to_owned(), EntryKind::DirectoryLink),
            ("src".to_owned(), EntryKind::Directory),
            ("Zeta".to_owned(), EntryKind::Directory),
            ("a.txt".to_owned(), EntryKind::File),
            ("B.md".to_owned(), EntryKind::File),
        ],
        "除外か並び順が違う"
    );
    assert_eq!(names(list(handle, window, ctx, &id, "src")), ["main.rs"]);
    assert_eq!(
        lock(&ctx.rec.listed).as_slice(),
        &[canonical.clone(), canonical.join("src")],
        "検査した実パスとは別の場所を読んだ"
    );
    // 読み込みは模擬のアプリのスレッド(本物ではメインスレッド)ではなく、待ってよいスレッドで行う。
    let threads = lock(&ctx.rec.threads);
    assert!(!threads.is_empty(), "ファイルシステムを読んでいない");
    assert!(
        threads.iter().all(|t| *t != ctx.main_thread),
        "コマンドの受け口のスレッドでフォルダを読んだ"
    );
}

fn 隠しファイル表示をオンにすると_3_つも返り写しと状態ファイルに反映される(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let before = snapshot(handle, window);
    let id = project_id_by_name(&before, "alpha");
    assert!(
        before.projects.iter().all(|p| !p.show_hidden),
        "既定で隠しファイル表示が ON"
    );

    call(
        handle,
        window,
        "set_show_hidden",
        json!({ "projectId": id, "showHidden": true }),
    )
    .unwrap_or_else(|e| panic!("set_show_hidden が失敗した: {e}"));

    let s = snapshot(handle, window);
    let card = s
        .projects
        .iter()
        .find(|p| p.id.as_str() == id)
        .expect("プロジェクトが消えた");
    assert!(card.show_hidden, "写しに反映されていない");
    assert_eq!(
        names(list(handle, window, ctx, &id, "")),
        [
            ".claude",
            ".git",
            "inside-link",
            "node_modules",
            "src",
            "Zeta",
            ".DS_Store",
            "a.txt",
            "B.md",
        ],
        "3 つが返らないか、コピーの作業用フォルダが出た"
    );

    // 前面でなくなったときは待たずに書く(保存は写しを送ったときに予約されている)。
    arin_lib::lifecycle::on_window_focus_changed(handle, false);
    let state = ctx.state_dir.join("state.json");
    // 既定値(偽)のときは `tree` を書かないので、無ければ偽とみなす。
    let saved_show_hidden = || -> Option<bool> {
        let text = fs::read_to_string(&state).ok()?;
        let saved: serde_json::Value = serde_json::from_str(&text).ok()?;
        let project = saved["projects"]
            .as_array()?
            .iter()
            .find(|p| p["id"] == json!(id))?;
        Some(project["tree"]["showHidden"].as_bool().unwrap_or(false))
    };
    assert!(
        wait_until(|| saved_show_hidden() == Some(true)),
        "状態ファイルに保存されなかった: {:?}",
        fs::read_to_string(&state)
    );

    // 戻すと 3 つは隠れ、写しも戻る。
    call(
        handle,
        window,
        "set_show_hidden",
        json!({ "projectId": id, "showHidden": false }),
    )
    .unwrap_or_else(|e| panic!("set_show_hidden が失敗した: {e}"));
    assert_eq!(snapshot(handle, window), before, "元に戻らなかった");
    assert!(!names(list(handle, window, ctx, &id, "")).contains(&".git".to_owned()));
    arin_lib::lifecycle::on_window_focus_changed(handle, false);
    assert!(
        wait_until(|| saved_show_hidden() == Some(false)),
        "戻した値が状態ファイルに保存されなかった"
    );

    // 知らない ID は何も変えずに失敗する。
    let unknown = error_text(call(
        handle,
        window,
        "set_show_hidden",
        json!({ "projectId": "00000000-0000-4000-8000-000000000000", "showHidden": true }),
    ));
    assert!(unknown.contains("見つからない"), "{unknown}");
    assert_eq!(snapshot(handle, window), before);
}

fn ドットドットや絶対パスを渡すと拒否し何も読まない(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let id = project_id_by_name(&snapshot(handle, window), "alpha");
    let absolute = ctx.work.join("alpha").display().to_string();
    ctx.rec.clear();

    for path in [
        "..",
        "../beta",
        "src/../..",
        ".",
        "./src",
        "/etc",
        absolute.as_str(),
        "src//main.rs",
        "src/",
        "src\0x",
    ] {
        let error = error_text(call(
            handle,
            window,
            "list_directory",
            json!({ "projectId": id, "path": path }),
        ));
        assert!(error.contains("相対パスにできない"), "{path:?}: {error}");
    }
    // 相対パスの型でない値も拒否する。
    for path in [json!(null), json!(1), json!(["src"])] {
        call(
            handle,
            window,
            "list_directory",
            json!({ "projectId": id, "path": path }),
        )
        .expect_err("相対パスでない値を受け取った");
    }
    assert!(
        lock(&ctx.rec.touched).is_empty(),
        "拒否したのにファイルシステムを読んだ: {:?}",
        lock(&ctx.rec.touched)
    );
}

fn シンボリックリンクでルートの外を指すフォルダは中身を返さない(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let (alpha, _) = ctx.folder("alpha");
    let (outside, outside_real) = ctx.folder("outside");
    fs::create_dir(outside.join("inner")).expect("フォルダを作れなかった");
    fs::write(outside.join("secret.txt"), "secret").expect("ファイルを作れなかった");
    symlink(&outside, alpha.join("escape")).expect("リンクを作れなかった");
    // ルートの親の階層を指す相対のリンクでも同じ(`src/up` → `alpha/..`)。
    symlink("../..", alpha.join("src/up")).expect("リンクを作れなかった");
    let id = project_id_by_name(&snapshot(handle, window), "alpha");
    ctx.rec.clear();

    for path in ["escape", "escape/inner", "src/up", "src/up/outside"] {
        assert_eq!(
            list(handle, window, ctx, &id, path),
            DirListing::OutsideRoot,
            "{path} の中身を返した"
        );
    }
    let root_real = fs::canonicalize(&alpha).expect("正規化できなかった");
    let listed = lock(&ctx.rec.listed);
    assert!(
        listed.is_empty(),
        "ルートの外を指すフォルダを読んだ: {listed:?}"
    );
    drop(listed);
    assert!(
        lock(&ctx.rec.touched)
            .iter()
            .all(|p| !p.starts_with(&outside_real)),
        "ルートの外の実パスを調べた"
    );

    // リンク自体はルートの一覧に「フォルダへのリンク」として出る(展開はしない)。中身は読んでいない。
    let root = entries(list(handle, window, ctx, &id, ""));
    assert!(root.contains(&("escape".to_owned(), EntryKind::DirectoryLink)));
    assert_eq!(lock(&ctx.rec.listed).as_slice(), &[root_real]);
}

fn 見つからないプロジェクトは見つからないを返す(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    // 知らない ID(形は正しい)と、ID の形でない値は失敗にする。
    let unknown = error_text(call(
        handle,
        window,
        "list_directory",
        json!({ "projectId": "00000000-0000-4000-8000-000000000000", "path": "" }),
    ));
    assert!(unknown.contains("見つからない"), "{unknown}");
    call(
        handle,
        window,
        "list_directory",
        json!({ "projectId": "../alpha", "path": "" }),
    )
    .expect_err("ID の形でない値を受け取った");

    let (beta, _) = ctx.folder("beta");
    fs::create_dir(beta.join("docs")).expect("フォルダを作れなかった");
    let id = register(handle, window, ctx, &beta);
    assert_eq!(names(list(handle, window, ctx, &id, "")), ["docs"]);

    // フォルダが無くなると、見回りが「見つからない」にする前でも読めず「見つからない」。
    fs::remove_dir_all(&beta).expect("一時フォルダの中のフォルダを消せなかった");
    assert_eq!(list(handle, window, ctx, &id, ""), DirListing::NotFound);
    assert_eq!(list(handle, window, ctx, &id, "docs"), DirListing::NotFound);

    // 見回りが「見つからない」にしたあとは、最後のパスを読まずに「見つからない」。
    assert!(
        wait_until(|| snapshot(handle, window)
            .projects
            .iter()
            .any(|p| p.id.as_str() == id && p.missing)),
        "見回りが見つからないにしなかった"
    );
    ctx.rec.clear();
    assert_eq!(list(handle, window, ctx, &id, ""), DirListing::NotFound);
    assert!(
        lock(&ctx.rec.touched).is_empty(),
        "見つからないプロジェクトのパスを読んだ"
    );
}

fn ファイルを指すとフォルダでないを返す(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let id = project_id_by_name(&snapshot(handle, window), "alpha");

    assert_eq!(
        list(handle, window, ctx, &id, "a.txt"),
        DirListing::NotADirectory
    );
    assert_eq!(
        list(handle, window, ctx, &id, "src/main.rs"),
        DirListing::NotADirectory
    );
    // 無いフォルダは「見つからない」。
    assert_eq!(list(handle, window, ctx, &id, "nope"), DirListing::NotFound);
    assert_eq!(
        list(handle, window, ctx, &id, "src/nope"),
        DirListing::NotFound
    );
}

const CHECKS: &[(&str, Check)] = &[
    (
        "ルートの中身が除外と並び順のとおりに返る",
        ルートの中身が除外と並び順のとおりに返る,
    ),
    (
        "隠しファイル表示をオンにすると_3_つも返り写しと状態ファイルに反映される",
        隠しファイル表示をオンにすると_3_つも返り写しと状態ファイルに反映される,
    ),
    (
        "ドットドットや絶対パスを渡すと拒否し何も読まない",
        ドットドットや絶対パスを渡すと拒否し何も読まない,
    ),
    (
        "シンボリックリンクでルートの外を指すフォルダは中身を返さない",
        シンボリックリンクでルートの外を指すフォルダは中身を返さない,
    ),
    (
        "見つからないプロジェクトは見つからないを返す",
        見つからないプロジェクトは見つからないを返す,
    ),
    (
        "ファイルを指すとフォルダでないを返す",
        ファイルを指すとフォルダでないを返す,
    ),
];

fn main() {
    let temp = support::TempDir::new("tree-commands");
    let state_dir = temp.path().join("state");
    let work = temp.path().join("work");
    fs::create_dir(&state_dir).expect("状態フォルダを作れなかった");
    fs::create_dir(&work).expect("作業フォルダを作れなかった");

    // ペーストボードは使わないが、ユーザーのクリップボードを渡さない。
    let pasteboard = MacPasteboard::named(format!(
        "dev.arin.test.tree-commands.{}",
        std::process::id()
    ));
    assert!(
        !pasteboard.is_general(),
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
        pasteboard: Arc::new(pasteboard.clone()),
        picker: Some(Arc::clone(&picker) as Arc<dyn FolderPicker>),
    };
    let app = arin_lib::builder_with_file_system::<MockRuntime>(
        launch,
        Arc::new(NoPty),
        Arc::new(|| ShellDefaults {
            shell: PathBuf::from("/bin/zsh"),
            locale: None,
        }),
        PersistencePorts::native(state_dir.clone()),
        project_ports,
        Arc::new(RecordingFs {
            inner: LocalFileSystem::new(),
            rec: Arc::clone(&rec),
        }),
    )
    .build(tauri::generate_context!())
    .unwrap_or_else(|e| panic!("組み立てに失敗した: {e}"));

    let ctx = Ctx {
        picker,
        rec,
        work,
        state_dir,
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
    pasteboard.release();
    assert_eq!(passed.get(), CHECKS.len(), "起動の完了まで進まなかった");
    println!("test result: ok. {} passed", CHECKS.len());
}
