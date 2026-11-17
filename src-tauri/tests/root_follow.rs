//! ルートへの追従の見回り(`src-tauri/src/patrol.rs`)と、ペインの寸法のコマンド(`src-tauri/src/commands/layout.rs`)。
//! 計画書 T33d、FR-007(AC-P07 と「見つからない」)、FR-009(寸法の保存)、設計書 §7.1「ルートフォルダへの追従」。
//!
//! - 見回りの 1 回分(`RootPatrol::tick`)を直接呼ぶ。1 秒を実際には待たない
//! - フォルダは偽物(`FakeWorld`。フォルダの実体を番号で持ち、改名・削除・作り直しを表せる)。実物の一時フォルダで 1 件
//! - 最後に Tauri の模擬ランタイムで、寸法のコマンドと終了時に見回りのスレッドが止まることを確かめる
//!
//! macOS ではメニューの部品をメインスレッドでしか作れないため、`harness = false` で走らせる(`app_builder` と同じ)。

mod support;

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use arin_core::persistence::codec::PersistedState;
use arin_core::ports::{
    BookmarkPort, OpenRoot, PtyError, PtyPort, PtySize, ResolvedBookmark, RootError,
    RootLocatorPort, SpawnedPty,
};
use arin_core::project::{Project, ProjectColor, ProjectId, RootReference};
use arin_core::terminal::ShellLaunchSpec;
use arin_core::workspace::{AppSnapshot, ProjectCardSnapshot, RootResolution, Startup, Workspace};
use arin_lib::patrol::{PatrolThread, RootPatrol};
use arin_lib::persistence::PersistencePorts;
use arin_lib::sessions::{SessionConfig, SessionRegistry, ShellDefaults};
use arin_platform::bookmark::MacBookmarks;
use arin_platform::watch::FsRootLocator;
use serde_json::json;
use support::{request, request_with};
use tauri::ipc::InvokeBody;
use tauri::test::{MockRuntime, get_ipc_response};
use tauri::{AppHandle, Manager, RunEvent, WebviewWindow};

const WAIT: Duration = Duration::from_secs(10);
const HOME: &str = "/Users/someone";
const ALPHA: &str = "/Users/someone/dev/alpha";
const BETA: &str = "/Users/someone/dev/beta";

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn wait_until(mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        thread::sleep(Duration::from_millis(2));
    }
    condition()
}

// ---------------------------------------------------------------------------
// 偽のフォルダ
// ---------------------------------------------------------------------------

/// フォルダの実体(番号)といまのパス。改名・移動はパスを変え、削除は実体を消す。
#[derive(Default)]
struct WorldState {
    folders: HashMap<u64, PathBuf>,
    next: u64,
    /// 次の `current_path` を何回 `NotFound` にするか(改名と同時に調べたときの、まれな失敗)。
    flaky: usize,
}

#[derive(Default)]
struct FakeWorld {
    state: Mutex<WorldState>,
    /// 開いているフォルダの数(記述子の数にあたる)。
    open: Arc<AtomicUsize>,
    /// これまでに開いた回数。
    opened: AtomicUsize,
}

impl FakeWorld {
    fn create(&self, path: &str) -> u64 {
        let mut s = lock(&self.state);
        s.next += 1;
        let n = s.next;
        s.folders.insert(n, PathBuf::from(path));
        n
    }

    /// 番号 `n` の実体を `path` に置く(その番号のブックマークが解決できるようになる)。
    fn create_as(&self, n: u64, path: &str) {
        lock(&self.state).folders.insert(n, PathBuf::from(path));
    }

    fn rename(&self, from: &str, to: &str) {
        let mut s = lock(&self.state);
        let folder = s
            .folders
            .values_mut()
            .find(|p| p.as_path() == Path::new(from))
            .unwrap_or_else(|| panic!("{from} が無い"));
        *folder = PathBuf::from(to);
    }

    fn delete(&self, path: &str) {
        lock(&self.state)
            .folders
            .retain(|_, p| p.as_path() != Path::new(path));
    }

    fn flaky(&self, times: usize) {
        lock(&self.state).flaky = times;
    }

    fn find(&self, path: &Path) -> Option<u64> {
        lock(&self.state)
            .folders
            .iter()
            .find(|(_, p)| p.as_path() == path)
            .map(|(n, _)| *n)
    }
}

struct FakeRoots(Arc<FakeWorld>);

