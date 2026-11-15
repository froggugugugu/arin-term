//! 状態ファイル(`state.json`)の起動時の読み込みと保存(計画書 T33a、FR-004・FR-056、設計書 §6.2・§7.1「起動」、決定 (d))。
//!
//! - 起動時(`load_startup`): 読む → `decode` → 読めなければ退避の名前へ改名して空で始め、知らせの 1 行を写しに載せる。
//!   改名に失敗したら、その回の起動では一度も書かない。読めたら各プロジェクトのブックマークを解決する
//! - 保存(`Persistence`): 状態が変わったら `mark_changed`。0.5 秒まとめて、ウィンドウが前面でなくなったとき(`flush`)と
//!   終了時(`shutdown`)は待たずに書く。書き込みは専用のスレッド 1 本で直列に行い、前回書いた中身と同じなら書かない
//! - 失敗しても落ちない。標準エラーに `STATE_LOG_PREFIX` で 1 行出す
//!
//! 規則(読んだ結果の解釈、退避の名前、まとめの時間)は `arin_core::persistence` にあり、ここは順番とスレッドだけを持つ。

use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};

use arin_core::persistence::codec::{
    LoadOutcome, LoadedState, PersistedState, QuarantineReason, broken_file_name, decode, encode,
};
use arin_core::persistence::scheduler::{SaveDecision, SaveScheduler};
use arin_core::ports::clock::Clock;
use arin_core::ports::state_file::{StateFileError, StateFilePort, StateFileRead};
use arin_core::ports::{BookmarkPort, RootError, RootLocatorPort};
use arin_core::project::{Project, RootReference};
use arin_core::workspace::{RootResolution, Startup};
use arin_platform::bookmark::MacBookmarks;
use arin_platform::storage::FsStateFile;
use arin_platform::system::SystemClock;
use arin_platform::watch::FsRootLocator;

/// 状態ファイルの読み書きの失敗を標準エラーに書くときの 1 行の頭。
pub const STATE_LOG_PREFIX: &str = "[arin:state]";

/// 退避の名前が埋まっていたときに名前を足して試し直す上限。
const MOVE_ASIDE_ATTEMPTS: usize = 100;

/// 状態の読み書きに使うポート。
#[derive(Clone)]
pub struct PersistencePorts {
    /// 状態フォルダの `state.json`。
    pub state_file: Arc<dyn StateFilePort>,
    /// ブックマークの作成と解決。
    pub bookmarks: Arc<dyn BookmarkPort>,
    /// ルートの正規化。
    pub roots: Arc<dyn RootLocatorPort>,
    /// 時計(まとめの時間と退避の名前の日時)。
    pub clock: Arc<dyn Clock>,
}

impl PersistencePorts {
    /// OS の実装で作る。`state_dir` は状態フォルダ(`AppPaths::state_dir`)。
    #[must_use]
    pub fn native(state_dir: PathBuf) -> Self {
        Self {
            state_file: Arc::new(FsStateFile::new(state_dir)),
            bookmarks: Arc::new(MacBookmarks::new()),
            roots: Arc::new(FsRootLocator::new()),
            clock: Arc::new(SystemClock::new()),
        }
    }
}

/// 起動時に読んだ結果。
#[derive(Debug)]
pub struct StartupLoad {
    /// `Workspace::restore` に渡すもの。
    pub startup: Startup,
    /// 保存の規則。退避や読み取りに失敗した回は書かない設定。
    pub scheduler: SaveScheduler,
    /// 読んだ内容と保存する内容が違う(ブックマークを作り直した、退避して空で始めた)ので、起動直後に保存を予約する。
    pub needs_save: bool,
}

