//! ツリーの監視の張り替えと `tree-changed`(`src-tauri/src/tree_watch.rs`、計画書 T42b)。
//! FR-014(外からの変更の反映)、FR-002 のツリーの部分(切替)、FR-007 の「ツリーも表示され続ける」(ルートの追従)、
//! 設計書 §1.3 #7(選択中のプロジェクトだけを監視する)、§12(イベントに絶対パスを載せない)。
//!
//! - 監視のポートは偽物(`FakeWatcher`。張った場所と、捨てられたかを記録し、受け口へ束を流せる)。
//!   実物(`FsEventsWatcher`)は一時フォルダで 1 件だけ
//! - プロジェクトのフォルダは一時フォルダの中に作る(張る前に実パスにそろえるのを、本物のファイルシステムで確かめる)
//! - 最後に Tauri の模擬ランタイムで、画面へ `tree-changed` が届くこと、終了で監視が捨てられることを確かめる
//!   (M1 の 9 枚の検証画面で監視を張らないことの確かめは、検証画面と一緒に M4 の T49d で外した)
//!
//! macOS ではメニューの部品をメインスレッドでしか作れないため、`harness = false` で走らせる(`app_builder` と同じ)。

mod support;

use std::collections::{HashSet, VecDeque};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use arin_core::paths::RelPath;
use arin_core::persistence::codec::PersistedState;
use arin_core::ports::watch::{ActiveWatch, WatchBatch, WatchError, WatchSink, WatcherPort};
use arin_core::ports::{PtyError, PtyPort, PtySize, SpawnedPty};
use arin_core::project::{Project, ProjectColor, ProjectId, RootReference};
use arin_core::terminal::ShellLaunchSpec;
use arin_core::tree::TreeChange;
use arin_core::workspace::{AddProjectOutcome, RootResolution, Startup, Workspace};
use arin_lib::events::TREE_CHANGED_EVENT;
use arin_lib::persistence::PersistencePorts;
use arin_lib::projects::{FolderPicker, FolderPurpose, ProjectPorts};
use arin_lib::sessions::{SessionConfig, SessionRegistry, ShellDefaults};
use arin_lib::tree_watch::{TreeChangeSink, TreeWatch};
use arin_platform::desktop::MacPasteboard;
use arin_platform::fs::{LocalFileSystem, STAGING_PREFIX};
use arin_platform::system::UrandomIdSource;
use arin_platform::watch::FsEventsWatcher;
use serde_json::json;
use support::request_with;
use tauri::ipc::InvokeBody;
use tauri::test::{MockRuntime, get_ipc_response};
use tauri::{Listener, Manager, RunEvent};

const WAIT: Duration = Duration::from_secs(10);
/// 「来ない」ことを確かめるときに待つ時間。
const QUIET: Duration = Duration::from_millis(200);
const HOME: &str = "/Users/someone";

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
// 偽の監視
// ---------------------------------------------------------------------------

/// 張った監視 1 本の記録。
struct Watched {
    root: PathBuf,
    sink: Arc<WatchSink>,
    alive: Arc<AtomicBool>,
}

/// 張った場所を記録する監視。`fail` に入れた場所には張れない。
#[derive(Default)]
struct FakeWatcher {
    watches: Mutex<Vec<Watched>>,
    fail: Mutex<HashSet<PathBuf>>,
}

struct FakeWatch(Arc<AtomicBool>);

impl ActiveWatch for FakeWatch {}

impl Drop for FakeWatch {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

impl WatcherPort for FakeWatcher {
    fn watch(&self, root: &Path, sink: WatchSink) -> Result<Box<dyn ActiveWatch>, WatchError> {
        if lock(&self.fail).contains(root) {
            return Err(WatchError("張れない(テスト)".into()));
        }
        let alive = Arc::new(AtomicBool::new(true));
        lock(&self.watches).push(Watched {
            root: root.to_path_buf(),
            sink: Arc::new(sink),
            alive: Arc::clone(&alive),
        });
        Ok(Box::new(FakeWatch(alive)))
    }
}

impl FakeWatcher {
    /// 張ったことのある回数。
    fn count(&self) -> usize {
        lock(&self.watches).len()
    }