impl RootLocatorPort for FakeRoots {
    fn canonical_dir(&self, path: &Path) -> Result<PathBuf, RootError> {
        self.0
            .find(path)
            .map(|_| path.to_path_buf())
            .ok_or_else(|| RootError::NotFound(path.display().to_string()))
    }

    fn open(&self, path: &Path) -> Result<Box<dyn OpenRoot>, RootError> {
        let folder = self
            .0
            .find(path)
            .ok_or_else(|| RootError::NotFound(path.display().to_string()))?;
        self.0.opened.fetch_add(1, Ordering::SeqCst);
        self.0.open.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(FakeOpenRoot {
            world: Arc::clone(&self.0),
            folder,
        }))
    }
}

struct FakeOpenRoot {
    world: Arc<FakeWorld>,
    folder: u64,
}

impl OpenRoot for FakeOpenRoot {
    fn current_path(&self) -> Result<PathBuf, RootError> {
        let mut s = lock(&self.world.state);
        if s.flaky > 0 {
            s.flaky -= 1;
            return Err(RootError::NotFound("改名の途中".into()));
        }
        s.folders
            .get(&self.folder)
            .cloned()
            .ok_or_else(|| RootError::NotFound("削除された".into()))
    }
}

impl Drop for FakeOpenRoot {
    fn drop(&mut self) {
        self.world.open.fetch_sub(1, Ordering::SeqCst);
    }
}

/// ブックマークはフォルダの実体の番号(`bm:N`)。削除した実体には解決できない。
struct FakeBookmarks(Arc<FakeWorld>);

impl BookmarkPort for FakeBookmarks {
    fn create(&self, path: &Path) -> Result<String, RootError> {
        self.0
            .find(path)
            .map(|n| format!("bm:{n}"))
            .ok_or_else(|| RootError::NotFound(path.display().to_string()))
    }

    fn resolve(&self, bookmark: &str) -> Result<ResolvedBookmark, RootError> {
        let n: u64 = bookmark
            .strip_prefix("bm:")
            .and_then(|n| n.parse().ok())
            .ok_or_else(|| RootError::Bookmark(bookmark.into()))?;
        lock(&self.0.state)
            .folders
            .get(&n)
            .map(|path| ResolvedBookmark {
                path: path.clone(),
                stale: false,
            })
            .ok_or_else(|| RootError::NotFound(bookmark.into()))
    }
}

// ---------------------------------------------------------------------------
// 組み立て
// ---------------------------------------------------------------------------

/// 起動しない PTY(このテストはシェルを起動しない)。
struct NoPty;

impl PtyPort for NoPty {
    fn spawn(&self, _spec: &ShellLaunchSpec, _size: PtySize) -> Result<SpawnedPty, PtyError> {
        Err(PtyError::Spawn("このテストではシェルを起動しない".into()))
    }
}

fn project_id(n: u8) -> ProjectId {
    ProjectId::from_random_bytes([n; 16])
}

/// 見回りと、見回りが追うセッション管理。写しを送るたびに記録する。
struct Fixture {
    world: Arc<FakeWorld>,
    sessions: SessionRegistry,
    patrol: RootPatrol,
    snapshots: Arc<Mutex<Vec<AppSnapshot>>>,
}