/// 状態ファイルを読み、`Workspace::restore` に渡すものを作る(設計書 §7.1「起動」3・4)。
#[must_use]
pub fn load_startup(ports: &PersistencePorts) -> StartupLoad {
    let outcome = match ports.state_file.read() {
        Ok(StateFileRead::Missing) => decode(None),
        Ok(StateFileRead::Contents(bytes)) => decode(Some(&bytes)),
        Err(e) => {
            // 無い以外の理由で読めない(権限など)。中身を確かめられないので、上書きしないよう書かない。
            eprintln!(
                "{STATE_LOG_PREFIX} state.json を読めなかったので、空の状態で始め、この回は保存しない: {e}"
            );
            return empty(
                Some(NOTICE_UNREADABLE.to_owned()),
                SaveScheduler::disabled(),
                false,
            );
        }
    };
    match outcome {
        LoadOutcome::Absent => empty(None, SaveScheduler::new(), false),
        LoadOutcome::Quarantine(reason) => quarantine(ports, &reason),
        LoadOutcome::Loaded(loaded) => resolve_roots(ports, loaded),
    }
}

/// 退避して空で始めたときの知らせ。`{name}` は退避の名前。
fn notice_moved_aside(name: &str) -> String {
    format!(
        "保存していた状態を読めなかったため、空の状態で始めました(元のファイルは {name} に残しています)"
    )
}

/// 退避もできなかったときの知らせ。
const NOTICE_NOT_MOVED: &str = "保存していた状態を読めなかったため、空の状態で始めました。元のファイルを退避できなかったので、この回は状態を保存しません";

/// 読み取り自体に失敗したときの知らせ。
const NOTICE_UNREADABLE: &str =
    "保存していた状態を読めなかったため、空の状態で始めました。この回は状態を保存しません";

fn empty(notice: Option<String>, scheduler: SaveScheduler, needs_save: bool) -> StartupLoad {
    StartupLoad {
        startup: Startup {
            notice,
            ..Startup::default()
        },
        scheduler,
        needs_save,
    }
}

/// 読めない状態ファイルを退避の名前へ改名する(消さない・上書きしない)。同じ名前があれば `-2`、`-3` と足す。
fn quarantine(ports: &PersistencePorts, reason: &QuarantineReason) -> StartupLoad {
    let seconds = ports.clock.since_unix_epoch().as_secs();
    let mut taken: Vec<String> = Vec::new();
    let mut last_error = None;
    for _ in 0..MOVE_ASIDE_ATTEMPTS {
        let name = broken_file_name(seconds, &taken);
        match ports.state_file.move_aside(&name) {
            Ok(()) => {
                eprintln!("{STATE_LOG_PREFIX} {reason}。{name} に退避して空の状態で始める");
                // 空の状態を書いて、次の起動では退避したファイルを読みに行かない。
                return empty(Some(notice_moved_aside(&name)), SaveScheduler::new(), true);
            }
            Err(StateFileError::BackupExists(_)) => taken.push(name),
            Err(e) => {
                last_error = Some(e);
                break;
            }
        }
    }
    let detail = last_error.map_or_else(
        || "空いている退避の名前が無い".to_owned(),
        |e| e.to_string(),
    );
    eprintln!("{STATE_LOG_PREFIX} {reason}。退避できなかったので、この回は保存しない: {detail}");
    empty(
        Some(NOTICE_NOT_MOVED.to_owned()),
        SaveScheduler::disabled(),
        false,
    )
}

/// 読めた状態の各プロジェクトのルートを解決する。
fn resolve_roots(ports: &PersistencePorts, loaded: LoadedState) -> StartupLoad {
    let LoadedState {
        state,
        rebuild_bookmarks: _,
    } = loaded;
    let mut needs_save = false;
    let roots = state
        .projects
        .iter()
        .map(|project| {
            let (resolution, changed) = resolve_root(ports, project);
            needs_save |= changed;
            (project.id().clone(), resolution)
        })
        .collect();
    StartupLoad {
        startup: Startup {
            state,
            roots,
            notice: None,
        },
        scheduler: SaveScheduler::new(),
        needs_save,
    }
}