    /// いま張っている(捨てられていない)場所。
    fn active(&self) -> Vec<PathBuf> {
        lock(&self.watches)
            .iter()
            .filter(|w| w.alive.load(Ordering::SeqCst))
            .map(|w| w.root.clone())
            .collect()
    }

    /// いま張っている監視の受け口へ束を流す(本物の監視のスレッドの代わり)。張っていなければ偽。
    fn push(&self, batch: WatchBatch) -> bool {
        let sink = lock(&self.watches)
            .iter()
            .rev()
            .find(|w| w.alive.load(Ordering::SeqCst))
            .map(|w| Arc::clone(&w.sink));
        match sink {
            Some(sink) => {
                sink(batch);
                true
            }
            None => false,
        }
    }
}

/// 本物の監視を包み、張り終えた回数を数える。
struct CountingWatcher {
    inner: FsEventsWatcher,
    started: Arc<AtomicUsize>,
}

impl WatcherPort for CountingWatcher {
    fn watch(&self, root: &Path, sink: WatchSink) -> Result<Box<dyn ActiveWatch>, WatchError> {
        let watch = self.inner.watch(root, sink)?;
        self.started.fetch_add(1, Ordering::SeqCst);
        Ok(watch)
    }
}

/// 起動しない PTY(このテストはシェルを起動しない)。
struct NoPty;

impl PtyPort for NoPty {
    fn spawn(&self, _spec: &ShellLaunchSpec, _size: PtySize) -> Result<SpawnedPty, PtyError> {
        Err(PtyError::Spawn("このテストではシェルを起動しない".into()))
    }
}

// ---------------------------------------------------------------------------
// 組み立て
// ---------------------------------------------------------------------------

fn project_id(n: u8) -> ProjectId {
    ProjectId::from_random_bytes([n; 16])
}

/// プロジェクト alpha(1)・beta(2)を一時フォルダに作り、alpha を選んだセッション管理と、張り替えのスレッド。
struct Fixture {
    _temp: support::TempDir,
    work: PathBuf,
    sessions: SessionRegistry,
    watch: TreeWatch,
    changes: Arc<Mutex<Vec<TreeChange>>>,
}

impl Fixture {
    fn new(name: &str, watcher: Arc<dyn WatcherPort>) -> Self {
        let temp = support::TempDir::new(name);
        let work = temp.path().to_path_buf();
        let mut projects = Vec::new();
        let mut roots = Vec::new();
        for (n, dir) in [(1_u8, "alpha"), (2, "beta")] {
            let path = work.join(dir);
            fs::create_dir(&path).expect("フォルダを作れなかった");
            // 登録したパスは元の形(`/var/...`)のまま。実パスへのそろえは監視の側が行う。
            let root = RootReference::new(path, None);
            projects.push(Project::restore(
                project_id(n),
                root.clone(),
                None,
                ProjectColor::Blue,
            ));
            roots.push((project_id(n), RootResolution::Found(root)));
        }
        let workspace = Workspace::restore(
            "0.0.0-test",
            PathBuf::from(HOME),
            [("HOME", HOME)],
            Startup {
                state: PersistedState {
                    projects,
                    selected_project_id: Some(project_id(1)),
                    ..PersistedState::default()
                },
                roots,
                notice: None,
            },
        );
        // `lib.rs` と同じく、写しを送るたびに張り替えを頼む。
        let cell: Arc<OnceLock<TreeWatch>> = Arc::new(OnceLock::new());
        let rewatch = Arc::clone(&cell);
        let sessions = SessionRegistry::new(
            workspace,
            Arc::new(NoPty),
            Arc::new(|| ShellDefaults {
                shell: PathBuf::from("/bin/zsh"),
                locale: None,
            }),
            Arc::new(move |_| {
                if let Some(watch) = rewatch.get() {
                    watch.refresh();
                }
            }),
            SessionConfig::default(),
        );
        let changes = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&changes);
        let emit: TreeChangeSink = Arc::new(move |change| lock(&seen).push(change));
        let watch = TreeWatch::spawn(
            sessions.clone(),
            watcher,
            Arc::new(LocalFileSystem::new()),
            emit,
        )
        .expect("張り替えのスレッドを起動できなかった");
        let _ = cell.set(watch.clone());
        Self {
            _temp: temp,
            work,
            sessions,
            watch,
            changes,
        }
    }