impl Fixture {
    /// `found` のプロジェクトは起動時に解決できた、`missing` は解決できなかったものとして始める。
    /// フォルダは `found` の分だけ偽の世界に作る。
    fn new(found: &[(u8, &str)], missing: &[(u8, &str)]) -> Self {
        let world = Arc::new(FakeWorld::default());
        let mut projects = Vec::new();
        let mut roots = Vec::new();
        for (n, path) in found {
            let folder = world.create(path);
            let root = RootReference::new(PathBuf::from(path), Some(format!("bm:{folder}")));
            projects.push(Project::restore(
                project_id(*n),
                root.clone(),
                None,
                ProjectColor::Blue,
            ));
            roots.push((project_id(*n), RootResolution::Found(root)));
        }
        for (n, path) in missing {
            let root = RootReference::new(PathBuf::from(path), Some("bm:999".into()));
            projects.push(Project::restore(
                project_id(*n),
                root,
                None,
                ProjectColor::Pink,
            ));
            roots.push((project_id(*n), RootResolution::Missing));
        }
        let workspace = Workspace::restore(
            "0.0.0-test",
            PathBuf::from(HOME),
            [("HOME", HOME)],
            Startup {
                state: PersistedState {
                    projects,
                    selected_project_id: None,
                    ..PersistedState::default()
                },
                roots,
                notice: None,
            },
        );
        let snapshots = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&snapshots);
        let sessions = SessionRegistry::new(
            workspace,
            Arc::new(NoPty),
            Arc::new(|| ShellDefaults {
                shell: PathBuf::from("/bin/zsh"),
                locale: None,
            }),
            Arc::new(move |snapshot| lock(&seen).push(snapshot)),
            SessionConfig::default(),
        );
        let patrol = RootPatrol::new(
            Arc::new(FakeRoots(Arc::clone(&world))),
            Arc::new(FakeBookmarks(Arc::clone(&world))),
        );
        Self {
            world,
            sessions,
            patrol,
            snapshots,
        }
    }

    fn tick(&mut self) {
        self.patrol.tick(&self.sessions);
    }

    fn sent(&self) -> usize {
        lock(&self.snapshots).len()
    }

    fn card(&self, n: u8) -> ProjectCardSnapshot {
        card_of(&self.sessions.snapshot(), n)
    }

    fn saved_root(&self, n: u8) -> RootReference {
        self.sessions
            .persisted_state()
            .projects
            .iter()
            .find(|p| p.id() == &project_id(n))
            .expect("保存する内容にプロジェクトが無い")
            .root()
            .clone()
    }
}

fn card_of(snapshot: &AppSnapshot, n: u8) -> ProjectCardSnapshot {
    snapshot
        .projects
        .iter()
        .find(|p| p.id == project_id(n))
        .cloned()
        .unwrap_or_else(|| panic!("プロジェクト {n} のカードが無い"))
}

// ---------------------------------------------------------------------------
// 見回り(偽のフォルダ)
// ---------------------------------------------------------------------------

fn 改名するとカードは同じプロジェクトのまま表示用のパスが新しくなる() {
    let mut f = Fixture::new(&[(1, ALPHA)], &[]);
    f.tick();
    let before = f.card(1);
    let sent = f.sent();

    f.world.rename(ALPHA, "/Users/someone/dev/alpha2");
    f.tick();

    let after = f.card(1);
    assert_eq!(after.id, before.id);
    assert_eq!(after.color, before.color);
    assert_eq!(after.terminal_ids, before.terminal_ids);
    assert_eq!(after.display_path, "~/dev/alpha2");
    assert_eq!(after.name, "alpha2");
    assert!(!after.missing);
    assert_eq!(f.sent(), sent + 1, "写しを 1 回送っていない");
    // 保存する内容(保存は写しを送ると予約される)にも新しいパスとブックマークが載る。
    let saved = f.saved_root(1);
    assert_eq!(
        saved.last_known_path(),
        Path::new("/Users/someone/dev/alpha2")
    );
    assert!(saved.bookmark().is_some());
}

fn 移動のあとも続けて追従する() {
    let mut f = Fixture::new(&[(1, ALPHA)], &[]);
    f.tick();

    f.world.rename(ALPHA, "/Users/someone/dev/alpha2");
    f.tick();
    f.world
        .rename("/Users/someone/dev/alpha2", "/Users/someone/work/alpha3");
    f.tick();

    assert_eq!(f.card(1).display_path, "~/work/alpha3");
}

fn 削除すると見つからないになりカードは残る() {
    let mut f = Fixture::new(&[(1, ALPHA)], &[]);
    f.tick();
    let tabs = f.card(1).terminal_ids;

    f.world.delete(ALPHA);
    f.tick();
    assert!(
        !f.card(1).missing,
        "1 回の失敗で見つからないにした(次の見回りで確かめ直す)"
    );
    f.tick();

    let after = f.card(1);
    assert!(after.missing, "見つからないにならない");
    assert_eq!(after.display_path, "~/dev/alpha");
    assert_eq!(after.terminal_ids, tabs, "タブが変わった");
    assert_eq!(f.sessions.snapshot().projects.len(), 1, "カードが消えた");
}

fn 改名と同時の一時的な失敗では見つからないにしない() {
    let mut f = Fixture::new(&[(1, ALPHA)], &[]);
    f.tick();

    f.world.flaky(1);
    f.world.rename(ALPHA, "/Users/someone/dev/alpha2");
    f.tick();
    assert!(!f.card(1).missing);
    f.tick();

    let after = f.card(1);
    assert!(!after.missing);
    assert_eq!(after.display_path, "~/dev/alpha2");
}