/// 1 つのプロジェクトのルートを解決する。2 つ目の値は、保存する内容が読んだものから変わったか。
///
/// - ブックマークがあれば解決し、返ったパスを正規化する。古くなっていれば作り直す
/// - ブックマークが null(【仮定】13)か壊れているときは、`lastKnownPath` を正規化してから作り直す
/// - フォルダが無ければ「見つからない」(保存値は読んだまま)
fn resolve_root(ports: &PersistencePorts, project: &Project) -> (RootResolution, bool) {
    let root = project.root();
    let saved_path = root.last_known_path();
    let from_bookmark = root.bookmark().map(|b| (b, ports.bookmarks.resolve(b)));
    let (path, keep) = match from_bookmark {
        Some((bookmark, Ok(resolved))) => match ports.roots.canonical_dir(&resolved.path) {
            Ok(path) => (path, (!resolved.stale).then(|| bookmark.to_owned())),
            Err(_) => return (RootResolution::Missing, false),
        },
        // 消えたフォルダはパスからは追わない(同じパスに別のフォルダがあっても別物として扱う)。
        Some((_, Err(RootError::NotFound(_)))) => return (RootResolution::Missing, false),
        // null・壊れている・それ以外で解決できない: 最後に分かっていたパスから作り直す。
        Some((_, Err(_))) | None => match ports.roots.canonical_dir(saved_path) {
            Ok(path) => (path, None),
            Err(_) => return (RootResolution::Missing, false),
        },
    };
    let bookmark = match keep {
        Some(bookmark) => Some(bookmark),
        // 作り直せなければ、元のブックマーク(古くても解決はできた)を残す。無ければ次の起動でもう一度作る。
        None => ports
            .bookmarks
            .create(&path)
            .ok()
            .or_else(|| root.bookmark().map(ToOwned::to_owned)),
    };
    let reference = RootReference::new(path, bookmark);
    let changed = &reference != root;
    (RootResolution::Found(reference), changed)
}

/// 保存する内容を取り出す関数。
pub type StateSource = Box<dyn Fn() -> PersistedState + Send>;

/// 保存の予約と書き込みのスレッド。Tauri の管理状態に登録する。複製しても同じものを指す。
#[derive(Clone)]
pub struct Persistence {
    inner: Arc<Shared>,
}

struct Shared {
    clock: Arc<dyn Clock>,
    state: Mutex<Control>,
    wake: Condvar,
    worker: Mutex<Option<JoinHandle<()>>>,
}

struct Control {
    scheduler: SaveScheduler,
    /// 待たずに書く(前面でなくなった・終了)。
    flush: bool,
    /// 書き込みのスレッドを終える(終了)。
    stopping: bool,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Persistence {
    /// 保存の規則と時計から作る。書き込みのスレッドは `start` で始める(それまでの予約は残る)。
    #[must_use]
    pub fn new(scheduler: SaveScheduler, clock: Arc<dyn Clock>) -> Self {
        Self {
            inner: Arc::new(Shared {
                clock,
                state: Mutex::new(Control {
                    scheduler,
                    flush: false,
                    stopping: false,
                }),
                wake: Condvar::new(),
                worker: Mutex::new(None),
            }),
        }
    }

    /// 書く設定か。
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        lock(&self.inner.state).scheduler.is_enabled()
    }

    /// 書き込みのスレッドを 1 本始める。書かない設定なら何もしない。2 回目以降も何もしない。
    ///
    /// - `writer`: 書き込み先
    /// - `source`: その時点の保存する内容を返す(状態の正本のロックを短く取る)
    /// - `baseline`: いまの状態ファイルの中身とみなす文字列。同じ中身なら書かない(`None` なら最初の予約で必ず書く)
    pub fn start(
        &self,
        writer: Arc<dyn StateFilePort>,
        source: StateSource,
        baseline: Option<String>,
    ) {
        if !self.is_enabled() {
            return;
        }
        let mut worker = lock(&self.inner.worker);
        if worker.is_some() {
            return;
        }
        let shared = Arc::clone(&self.inner);
        let spawned = thread::Builder::new()
            .name("arin-persistence".into())
            .spawn(move || run_worker(&shared, writer.as_ref(), &source, baseline));
        match spawned {
            Ok(handle) => *worker = Some(handle),
            Err(e) => eprintln!(
                "{STATE_LOG_PREFIX} 保存のスレッドを始められなかったので、この回は保存しない: {e}"
            ),
        }
    }

    /// 保存対象が変わった。0.5 秒まとめてから書く。
    pub fn mark_changed(&self) {
        let now = self.inner.clock.monotonic();
        lock(&self.inner.state).scheduler.mark_changed(now);
        self.inner.wake.notify_all();
    }