    fn path(&self, dir: &str) -> PathBuf {
        self.work.join(dir)
    }

    /// 監視が届けるのと同じ形(実パス)。
    fn real(&self, dir: &str) -> PathBuf {
        fs::canonicalize(self.path(dir)).expect("実パスにできなかった")
    }

    fn changes(&self) -> Vec<TreeChange> {
        lock(&self.changes).clone()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.watch.stop();
    }
}

fn fixture(name: &str) -> (Fixture, Arc<FakeWatcher>) {
    let watcher = Arc::new(FakeWatcher::default());
    let f = Fixture::new(name, Arc::clone(&watcher) as Arc<dyn WatcherPort>);
    (f, watcher)
}

fn rel(path: &str) -> RelPath {
    RelPath::parse(path).unwrap_or_else(|e| panic!("{path:?} を相対パスにできなかった: {e}"))
}

fn batch(paths: &[PathBuf]) -> WatchBatch {
    WatchBatch {
        paths: paths.to_vec(),
        rescan: false,
    }
}

/// `actual` の監視が `expected` だけになるまで待つ。
fn wait_active(watcher: &FakeWatcher, expected: &[PathBuf]) {
    assert!(
        wait_until(|| watcher.active() == expected),
        "監視の場所が {expected:?} にならない: {:?}",
        watcher.active()
    );
}

// ---------------------------------------------------------------------------
// 張り替え(偽の監視)
// ---------------------------------------------------------------------------

fn 選択中のプロジェクトのルートにだけ監視を張る() {
    let (f, watcher) = fixture("only-selected");

    wait_active(&watcher, &[f.real("alpha")]);
    thread::sleep(QUIET);
    assert_eq!(watcher.count(), 1, "選択中のほかにも張った");
    assert_eq!(watcher.active(), [f.real("alpha")]);
}

fn プロジェクトを切り替えると前の監視をやめ新しいルートに張る() {
    let (f, watcher) = fixture("switch");
    wait_active(&watcher, &[f.real("alpha")]);

    f.sessions
        .select_project(&project_id(2))
        .expect("選べなかった");

    wait_active(&watcher, &[f.real("beta")]);
    assert_eq!(watcher.count(), 2);
    // 前の監視は捨てられているので、束を流しても alpha のものは送られない。
    assert!(watcher.push(batch(&[f.real("beta").join("b.txt")])));
    assert_eq!(
        f.changes(),
        [TreeChange {
            project_id: project_id(2),
            dirs: vec![RelPath::root()],
            full_reload: false,
        }]
    );
}

fn ルートが改名に追従すると新しいパスに張り替える() {
    let (f, watcher) = fixture("follow");
    wait_active(&watcher, &[f.real("alpha")]);

    fs::rename(f.path("alpha"), f.path("alpha-renamed")).expect("改名できなかった");
    let followed = f
        .sessions
        .follow_project_root(
            &project_id(1),
            &f.path("alpha"),
            RootReference::new(f.path("alpha-renamed"), None),
        )
        .expect("追従できなかった");
    assert!(followed, "ルートが変わらなかった");

    wait_active(&watcher, &[f.real("alpha-renamed")]);
    assert_eq!(watcher.count(), 2);
}

fn 見つからないになると監視をやめ見つかると張り直す() {
    let (f, watcher) = fixture("missing");
    wait_active(&watcher, &[f.real("alpha")]);

    assert!(
        f.sessions
            .mark_project_missing(&project_id(1), &f.path("alpha"))
            .expect("見つからないにできなかった")
    );
    wait_active(&watcher, &[]);

    // 同じパスで見つかった(見回りの追従と同じ呼び方)。
    assert!(
        f.sessions
            .follow_project_root(
                &project_id(1),
                &f.path("alpha"),
                RootReference::new(f.path("alpha"), None),
            )
            .expect("追従できなかった")
    );
    wait_active(&watcher, &[f.real("alpha")]);
    assert_eq!(watcher.count(), 2, "張り直していない");
}