fn ゴミ箱へ移すと見つからない() {
    let mut f = Fixture::new(&[(1, ALPHA)], &[]);
    f.tick();

    f.world.rename(ALPHA, "/Users/someone/.Trash/alpha");
    f.tick();

    let after = f.card(1);
    assert!(after.missing);
    assert_eq!(after.display_path, "~/dev/alpha", "ゴミ箱のパスに追従した");
    assert_eq!(f.saved_root(1).last_known_path(), Path::new(ALPHA));
    // ゴミ箱にあるあいだは写しを送り続けない。
    let sent = f.sent();
    f.tick();
    f.tick();
    assert_eq!(f.sent(), sent);
}

fn ゴミ箱から戻すと見つからないが解ける() {
    let mut f = Fixture::new(&[(1, ALPHA)], &[]);
    f.tick();
    f.world.rename(ALPHA, "/Users/someone/.Trash/alpha");
    f.tick();
    assert!(f.card(1).missing);

    f.world.rename(
        "/Users/someone/.Trash/alpha",
        "/Users/someone/dev/alpha-back",
    );
    f.tick();

    let after = f.card(1);
    assert!(!after.missing);
    assert_eq!(after.display_path, "~/dev/alpha-back");
}

fn 変わらなければ写しを送らない() {
    let mut f = Fixture::new(&[(1, ALPHA), (2, BETA)], &[(3, "/Users/someone/dev/gone")]);
    f.tick();
    let sent = f.sent();

    for _ in 0..5 {
        f.tick();
    }

    assert_eq!(f.sent(), sent, "何も変わらないのに写しを送った");
    assert_eq!(sent, 0, "起動直後の見回りで写しを送った");
}

fn 見つからないプロジェクトがブックマークで解決できると戻りタブは開かない() {
    let mut f = Fixture::new(&[], &[(3, "/Users/someone/dev/gone")]);
    f.tick();
    assert!(f.card(3).missing);

    // ブックマーク(bm:999)の指す実体が現れた(外付けのボリュームをつないだ、など)。
    f.world.create_as(999, "/Users/someone/dev/elsewhere");
    f.tick();

    let after = f.card(3);
    assert!(!after.missing);
    assert_eq!(after.display_path, "~/dev/elsewhere");
    assert!(after.terminal_ids.is_empty(), "タブを開いた");
}

fn 見つからないプロジェクトの元の場所にフォルダが戻ると見つからないが解ける() {
    let mut f = Fixture::new(&[(1, ALPHA)], &[]);
    f.tick();
    f.world.delete(ALPHA);
    f.tick();
    f.tick();
    assert!(f.card(1).missing);

    f.world.create(ALPHA);
    f.tick();

    assert!(!f.card(1).missing);
    assert_eq!(f.card(1).display_path, "~/dev/alpha");
}

fn 登録_削除_再リンクに合わせて開いているフォルダを張り替える() {
    let mut f = Fixture::new(&[(1, ALPHA)], &[]);
    f.tick();
    assert_eq!(f.world.open.load(Ordering::SeqCst), 1);

    // 登録すると、次の見回りで開く。
    f.world.create(BETA);
    f.sessions
        .register_project(project_id(2), RootReference::new(PathBuf::from(BETA), None))
        .expect("登録できなかった");
    f.tick();
    assert_eq!(f.world.open.load(Ordering::SeqCst), 2);
    f.world.rename(BETA, "/Users/someone/dev/beta2");
    f.tick();
    assert_eq!(f.card(2).display_path, "~/dev/beta2");

    // 再リンクすると、古いフォルダを閉じて新しいフォルダを開く。古いフォルダの改名には追従しない。
    f.world.create("/Users/someone/dev/other");
    f.sessions
        .relink_project(
            &project_id(1),
            RootReference::new(PathBuf::from("/Users/someone/dev/other"), None),
        )
        .expect("再リンクできなかった");
    f.tick();
    assert_eq!(f.world.open.load(Ordering::SeqCst), 2);
    f.world.rename(ALPHA, "/Users/someone/dev/alpha-old");
    f.world
        .rename("/Users/someone/dev/other", "/Users/someone/dev/other2");
    f.tick();
    assert_eq!(f.card(1).display_path, "~/dev/other2");

    // 削除すると閉じる。
    f.sessions
        .remove_project(&project_id(2), true)
        .expect("削除できなかった");
    f.tick();
    assert_eq!(f.world.open.load(Ordering::SeqCst), 1);
    // 開き直し続けない。
    let opened = f.world.opened.load(Ordering::SeqCst);
    f.tick();
    f.tick();
    assert_eq!(f.world.opened.load(Ordering::SeqCst), opened);
}