    /// ウィンドウが前面でなくなったとき。保存できていない変更があれば待たずに書く(書き終わるのは待たない)。
    pub fn flush(&self) {
        lock(&self.inner.state).flush = true;
        self.inner.wake.notify_all();
    }

    /// 終了時。保存できていない変更を書き終えるまで待ち、書き込みのスレッドを終える。2 回目以降は何もしない。
    pub fn shutdown(&self) {
        {
            let mut control = lock(&self.inner.state);
            control.flush = true;
            control.stopping = true;
        }
        self.inner.wake.notify_all();
        let handle = lock(&self.inner.worker).take();
        if let Some(handle) = handle
            && handle.join().is_err()
        {
            eprintln!("{STATE_LOG_PREFIX} 保存のスレッドが異常終了した");
        }
    }
}

/// 書き込みのスレッドの本体。予約を見て、書く時が来たら書き、`stopping` で抜ける。
fn run_worker(
    shared: &Shared,
    writer: &dyn StateFilePort,
    source: &StateSource,
    mut last: Option<String>,
) {
    let mut control = lock(&shared.state);
    loop {
        // 終了の処理中は、まとめの待ちを使わず残りをすぐ書く。
        let decision = if control.flush || control.stopping {
            control.flush = false;
            control.scheduler.flush()
        } else {
            control.scheduler.poll(shared.clock.monotonic())
        };
        match decision {
            SaveDecision::Save => {
                drop(control);
                let saved = save(writer, source, &mut last);
                control = lock(&shared.state);
                if saved {
                    control.scheduler.save_succeeded();
                } else {
                    control.scheduler.save_failed();
                    // 終了の処理中は試し直さない(失敗は標準エラーに出した)。
                    if control.stopping {
                        return;
                    }
                }
            }
            SaveDecision::Wait(_) | SaveDecision::Idle if control.stopping => return,
            SaveDecision::Wait(duration) => {
                control = shared
                    .wake
                    .wait_timeout(control, duration)
                    .unwrap_or_else(PoisonError::into_inner)
                    .0;
            }
            SaveDecision::Idle => {
                control = shared
                    .wake
                    .wait(control)
                    .unwrap_or_else(PoisonError::into_inner);
            }
        }
    }
}