fn 削除で選択中が無くなると監視をやめる() {
    let (f, watcher) = fixture("remove");
    wait_active(&watcher, &[f.real("alpha")]);

    f.sessions
        .remove_project(&project_id(1), true)
        .expect("外せなかった");
    // 選択はほかのプロジェクトに移るか、無くなる。移ったならそのルートだけを監視する。
    let selected = f.sessions.snapshot().selected_project_id;
    let expected: Vec<PathBuf> = if selected == Some(project_id(2)) {
        vec![f.real("beta")]
    } else {
        Vec::new()
    };
    wait_active(&watcher, &expected);

    if selected.is_some() {
        f.sessions
            .remove_project(&project_id(2), true)
            .expect("外せなかった");
    }
    // 0 件。
    wait_active(&watcher, &[]);
    assert!(f.watch.is_running(), "0 件で張り替えのスレッドが止まった");
}

fn 監視を張れないときは落ちずに監視しない() {
    let (f, watcher) = fixture("fail");
    wait_active(&watcher, &[f.real("alpha")]);
    lock(&watcher.fail).insert(f.real("beta"));

    f.sessions
        .select_project(&project_id(2))
        .expect("選べなかった");
    wait_active(&watcher, &[]);
    assert!(
        f.watch.is_running(),
        "張れなかったら張り替えのスレッドが止まった"
    );

    // 対象が変われば張り直す。
    f.sessions
        .select_project(&project_id(1))
        .expect("選べなかった");
    wait_active(&watcher, &[f.real("alpha")]);
}

// ---------------------------------------------------------------------------
// 送るもの(偽の監視)
// ---------------------------------------------------------------------------

fn 変わったフォルダを相対パスでまとめて送る() {
    let (f, watcher) = fixture("summarize");
    let root = f.real("alpha");
    wait_active(&watcher, std::slice::from_ref(&root));

    assert!(watcher.push(batch(&[
        root.join("src/a.ts"),
        root.join("src/b.ts"),
        root.join("docs/x/y.md"),
        root.join("top.txt"),
        // 監視しているルートの外は捨てる。
        f.real("beta").join("z.txt"),
    ])));
    assert!(watcher.push(WatchBatch {
        paths: Vec::new(),
        rescan: true,
    }));

    let changes = f.changes();
    assert_eq!(
        changes,
        [
            TreeChange {
                project_id: project_id(1),
                dirs: vec![rel(""), rel("docs/x"), rel("src")],
                full_reload: false,
            },
            TreeChange {
                project_id: project_id(1),
                dirs: Vec::new(),
                full_reload: true,
            },
        ]
    );
    // 画面へ送る形(JSON)に絶対パスが入らない。
    let text = serde_json::to_string(&changes).expect("JSON にできなかった");
    for path in [f.path("alpha"), root] {
        assert!(
            !text.contains(&path.display().to_string()),
            "絶対パスが入っている: {text}"
        );
    }
    assert!(
        text.contains("\"projectId\""),
        "画面の形(camelCase)でない: {text}"
    );
}

fn 除外の中の変更は送らない() {
    let (f, watcher) = fixture("exclusion");
    let root = f.real("alpha");
    wait_active(&watcher, std::slice::from_ref(&root));

    assert!(watcher.push(batch(&[
        root.join(".git/HEAD"),
        root.join("node_modules/pkg/index.js"),
        root.join(".DS_Store"),
        // コピーの作業用フォルダの出入りと中身。
        root.join(format!("{STAGING_PREFIX}1-0")),
        root.join(format!("{STAGING_PREFIX}1-0/a.txt")),
        root.join(format!("src/{STAGING_PREFIX}2-0/b.txt")),
    ])));
    thread::sleep(QUIET);
    assert_eq!(f.changes(), [], "除外の中の変更を送った");

    // 除外と並べて届いた変更は送る。
    assert!(watcher.push(batch(&[
        root.join(".git/index"),
        root.join(format!("{STAGING_PREFIX}3-0/c.txt")),
        root.join("src/c.txt"),
    ])));
    assert_eq!(
        f.changes(),
        [TreeChange {
            project_id: project_id(1),
            dirs: vec![rel("src")],
            full_reload: false,
        }]
    );
}