fn 別のプロジェクトと同じ場所へ移ったときは追従しない() {
    let mut f = Fixture::new(&[(1, ALPHA)], &[(2, BETA)]);
    f.tick();

    // 見つからない B の元の場所に、A を改名して置いた。
    f.world.rename(ALPHA, BETA);
    f.tick();
    f.tick();

    // A は登録済みのフォルダ(B)と重なるので動かさない。B は元の場所のフォルダで戻る(既知の制約と同じ扱い)。
    assert_eq!(f.card(1).display_path, "~/dev/alpha");
    let s = f.sessions.snapshot();
    let paths: Vec<&str> = s.projects.iter().map(|p| p.display_path.as_str()).collect();
    assert_eq!(paths, ["~/dev/alpha", "~/dev/beta"]);
}

fn 見回りのスレッドは間隔ごとに回り_止めるとすぐ終わる() {
    let f = Fixture::new(&[(1, ALPHA)], &[]);
    let Fixture {
        world,
        sessions,
        patrol,
        ..
    } = f;
    let thread = PatrolThread::spawn(sessions.clone(), patrol, Duration::from_millis(5))
        .expect("見回りのスレッドを起動できなかった");
    assert!(thread.is_running());

    world.rename(ALPHA, "/Users/someone/dev/alpha2");
    assert!(
        wait_until(|| card_of(&sessions.snapshot(), 1).display_path == "~/dev/alpha2"),
        "見回りのスレッドが追従しない"
    );

    let started = Instant::now();
    thread.stop();
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "止めるのに時間がかかった"
    );
    assert!(!thread.is_running());
    // 2 回目は何もしない。止めたあとは開いていたフォルダを閉じている。
    thread.stop();
    assert_eq!(world.open.load(Ordering::SeqCst), 0);
}

fn 間隔が長くても止めるとすぐ終わる() {
    let f = Fixture::new(&[(1, ALPHA)], &[]);
    let thread = PatrolThread::spawn(f.sessions.clone(), f.patrol, Duration::from_secs(3600))
        .expect("見回りのスレッドを起動できなかった");
    let started = Instant::now();
    thread.stop();
    assert!(started.elapsed() < Duration::from_millis(500));
}

// ---------------------------------------------------------------------------
// 見回り(実物の一時フォルダ)
// ---------------------------------------------------------------------------

fn 実物のフォルダを改名するとカードの表示用のパスが新しくなる() {
    let temp = support::TempDir::new("root-follow-real");
    let original = temp.path().join("app");
    fs::create_dir(&original).expect("フォルダを作れなかった");
    let canonical = fs::canonicalize(&original).expect("正規化できなかった");
    let bookmarks = MacBookmarks::new();
    let bookmark = bookmarks
        .create(&canonical)
        .expect("ブックマークを作れなかった");
    let root = RootReference::new(canonical.clone(), Some(bookmark));
    let workspace = Workspace::restore(
        "0.0.0-test",
        PathBuf::from(HOME),
        [("HOME", HOME)],
        Startup {
            state: PersistedState {
                projects: vec![Project::restore(
                    project_id(1),
                    root.clone(),
                    None,
                    ProjectColor::Teal,
                )],
                ..PersistedState::default()
            },
            roots: vec![(project_id(1), RootResolution::Found(root))],
            notice: None,
        },
    );
    let sessions = SessionRegistry::new(
        workspace,
        Arc::new(NoPty),
        Arc::new(|| ShellDefaults {
            shell: PathBuf::from("/bin/zsh"),
            locale: None,
        }),
        Arc::new(|_| {}),
        SessionConfig::default(),
    );
    let mut patrol = RootPatrol::new(
        Arc::new(FsRootLocator::new()),
        Arc::new(MacBookmarks::new()),
    );
    patrol.tick(&sessions);
    let before = card_of(&sessions.snapshot(), 1);

    let renamed = temp.path().join("app2");
    fs::rename(&original, &renamed).expect("改名できなかった");
    patrol.tick(&sessions);

    let after = card_of(&sessions.snapshot(), 1);
    let expected = fs::canonicalize(&renamed).expect("正規化できなかった");
    assert_eq!(after.id, before.id);
    assert_eq!(after.name, "app2");
    assert!(!after.missing);
    assert_eq!(
        sessions
            .project_root_path(&project_id(1))
            .expect("パスが無い"),
        expected
    );
    // 作り直したブックマークは新しいパスへ解決される。
    let saved = sessions.persisted_state();
    let resolved = bookmarks
        .resolve(
            saved.projects[0]
                .root()
                .bookmark()
                .expect("ブックマークが無い"),
        )
        .expect("解決できなかった");
    assert_eq!(resolved.path, expected);

    // 削除すると、2 回目の見回りで見つからない。
    fs::remove_dir(&renamed).expect("消せなかった");
    patrol.tick(&sessions);
    patrol.tick(&sessions);
    assert!(card_of(&sessions.snapshot(), 1).missing);
}