/// その時点の内容を書く。前回と同じ中身なら書かない。書けた(または書く必要が無かった)ら真。
fn save(writer: &dyn StateFilePort, source: &StateSource, last: &mut Option<String>) -> bool {
    let state = source();
    let text = match encode(&state) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("{STATE_LOG_PREFIX} 状態を保存できなかった: {e}");
            return false;
        }
    };
    if last.as_deref() == Some(text.as_str()) {
        return true;
    }
    match writer.write(text.as_bytes()) {
        Ok(()) => {
            *last = Some(text);
            true
        }
        Err(e) => {
            eprintln!("{STATE_LOG_PREFIX} 状態を保存できなかった: {e}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::Duration;

    use arin_core::ports::{OpenRoot, ResolvedBookmark};
    use arin_core::project::{ProjectColor, ProjectId};

    use super::*;

    /// パスごとに決めた結果を返すブックマーク。作ると `new:<パス>` を返す。
    struct FakeBookmarks {
        resolve: Vec<(&'static str, Result<ResolvedBookmark, RootError>)>,
        create_fails: bool,
    }

    impl BookmarkPort for FakeBookmarks {
        fn create(&self, path: &Path) -> Result<String, RootError> {
            if self.create_fails {
                Err(RootError::Bookmark("作れない".into()))
            } else {
                Ok(format!("new:{}", path.display()))
            }
        }

        fn resolve(&self, bookmark: &str) -> Result<ResolvedBookmark, RootError> {
            self.resolve
                .iter()
                .find(|(b, _)| *b == bookmark)
                .map(|(_, r)| r.clone())
                .unwrap_or_else(|| Err(RootError::Bookmark("壊れている".into())))
        }
    }

    /// `/real` で始まるパスだけがあり、正規化すると `/canon` に置き換わる。
    struct FakeRoots;

    impl RootLocatorPort for FakeRoots {
        fn canonical_dir(&self, path: &Path) -> Result<PathBuf, RootError> {
            let text = path.to_string_lossy();
            text.strip_prefix("/real")
                .map(|rest| PathBuf::from(format!("/canon{rest}")))
                .ok_or_else(|| RootError::NotFound(text.into_owned()))
        }

        fn open(&self, _path: &Path) -> Result<Box<dyn OpenRoot>, RootError> {
            Err(RootError::Io("使わない".into()))
        }
    }

    /// 決めた順に結果を返す状態ファイル。
    struct FakeStateFile {
        read: Result<StateFileRead, StateFileError>,
        moves: Mutex<Vec<Result<(), StateFileError>>>,
        moved: Mutex<Vec<String>>,
    }

    impl StateFilePort for FakeStateFile {
        fn read(&self) -> Result<StateFileRead, StateFileError> {
            self.read.clone()
        }

        fn write(&self, _contents: &[u8]) -> Result<(), StateFileError> {
            Ok(())
        }

        fn move_aside(&self, name: &str) -> Result<(), StateFileError> {
            lock(&self.moved).push(name.to_owned());
            let mut moves = lock(&self.moves);
            if moves.is_empty() {
                Ok(())
            } else {
                moves.remove(0)
            }
        }
    }

    struct FixedClock;

    impl Clock for FixedClock {
        fn monotonic(&self) -> Duration {
            Duration::ZERO
        }

        fn since_unix_epoch(&self) -> Duration {
            // 2026-11-15T09:30:00Z
            Duration::from_secs(1_794_735_000)
        }
    }

    fn ports(bookmarks: FakeBookmarks, state_file: Arc<FakeStateFile>) -> PersistencePorts {
        PersistencePorts {
            state_file,
            bookmarks: Arc::new(bookmarks),
            roots: Arc::new(FakeRoots),
            clock: Arc::new(FixedClock),
        }
    }

    fn state_file(read: Result<StateFileRead, StateFileError>) -> Arc<FakeStateFile> {
        Arc::new(FakeStateFile {
            read,
            moves: Mutex::new(Vec::new()),
            moved: Mutex::new(Vec::new()),
        })
    }

    fn project(path: &str, bookmark: Option<&str>) -> Project {
        Project::restore(
            ProjectId::from_random_bytes([7; 16]),
            RootReference::new(PathBuf::from(path), bookmark.map(ToOwned::to_owned)),
            None,
            ProjectColor::Green,
        )
    }

    fn no_bookmarks() -> FakeBookmarks {
        FakeBookmarks {
            resolve: Vec::new(),
            create_fails: false,
        }
    }

    fn found(path: &str, bookmark: Option<&str>) -> RootResolution {
        RootResolution::Found(RootReference::new(
            PathBuf::from(path),
            bookmark.map(ToOwned::to_owned),
        ))
    }

    fn resolved(path: &str, stale: bool) -> Result<ResolvedBookmark, RootError> {
        Ok(ResolvedBookmark {
            path: PathBuf::from(path),
            stale,
        })
    }

    #[test]
    fn ブックマークが_null_なら正規化したパスから作り直し保存を予約する() {
        let p = ports(no_bookmarks(), state_file(Ok(StateFileRead::Missing)));
        let (resolution, changed) = resolve_root(&p, &project("/real/a", None));
        assert_eq!(resolution, found("/canon/a", Some("new:/canon/a")));
        assert!(changed);
    }

    #[test]
    fn 古くないブックマークはそのまま使い_パスが同じなら保存しない() {
        let bookmarks = FakeBookmarks {
            resolve: vec![("b1", resolved("/real/a", false))],
            create_fails: false,
        };
        let p = ports(bookmarks, state_file(Ok(StateFileRead::Missing)));
        let (resolution, changed) = resolve_root(&p, &project("/canon/a", Some("b1")));
        assert_eq!(resolution, found("/canon/a", Some("b1")));
        assert!(!changed);
    }

    #[test]
    fn 古くなったブックマークは新しいパスから作り直す() {
        let bookmarks = FakeBookmarks {
            resolve: vec![("b1", resolved("/real/moved", true))],
            create_fails: false,
        };
        let p = ports(bookmarks, state_file(Ok(StateFileRead::Missing)));
        let (resolution, changed) = resolve_root(&p, &project("/canon/a", Some("b1")));
        assert_eq!(resolution, found("/canon/moved", Some("new:/canon/moved")));
        assert!(changed);
    }

    #[test]
    fn 作り直せなければ古いブックマークを残す() {
        let bookmarks = FakeBookmarks {
            resolve: vec![("b1", resolved("/real/moved", true))],
            create_fails: true,
        };
        let p = ports(bookmarks, state_file(Ok(StateFileRead::Missing)));
        let (resolution, _) = resolve_root(&p, &project("/canon/a", Some("b1")));
        assert_eq!(resolution, found("/canon/moved", Some("b1")));
    }

    #[test]
    fn 壊れたブックマークは最後に分かっていたパスから作り直す() {
        let p = ports(no_bookmarks(), state_file(Ok(StateFileRead::Missing)));
        let (resolution, changed) = resolve_root(&p, &project("/real/a", Some("broken")));
        assert_eq!(resolution, found("/canon/a", Some("new:/canon/a")));
        assert!(changed);
    }

    #[test]
    fn 消えたフォルダとパスの無いフォルダは見つからない() {
        let bookmarks = FakeBookmarks {
            resolve: vec![("gone", Err(RootError::NotFound("消えた".into())))],
            create_fails: false,
        };
        let p = ports(bookmarks, state_file(Ok(StateFileRead::Missing)));
        // パスはあっても、ブックマークが「消えた」と言えばパスからは追わない。
        assert_eq!(
            resolve_root(&p, &project("/real/a", Some("gone"))),
            (RootResolution::Missing, false)
        );
        assert_eq!(
            resolve_root(&p, &project("/nowhere", None)),
            (RootResolution::Missing, false)
        );
    }

    #[test]
    fn 退避の名前が埋まっていれば名前を足して改名する() {
        let file = state_file(Ok(StateFileRead::Contents(b"{".to_vec())));
        lock(&file.moves).push(Err(StateFileError::BackupExists("x".into())));
        let p = ports(no_bookmarks(), Arc::clone(&file));
        let load = load_startup(&p);
        assert_eq!(
            *lock(&file.moved),
            vec![
                "state.json.broken-20261115T093000Z".to_owned(),
                "state.json.broken-20261115T093000Z-2".to_owned()
            ]
        );
        assert!(load.scheduler.is_enabled());
        assert!(load.needs_save);
        let notice = load.startup.notice.expect("知らせが無い");
        assert!(
            notice.contains("state.json.broken-20261115T093000Z-2"),
            "{notice}"
        );
    }

    #[test]
    fn 退避に失敗したら書かない設定で知らせを出す() {
        let file = state_file(Ok(StateFileRead::Contents(b"{".to_vec())));
        lock(&file.moves).push(Err(StateFileError::Io("権限が無い".into())));
        let load = load_startup(&ports(no_bookmarks(), file));
        assert!(!load.scheduler.is_enabled());
        assert!(!load.needs_save);
        assert_eq!(load.startup.notice.as_deref(), Some(NOTICE_NOT_MOVED));
        assert!(load.startup.state.projects.is_empty());
    }

    #[test]
    fn 読み取りに失敗したら書かない設定で空から始める() {
        let file = state_file(Err(StateFileError::Io("読めない".into())));
        let load = load_startup(&ports(no_bookmarks(), Arc::clone(&file)));
        assert!(!load.scheduler.is_enabled());
        assert_eq!(load.startup.notice.as_deref(), Some(NOTICE_UNREADABLE));
        assert!(lock(&file.moved).is_empty(), "読めないファイルを改名した");
    }

    #[test]
    fn 無ければ空で始め保存を予約しない() {
        let load = load_startup(&ports(
            no_bookmarks(),
            state_file(Ok(StateFileRead::Missing)),
        ));
        assert!(load.scheduler.is_enabled());
        assert!(!load.needs_save);
        assert_eq!(load.startup.notice, None);
        assert!(load.startup.state.projects.is_empty());
    }
}