fn 隠しファイル表示を切り替えると監視の除外の規則も変わる() {
    let (f, watcher) = fixture("show-hidden");
    let root = f.real("alpha");
    wait_active(&watcher, std::slice::from_ref(&root));

    f.sessions
        .set_project_show_hidden(&project_id(1), true)
        .expect("切り替えられなかった");

    // 規則の差し替えは張り替えのスレッドで行うので、届くまで流し直す。
    let git = || f.changes().iter().any(|c| c.dirs.contains(&rel(".git")));
    assert!(
        wait_until(|| {
            watcher.push(batch(&[root.join(".git/HEAD")]));
            git()
        }),
        "隠しファイル表示を ON にしても .git の中の変更が送られない"
    );
    assert_eq!(watcher.count(), 1, "隠しファイル表示の切替で張り替えた");
    // 作業用フォルダは ON でも送らない。
    lock(&f.changes).clear();
    assert!(watcher.push(batch(&[root.join(format!("{STAGING_PREFIX}1-0/a.txt"))])));
    assert_eq!(f.changes(), []);

    // OFF に戻すと、また送らない。
    f.sessions
        .set_project_show_hidden(&project_id(1), false)
        .expect("切り替えられなかった");
    let quiet_since = |since: &mut Option<Instant>| {
        lock(&f.changes).clear();
        watcher.push(batch(&[root.join(".git/HEAD")]));
        if f.changes().is_empty() {
            since.get_or_insert_with(Instant::now).elapsed() >= QUIET
        } else {
            *since = None;
            false
        }
    };
    let mut since = None;
    assert!(
        wait_until(|| quiet_since(&mut since)),
        "隠しファイル表示を OFF に戻しても .git の中の変更が送られる"
    );
}

fn 止めると監視を捨ててスレッドが残らない() {
    let (f, watcher) = fixture("stop");
    wait_active(&watcher, &[f.real("alpha")]);

    let started = Instant::now();
    f.watch.stop();

    assert!(!f.watch.is_running(), "止めてもスレッドが動いている");
    assert_eq!(
        watcher.active(),
        Vec::<PathBuf>::new(),
        "止めても監視が残っている"
    );
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "止めるのに時間がかかった: {:?}",
        started.elapsed()
    );
    // 止めたあとの写しでは張り直さない。2 回目の stop も何もしない。
    f.sessions
        .select_project(&project_id(2))
        .expect("選べなかった");
    thread::sleep(QUIET);
    assert_eq!(watcher.count(), 1);
    f.watch.stop();
}

// ---------------------------------------------------------------------------
// 実物の監視(一時フォルダ)
// ---------------------------------------------------------------------------

fn 実物の一時フォルダでファイルを作ると_2_秒以内に_tree_changed_が届く() {
    let started = Arc::new(AtomicUsize::new(0));
    let f = Fixture::new(
        "fsevents",
        Arc::new(CountingWatcher {
            inner: FsEventsWatcher::new(),
            started: Arc::clone(&started),
        }),
    );
    fs::create_dir(f.path("alpha").join("src")).expect("フォルダを作れなかった");
    assert!(
        wait_until(|| started.load(Ordering::SeqCst) == 1),
        "監視を張らなかった"
    );
    // 張る前の変更(src を作ったこと)が遅れて届くことがあるので、落ち着いてから捨てる。
    thread::sleep(Duration::from_millis(500));
    lock(&f.changes).clear();

    let touched = Instant::now();
    fs::write(f.path("alpha").join("src/new.txt"), "x").expect("ファイルを作れなかった");
    let arrived = (|| {
        let deadline = touched + Duration::from_secs(2);
        while Instant::now() < deadline {
            if f.changes().iter().any(|c| {
                c.project_id == project_id(1) && (c.full_reload || c.dirs.contains(&rel("src")))
            }) {
                return Some(touched.elapsed());
            }
            thread::sleep(Duration::from_millis(10));
        }
        None
    })();
    let Some(took) = arrived else {
        panic!("2 秒以内に届かなかった: {:?}", f.changes());
    };
    println!("    (tree-changed まで {took:?})");

    let text = serde_json::to_string(&f.changes()).expect("JSON にできなかった");
    assert!(
        !text.contains(&f.work.display().to_string())
            && !text.contains(&f.real("alpha").display().to_string()),
        "絶対パスが入っている: {text}"
    );
    f.watch.stop();
    assert!(!f.watch.is_running());
}