const CHECKS: &[(&str, fn())] = &[
    (
        "改名するとカードは同じプロジェクトのまま表示用のパスが新しくなる",
        改名するとカードは同じプロジェクトのまま表示用のパスが新しくなる,
    ),
    ("移動のあとも続けて追従する", 移動のあとも続けて追従する),
    (
        "削除すると見つからないになりカードは残る",
        削除すると見つからないになりカードは残る,
    ),
    (
        "改名と同時の一時的な失敗では見つからないにしない",
        改名と同時の一時的な失敗では見つからないにしない,
    ),
    ("ゴミ箱へ移すと見つからない", ゴミ箱へ移すと見つからない),
    (
        "ゴミ箱から戻すと見つからないが解ける",
        ゴミ箱から戻すと見つからないが解ける,
    ),
    ("変わらなければ写しを送らない", 変わらなければ写しを送らない),
    (
        "見つからないプロジェクトがブックマークで解決できると戻りタブは開かない",
        見つからないプロジェクトがブックマークで解決できると戻りタブは開かない,
    ),
    (
        "見つからないプロジェクトの元の場所にフォルダが戻ると見つからないが解ける",
        見つからないプロジェクトの元の場所にフォルダが戻ると見つからないが解ける,
    ),
    (
        "登録_削除_再リンクに合わせて開いているフォルダを張り替える",
        登録_削除_再リンクに合わせて開いているフォルダを張り替える,
    ),
    (
        "別のプロジェクトと同じ場所へ移ったときは追従しない",
        別のプロジェクトと同じ場所へ移ったときは追従しない,
    ),
    (
        "見回りのスレッドは間隔ごとに回り_止めるとすぐ終わる",
        見回りのスレッドは間隔ごとに回り_止めるとすぐ終わる,
    ),
    ("間隔が長くても止めるとすぐ終わる", 間隔が長くても止めるとすぐ終わる),
    (
        "実物のフォルダを改名するとカードの表示用のパスが新しくなる",
        実物のフォルダを改名するとカードの表示用のパスが新しくなる,
    ),
];

// ---------------------------------------------------------------------------
// ペインの寸法のコマンドと終了(模擬ランタイム)
// ---------------------------------------------------------------------------

fn snapshot(handle: &AppHandle<MockRuntime>, window: &WebviewWindow<MockRuntime>) -> AppSnapshot {
    get_ipc_response(window, request(handle, "get_app_snapshot"))
        .unwrap_or_else(|e| panic!("get_app_snapshot が失敗した: {e}"))
        .deserialize()
        .unwrap_or_else(|e| panic!("AppSnapshot として読めなかった: {e}"))
}

fn call(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    cmd: &str,
    args: serde_json::Value,
) {
    get_ipc_response(window, request_with(handle, cmd, InvokeBody::Json(args)))
        .unwrap_or_else(|e| panic!("{cmd} が失敗した: {e}"));
}

/// 状態ファイルの `ui`。
fn saved_ui(state_dir: &Path) -> Option<serde_json::Value> {
    let text = fs::read_to_string(state_dir.join("state.json")).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value.get("ui").cloned()
}