type Check = fn();

const CHECKS: &[(&str, Check)] = &[
    (
        "選択中のプロジェクトのルートにだけ監視を張る",
        選択中のプロジェクトのルートにだけ監視を張る,
    ),
    (
        "プロジェクトを切り替えると前の監視をやめ新しいルートに張る",
        プロジェクトを切り替えると前の監視をやめ新しいルートに張る,
    ),
    (
        "ルートが改名に追従すると新しいパスに張り替える",
        ルートが改名に追従すると新しいパスに張り替える,
    ),
    (
        "見つからないになると監視をやめ見つかると張り直す",
        見つからないになると監視をやめ見つかると張り直す,
    ),
    (
        "削除で選択中が無くなると監視をやめる",
        削除で選択中が無くなると監視をやめる,
    ),
    (
        "監視を張れないときは落ちずに監視しない",
        監視を張れないときは落ちずに監視しない,
    ),
    (
        "変わったフォルダを相対パスでまとめて送る",
        変わったフォルダを相対パスでまとめて送る,
    ),
    ("除外の中の変更は送らない", 除外の中の変更は送らない),
    (
        "隠しファイル表示を切り替えると監視の除外の規則も変わる",
        隠しファイル表示を切り替えると監視の除外の規則も変わる,
    ),
    (
        "止めると監視を捨ててスレッドが残らない",
        止めると監視を捨ててスレッドが残らない,
    ),
    (
        "実物の一時フォルダでファイルを作ると_2_秒以内に_tree_changed_が届く",
        実物の一時フォルダでファイルを作ると_2_秒以内に_tree_changed_が届く,
    ),
];

// ---------------------------------------------------------------------------
// 模擬のアプリ
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

/// 模擬のアプリを組み立てて走らせる。`on_ready` は起動の完了で 1 回呼ぶ。`on_exit` は終了の後片付けのあと。
fn run_app(
    name: &str,
    watcher: Arc<FakeWatcher>,
    picker: Arc<FakePicker>,
    on_ready: impl Fn(&tauri::AppHandle<MockRuntime>, &tauri::WebviewWindow<MockRuntime>) + 'static,
    on_exit: impl Fn(&tauri::AppHandle<MockRuntime>) + 'static,
) {
    let temp = support::TempDir::new(name);
    let launch = support::launch_in(&temp, false);
    let pasteboard = MacPasteboard::named(format!(
        "dev.arin.test.tree-watch.{name}.{}",
        std::process::id()
    ));
    assert!(
        !pasteboard.is_general(),
        "ユーザーのクリップボードを使おうとした"
    );
    let project_ports = ProjectPorts {
        ids: Arc::new(UrandomIdSource::new()),
        pasteboard: Arc::new(pasteboard.clone()),
        picker: Some(picker as Arc<dyn FolderPicker>),
    };
    let app = arin_lib::builder_with_watcher::<MockRuntime>(
        launch,
        Arc::new(NoPty),
        Arc::new(|| ShellDefaults {
            shell: PathBuf::from("/bin/zsh"),
            locale: None,
        }),
        PersistencePorts::native(temp.path().to_path_buf()),
        project_ports,
        Arc::new(LocalFileSystem::new()),
        watcher,
    )
    .build(tauri::generate_context!())
    .unwrap_or_else(|e| panic!("組み立てに失敗した: {e}"));

    let readied = Arc::new(AtomicBool::new(false));
    let exited = Arc::new(AtomicBool::new(false));
    let (seen_ready, seen_exit) = (Arc::clone(&readied), Arc::clone(&exited));
    app.run_return(move |handle, event| {
        if let RunEvent::Ready = event {
            let window = handle
                .get_webview_window("main")
                .expect("設定にあるウィンドウ main が作られなかった");
            on_ready(handle, &window);
            seen_ready.store(true, Ordering::SeqCst);
            for window in handle.webview_windows().into_values() {
                window.destroy().expect("ウィンドウを閉じられなかった");
            }
        }
        let exiting = matches!(event, RunEvent::Exit);
        arin_lib::lifecycle::on_run_event(handle, event);
        if exiting {
            on_exit(handle);
            seen_exit.store(true, Ordering::SeqCst);
        }
    });
    pasteboard.release();
    assert!(readied.load(Ordering::SeqCst), "起動の完了まで進まなかった");
    assert!(exited.load(Ordering::SeqCst), "終了のイベントが来なかった");
}