fn 寸法を送ると保存が予約され_最小を下回る値は最小になる(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    state_dir: &Path,
) {
    call(
        handle,
        window,
        "set_pane_sizes",
        json!({ "sidebarWidth": 10.0, "cardAreaHeight": 333.0 }),
    );

    let layout = snapshot(handle, window).layout;
    assert_eq!(layout.sidebar_width, 180.0, "最小に直していない");
    assert_eq!(layout.card_area_height, 333.0);
    assert!(
        wait_until(|| saved_ui(state_dir).is_some_and(|ui| {
            ui["sidebarWidth"] == json!(180.0) && ui["cardAreaHeight"] == json!(333.0)
        })),
        "状態ファイルに寸法が書かれない: {:?}",
        saved_ui(state_dir)
    );
}

fn 折りたたみを送ると保存が予約され幅は保つ(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    state_dir: &Path,
) {
    call(
        handle,
        window,
        "set_sidebar_collapsed",
        json!({ "collapsed": true }),
    );

    let layout = snapshot(handle, window).layout;
    assert!(layout.sidebar_collapsed);
    assert_eq!(layout.sidebar_width, 180.0);
    assert!(
        wait_until(|| saved_ui(state_dir).is_some_and(|ui| ui["sidebarCollapsed"] == json!(true))),
        "状態ファイルに折りたたみが書かれない: {:?}",
        saved_ui(state_dir)
    );
}

type AppCheck = fn(&AppHandle<MockRuntime>, &WebviewWindow<MockRuntime>, &Path);

const APP_CHECKS: &[(&str, AppCheck)] = &[
    (
        "寸法を送ると保存が予約され_最小を下回る値は最小になる",
        寸法を送ると保存が予約され_最小を下回る値は最小になる,
    ),
    (
        "折りたたみを送ると保存が予約され幅は保つ",
        折りたたみを送ると保存が予約され幅は保つ,
    ),
];

fn run_app_checks() -> usize {
    let temp = support::TempDir::new("root-follow-app");
    let state_dir = temp.path().to_path_buf();
    let launch = support::launch_in(&temp, false);
    let app = arin_lib::builder_with_ports::<MockRuntime>(
        launch,
        Arc::new(NoPty),
        Arc::new(|| ShellDefaults {
            shell: PathBuf::from("/bin/zsh"),
            locale: None,
        }),
        PersistencePorts::native(state_dir.clone()),
    )
    .build(tauri::generate_context!())
    .unwrap_or_else(|e| panic!("組み立てに失敗した: {e}"));

    let passed = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&passed);
    let stopped = Arc::new(Mutex::new(None::<(bool, Duration)>));
    let seen_stop = Arc::clone(&stopped);
    app.run_return(move |handle, event| {
        if let RunEvent::Ready = event {
            let window = handle
                .get_webview_window("main")
                .expect("設定にあるウィンドウ main が作られなかった");
            assert!(
                handle.state::<PatrolThread>().is_running(),
                "見回りのスレッドが動いていない"
            );
            for (name, check) in APP_CHECKS {
                check(handle, &window, &state_dir);
                println!("test {name} ... ok");
                count.fetch_add(1, Ordering::SeqCst);
            }
            for window in handle.webview_windows().into_values() {
                window.destroy().expect("ウィンドウを閉じられなかった");
            }
        }
        let exiting = matches!(event, RunEvent::Exit);
        let started = Instant::now();
        arin_lib::lifecycle::on_run_event(handle, event);
        if exiting {
            *lock(&seen_stop) = Some((
                handle.state::<PatrolThread>().is_running(),
                started.elapsed(),
            ));
        }
    });
    let (running, took) = lock(&stopped).expect("終了のイベントが来なかった");
    assert!(!running, "終了しても見回りのスレッドが止まらない");
    assert!(
        took < Duration::from_secs(1),
        "終了の後片付けに時間がかかった: {took:?}"
    );
    println!("test 終了時に見回りのスレッドが止まる ... ok");
    passed.load(Ordering::SeqCst) + 1
}

fn main() {
    for (name, check) in CHECKS {
        check();
        println!("test {name} ... ok");
    }
    let app_passed = run_app_checks();
    assert_eq!(
        app_passed,
        APP_CHECKS.len() + 1,
        "起動の完了まで進まなかった"
    );
    let _ = io::Write::flush(&mut io::stdout());
    println!("test result: ok. {} passed", CHECKS.len() + app_passed);
}