fn 画面へ_tree_changed_を送り終了で監視を捨てる() {
    let watcher = Arc::new(FakeWatcher::default());
    let picker = Arc::new(FakePicker::default());
    let work = support::TempDir::new("tree-watch-app-work");
    let alpha = work.path().join("alpha");
    fs::create_dir(&alpha).expect("フォルダを作れなかった");
    let real = fs::canonicalize(&alpha).expect("実パスにできなかった");
    lock(&picker.answers).push_back(Some(alpha.clone()));

    let in_ready = Arc::clone(&watcher);
    let in_exit = Arc::clone(&watcher);
    let real_in_ready = real.clone();
    run_app(
        "tree-watch-app",
        Arc::clone(&watcher),
        picker,
        move |handle, window| {
            // プロジェクトが無い起動では張らない。
            assert!(
                handle.try_state::<TreeWatch>().is_some(),
                "張り替えのスレッドが無い"
            );
            thread::sleep(QUIET);
            assert_eq!(in_ready.count(), 0, "プロジェクトが無いのに張った");

            let outcome: AddProjectOutcome = get_ipc_response(
                window,
                request_with(handle, "add_project_by_dialog", InvokeBody::Json(json!({}))),
            )
            .unwrap_or_else(|e| panic!("add_project_by_dialog が失敗した: {e}"))
            .deserialize()
            .expect("AddProjectOutcome として読めなかった");
            let AddProjectOutcome::Added { project_id } = outcome else {
                panic!("登録されなかった: {outcome:?}");
            };
            wait_active(&in_ready, std::slice::from_ref(&real_in_ready));

            let payloads = Arc::new(Mutex::new(Vec::<String>::new()));
            let seen = Arc::clone(&payloads);
            handle.listen(TREE_CHANGED_EVENT, move |event| {
                lock(&seen).push(event.payload().to_owned());
            });
            assert!(in_ready.push(batch(&[real_in_ready.join("src/a.ts")])));
            assert!(
                wait_until(|| !lock(&payloads).is_empty()),
                "tree-changed が届かない"
            );
            let payload: serde_json::Value =
                serde_json::from_str(&lock(&payloads)[0]).expect("中身が JSON でない");
            assert_eq!(
                payload,
                json!({ "projectId": project_id.as_str(), "dirs": ["src"], "fullReload": false })
            );
        },
        move |handle| {
            assert!(
                !handle.state::<TreeWatch>().is_running(),
                "終了しても張り替えのスレッドが止まらない"
            );
            assert_eq!(
                in_exit.active(),
                Vec::<PathBuf>::new(),
                "終了しても監視が残っている"
            );
        },
    );
    assert_eq!(watcher.count(), 1);
}

const APP_CHECKS: &[(&str, Check)] = &[(
    "画面へ_tree_changed_を送り終了で監視を捨てる",
    画面へ_tree_changed_を送り終了で監視を捨てる,
)];

fn main() {
    for (name, check) in CHECKS.iter().chain(APP_CHECKS) {
        check();
        println!("test {name} ... ok");
    }
    let _ = io::Write::flush(&mut io::stdout());
    println!(
        "test result: ok. {} passed",
        CHECKS.len() + APP_CHECKS.len()
    );
}
