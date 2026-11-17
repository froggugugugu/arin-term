//! 設定ファイルの保存の検知と即時の反映(`src-tauri/src/settings_watch.rs`、計画書 T65c、ADR-012 の 5、
//! AC-K03・AC-K05(動作中の構文エラー)・AC-K07 の Rust 側)。
//!
//! - 監視のポートは偽物(`FakeWatcher`。張った場所と、捨てられたかを記録し、受け口へ束を流せる)。
//!   実物(`FsEventsWatcher`)は一時フォルダで 1 件だけ(エディタの保存の形=一時ファイルに書いて改名)
//! - 読み直しの手順(表の差し替え・メニューの作り直し・`settings-changed`)は Tauri の模擬ランタイムで確かめる。
//!   メニューの作り直しはメインスレッド(模擬のループ)で動くので、確かめる手順は別のスレッドで動かし、
//!   終わったらウィンドウを閉じて終了まで進める
//! - 状態フォルダは一時フォルダ(`canonicalize` した実パス)に差し替え、本物の設定ファイルを読まない。
//!   設定ファイルはテストが書くだけで、アプリは作らない・書かない
//!
//! macOS ではメニューの部品をメインスレッドでしか作れないため、`harness = false` で走らせる(`app_builder` と同じ)。

mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use arin_core::paths::AppPaths;
use arin_core::ports::watch::{ActiveWatch, WatchBatch, WatchError, WatchSink, WatcherPort};
use arin_core::ports::{PtyError, PtyPort, PtySize, SpawnedPty};
use arin_core::shortcuts::table::{ShortcutCommandId, ShortcutTable};
use arin_core::terminal::ShellLaunchSpec;
use arin_lib::TreePorts;
use arin_lib::events::SETTINGS_CHANGED_EVENT;
use arin_lib::menu::{self, Shortcuts};
use arin_lib::persistence::PersistencePorts;
use arin_lib::projects::ProjectPorts;
use arin_lib::sessions::ShellDefaults;
use arin_lib::settings::AppSettings;
use arin_lib::settings_watch::SettingsWatch;
use arin_platform::desktop::{MacDesktop, MacFilePasteboard};
use arin_platform::fs::LocalFileSystem;
use arin_platform::watch::FsEventsWatcher;
use tauri::test::MockRuntime;
use tauri::{AppHandle, Listener, Manager, RunEvent};

use ShortcutCommandId as C;

/// 状態フォルダの中の設定ファイルの名前。
const SETTINGS: &str = "settings.toml";
/// どの待ちにも付ける上限。
const WAIT: Duration = Duration::from_secs(10);
/// 「来ない」ことを確かめるときに待つ時間(模擬のループは 1 回に 1 秒眠るので、それより長く待つ)。
const QUIET: Duration = Duration::from_millis(1500);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
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
// 偽の監視
// ---------------------------------------------------------------------------

struct Watched {
    root: PathBuf,
    sink: Arc<WatchSink>,
    alive: Arc<AtomicBool>,
}

/// 張った場所を記録する監視。`fail` が真なら張れない。
#[derive(Default)]
struct FakeWatcher {
    watches: Mutex<Vec<Watched>>,
    fail: AtomicBool,
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
        if self.fail.load(Ordering::SeqCst) {
            return Err(WatchError(format!("{}: 張れない(テスト)", root.display())));
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

    /// いま張っている監視の場所。張るまで待つ。
    fn wait_root(&self) -> PathBuf {
        assert!(
            wait_until(|| !self.active().is_empty()),
            "状態フォルダに監視を張らなかった"
        );
        self.active().remove(0)
    }

    /// いま張っている監視の受け口へ、監視の場所の下の `names` の束を流す(本物の監視のスレッドの代わり)。
    fn push_names(&self, names: &[&str]) {
        let (root, sink) = {
            let watches = lock(&self.watches);
            let w = watches
                .iter()
                .rev()
                .find(|w| w.alive.load(Ordering::SeqCst))
                .expect("張っている監視が無い");
            (w.root.clone(), Arc::clone(&w.sink))
        };
        sink(WatchBatch {
            paths: names.iter().map(|n| root.join(n)).collect(),
            rescan: false,
        });
    }
}

/// どこにも張らない監視(ツリーの監視用。このテストはプロジェクトを持たない)。
struct NoWatcher;

impl WatcherPort for NoWatcher {
    fn watch(&self, _root: &Path, _sink: WatchSink) -> Result<Box<dyn ActiveWatch>, WatchError> {
        Err(WatchError("このテストではツリーを監視しない".into()))
    }
}

/// シェルを起動しない PTY(このテストはターミナルに接続しない)。
struct NoPty;

impl PtyPort for NoPty {
    fn spawn(&self, _spec: &ShellLaunchSpec, _size: PtySize) -> Result<SpawnedPty, PtyError> {
        Err(PtyError::Spawn("このテストではシェルを起動しない".into()))
    }
}

// ---------------------------------------------------------------------------
// 模擬のアプリ
// ---------------------------------------------------------------------------

/// 一時の状態フォルダ(実パス)。
struct StateDir {
    _temp: support::TempDir,
    real: PathBuf,
}

impl StateDir {
    fn new(name: &str) -> Self {
        let temp = support::TempDir::new(name);
        let real = fs::canonicalize(temp.path()).expect("実パスにできなかった");
        Self { _temp: temp, real }
    }

    fn path(&self) -> &Path {
        &self.real
    }

    fn write(&self, text: &str) {
        fs::write(self.real.join(SETTINGS), text).expect("設定ファイルを置けなかった");
    }
}

/// 確かめる手順に渡すもの。
struct Ctx {
    handle: AppHandle<MockRuntime>,
    watcher: Arc<FakeWatcher>,
    dir: PathBuf,
    /// 届いた `settings-changed` の中身。
    events: Arc<Mutex<Vec<String>>>,
}

impl Ctx {
    /// 設定ファイルを丸ごと置き換える(一時ファイルに書いて改名する)。前の知らせの読み直しが、書きかけの
    /// (空や途中までの)ファイルを読まないように。
    fn write(&self, text: &str) {
        let tmp = self.dir.join(".settings.toml.writing");
        fs::write(&tmp, text).expect("一時ファイルを書けなかった");
        fs::rename(&tmp, self.dir.join(SETTINGS)).expect("設定ファイルを置き換えられなかった");
    }

    fn table(&self) -> ShortcutTable {
        self.handle.state::<Shortcuts>().table()
    }

    fn notice(&self) -> Option<String> {
        self.handle.state::<AppSettings>().notice()
    }

    fn event_count(&self) -> usize {
        lock(&self.events).len()
    }

    /// `settings-changed` が `n` 件になるまで待つ。
    fn wait_events(&self, n: usize) {
        assert!(
            wait_until(|| self.event_count() >= n),
            "settings-changed が {n} 件届かない(届いたのは {} 件)",
            self.event_count()
        );
    }

    /// しばらく待っても `settings-changed` が `n` 件のままか。
    fn assert_quiet(&self, n: usize) {
        thread::sleep(QUIET);
        assert_eq!(
            self.event_count(),
            n,
            "送らなくてよい settings-changed が届いた"
        );
    }

    fn menu_id(&self) -> String {
        self.handle
            .menu()
            .expect("アプリにメニューが付いていない")
            .id()
            .0
            .clone()
    }
}

/// 状態フォルダ `dir` で、設定の監視を偽物にした模擬のアプリを起動し、準備ができたら別のスレッドで `check` を動かす。
/// `check` が終わったら(失敗しても)ウィンドウを閉じて終了まで進め、終了のあとに `on_exit` を呼ぶ。
fn run_watching(
    dir: &StateDir,
    check: impl FnOnce(&Ctx) + Send + 'static,
    on_exit: impl FnOnce(&AppHandle<MockRuntime>, &FakeWatcher) + 'static,
) {
    let watcher = Arc::new(FakeWatcher::default());
    let launch =
        arin_lib::launch::LaunchConfig::from_inputs(None, Some(dir.path().as_os_str()), None)
            .unwrap_or_else(|e| panic!("組み立ての設定を作れなかった: {e}"));
    let app = arin_lib::builder_with_settings_watcher::<MockRuntime>(
        launch,
        Arc::new(NoPty),
        Arc::new(|| ShellDefaults {
            shell: PathBuf::from("/bin/zsh"),
            locale: None,
        }),
        PersistencePorts::native(dir.path().to_path_buf()),
        ProjectPorts::native(),
        TreePorts {
            fs: Arc::new(LocalFileSystem::new()),
            watcher: Arc::new(NoWatcher),
            file_pasteboard: Arc::new(MacFilePasteboard::general()),
        },
        Arc::new(MacDesktop::new()),
        Arc::clone(&watcher) as Arc<dyn WatcherPort>,
    )
    .build(tauri::generate_context!())
    .unwrap_or_else(|e| panic!("組み立てに失敗した: {e}"));

    let outcome: Arc<Mutex<Option<thread::Result<()>>>> = Arc::new(Mutex::new(None));
    let exited = Arc::new(AtomicBool::new(false));
    let mut check = Some(check);
    let mut on_exit = Some(on_exit);
    let in_loop = Arc::clone(&watcher);
    let (seen_outcome, seen_exit) = (Arc::clone(&outcome), Arc::clone(&exited));
    let real = dir.path().to_path_buf();
    app.run_return(move |handle, event| {
        if let RunEvent::Ready = event
            && let Some(check) = check.take()
        {
            let events = Arc::new(Mutex::new(Vec::new()));
            let seen = Arc::clone(&events);
            handle.listen_any(SETTINGS_CHANGED_EVENT, move |event| {
                lock(&seen).push(event.payload().to_owned());
            });
            let ctx = Ctx {
                handle: handle.clone(),
                watcher: Arc::clone(&in_loop),
                dir: real.clone(),
                events,
            };
            let result = Arc::clone(&seen_outcome);
            thread::spawn(move || {
                let got = panic::catch_unwind(AssertUnwindSafe(|| check(&ctx)));
                *lock(&result) = Some(got);
                arin_lib::lifecycle::quit_now(&ctx.handle);
            });
        }
        let exiting = matches!(event, RunEvent::Exit);
        arin_lib::lifecycle::on_run_event(handle, event);
        if exiting {
            if let Some(f) = on_exit.take() {
                f(handle, &in_loop);
            }
            seen_exit.store(true, Ordering::SeqCst);
        }
    });
    assert!(exited.load(Ordering::SeqCst), "終了まで進まなかった");
    match lock(&outcome).take() {
        Some(Ok(())) => {}
        Some(Err(e)) => panic::resume_unwind(e),
        None => panic!("確かめる手順が終わらなかった"),
    }
}

// ---------------------------------------------------------------------------
// テスト
// ---------------------------------------------------------------------------

fn settings_toml_の変更でキーが替わり_settings_changed_が届く() {
    let dir = StateDir::new("settings-watch-change");
    run_watching(
        &dir,
        |ctx| {
            let root = ctx.watcher.wait_root();
            assert_eq!(root, ctx.dir, "状態フォルダの実パスに張っていない");
            assert_eq!(ctx.table(), ShortcutTable::default_table());
            let before_menu = ctx.menu_id();

            ctx.write("[shortcuts]\n\"terminal.new-tab\" = \"Cmd+Shift+T\"\n");
            ctx.watcher.push_names(&[SETTINGS]);
            ctx.wait_events(1);

            // 中身は無い(画面は表と知らせを取り直す。T65e)。
            assert_eq!(lock(&ctx.events)[0], "null");
            let table = ctx.table();
            assert_eq!(
                menu::accelerator(&table, C::TerminalNewTab).as_deref(),
                Some("Cmd+Shift+T")
            );
            // 書かなかった行は既定のまま(AC-K03)。
            assert_eq!(
                menu::accelerator(&table, C::TerminalCloseTab).as_deref(),
                Some("Cmd+W")
            );
            assert_eq!(ctx.notice(), None);
            // メニューはメインスレッドで作り直される(項目ごとではなく丸ごと差し替え。ADR-012 の 5)。
            assert!(
                wait_until(|| ctx.menu_id() != before_menu),
                "メニューが作り直されない"
            );
        },
        |_, _| {},
    );
    // 設定ファイルはテストが書いたまま(アプリは書き換えない)。
    assert_eq!(
        fs::read_to_string(dir.path().join(SETTINGS)).expect("設定ファイルが無い"),
        "[shortcuts]\n\"terminal.new-tab\" = \"Cmd+Shift+T\"\n"
    );
}

fn state_json_の変更では読み直さない() {
    let dir = StateDir::new("settings-watch-state-json");
    run_watching(
        &dir,
        |ctx| {
            ctx.watcher.wait_root();
            // 設定ファイルは変わっているが、届いた名前は状態ファイル(と、ほかのフォルダの同じ名前)だけ。
            ctx.write("[shortcuts]\n\"terminal.new-tab\" = \"Cmd+Shift+T\"\n");
            ctx.watcher
                .push_names(&["state.json", "state.json.tmp", "sub/settings.toml"]);
            ctx.assert_quiet(0);
            assert_eq!(ctx.table(), ShortcutTable::default_table());

            // 名前が settings.toml の知らせなら読み直す(同じ監視が生きている)。
            ctx.watcher.push_names(&[SETTINGS]);
            ctx.wait_events(1);
            assert_eq!(
                menu::accelerator(&ctx.table(), C::TerminalNewTab).as_deref(),
                Some("Cmd+Shift+T")
            );
        },
        |_, _| {},
    );
}

fn 同じ内容で保存し直しても送らない() {
    let dir = StateDir::new("settings-watch-same");
    dir.write("[shortcuts]\n\"terminal.new-tab\" = \"Cmd+Shift+T\"\n");
    run_watching(
        &dir,
        |ctx| {
            ctx.watcher.wait_root();
            let before_table = ctx.table();
            let before_menu = ctx.menu_id();
            // 同じ内容で保存し直す。書き方が違っても同じキーなら同じ表(`shift+cmd+t`)。
            ctx.write("[shortcuts]\n\"terminal.new-tab\" = \"Cmd+Shift+T\"\n");
            ctx.watcher.push_names(&[SETTINGS]);
            ctx.write(
                "# コメントだけ足した\n[shortcuts]\n\"terminal.new-tab\" = \"shift+cmd+t\"\n",
            );
            ctx.watcher.push_names(&[SETTINGS]);
            ctx.assert_quiet(0);
            assert_eq!(ctx.table(), before_table);
            assert_eq!(ctx.menu_id(), before_menu, "同じ表でメニューを作り直した");
        },
        |_, _| {},
    );
}

fn 構文エラーに直すと直前の表のまま知らせだけが替わる() {
    let dir = StateDir::new("settings-watch-syntax");
    dir.write("[shortcuts]\n\"terminal.new-tab\" = \"Cmd+Shift+T\"\n");
    run_watching(
        &dir,
        |ctx| {
            ctx.watcher.wait_root();
            let before_table = ctx.table();
            assert_eq!(ctx.notice(), None);
            let before_menu = ctx.menu_id();

            // 構文エラー(3 行目の値が無い)。読めた行も使わず、直前の表のまま(AC-K05)。
            ctx.write(
                "[shortcuts]\n\"terminal.new-tab\" = \"Cmd+Alt+T\"\n\"terminal.close-tab\" = \n",
            );
            ctx.watcher.push_names(&[SETTINGS]);
            // 知らせだけが替わったときも送る(画面が知らせを取り直すため)。
            ctx.wait_events(1);
            assert_eq!(ctx.table(), before_table, "構文エラーで表が替わった");
            let notice = ctx.notice().expect("知らせが無い");
            assert!(
                notice.contains("3 行") && notice.contains("列目"),
                "行と列が無い: {notice:?}"
            );
            assert!(!notice.contains("Cmd+Alt+T"), "中身が出た: {notice:?}");
            thread::sleep(QUIET);
            assert_eq!(
                ctx.menu_id(),
                before_menu,
                "表が同じなのにメニューを作り直した"
            );

            // 直すと新しい内容になり、知らせが消える。
            ctx.write("[shortcuts]\n\"terminal.new-tab\" = \"Cmd+Alt+T\"\n");
            ctx.watcher.push_names(&[SETTINGS]);
            ctx.wait_events(2);
            assert_eq!(
                menu::accelerator(&ctx.table(), C::TerminalNewTab).as_deref(),
                Some("Cmd+Alt+T")
            );
            assert_eq!(ctx.notice(), None);
        },
        |_, _| {},
    );
}

fn ファイルを消すと既定の表に戻る() {
    let dir = StateDir::new("settings-watch-removed");
    dir.write("[shortcuts]\n\"terminal.new-tab\" = \"Cmd+Shift+T\"\n\"no.such\" = \"Cmd+J\"\n");
    run_watching(
        &dir,
        |ctx| {
            ctx.watcher.wait_root();
            assert_ne!(ctx.table(), ShortcutTable::default_table());
            assert!(ctx.notice().is_some(), "知らないコマンドの知らせが無い");

            fs::remove_file(ctx.dir.join(SETTINGS)).expect("設定ファイルを消せなかった");
            ctx.watcher.push_names(&[SETTINGS]);
            ctx.wait_events(1);
            assert_eq!(ctx.table(), ShortcutTable::default_table());
            assert_eq!(ctx.notice(), None, "ファイルが無いのに知らせがある");
        },
        |_, _| {},
    );
    assert!(
        !dir.path().join(SETTINGS).exists(),
        "消した設定ファイルを作り直した"
    );
}

fn 状態フォルダが無ければ_0700_で作ってから張り設定ファイルは作らない() {
    let temp = StateDir::new("settings-watch-mkdir");
    let state_dir = temp.path().join("Arin");
    let watcher = Arc::new(FakeWatcher::default());
    let watch = SettingsWatch::spawn(
        state_dir.clone(),
        Arc::clone(&watcher) as Arc<dyn WatcherPort>,
        Arc::new(|| {}),
    )
    .expect("監視のスレッドを起動できなかった");
    let root = watcher.wait_root();
    assert_eq!(root, state_dir, "状態フォルダに張っていない");
    let meta = fs::metadata(&state_dir).expect("状態フォルダが無い");
    assert!(meta.is_dir());
    assert_eq!(meta.permissions().mode() & 0o777, 0o700);
    let entries: Vec<_> = fs::read_dir(&state_dir)
        .expect("状態フォルダを読めなかった")
        .collect();
    assert!(entries.is_empty(), "状態フォルダに何か作った: {entries:?}");
    watch.stop();
    assert!(watcher.active().is_empty(), "止めても監視が残っている");
}

fn 張れないときは落とさず止められる() {
    let temp = StateDir::new("settings-watch-fail");
    let watcher = Arc::new(FakeWatcher::default());
    watcher.fail.store(true, Ordering::SeqCst);
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let watch = SettingsWatch::spawn(
        temp.path().to_path_buf(),
        Arc::clone(&watcher) as Arc<dyn WatcherPort>,
        Arc::new(move || {
            counted.fetch_add(1, Ordering::SeqCst);
        }),
    )
    .expect("監視のスレッドを起動できなかった");
    thread::sleep(Duration::from_millis(200));
    assert_eq!(watcher.count(), 0);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    watch.stop();
    assert!(!watch.is_running(), "止めてもスレッドが残っている");
    watch.stop(); // 2 回目は何もしない
}

fn 終了のときにスレッドが抜ける() {
    let dir = StateDir::new("settings-watch-exit");
    let running_at_ready = Arc::new(AtomicBool::new(false));
    let seen = Arc::clone(&running_at_ready);
    run_watching(
        &dir,
        move |ctx| {
            ctx.watcher.wait_root();
            seen.store(
                ctx.handle.state::<SettingsWatch>().is_running(),
                Ordering::SeqCst,
            );
        },
        |handle, watcher| {
            assert!(
                !handle.state::<SettingsWatch>().is_running(),
                "終了しても設定の監視のスレッドが止まらない"
            );
            assert!(watcher.active().is_empty(), "終了しても監視が残っている");
            assert_eq!(watcher.count(), 1);
        },
    );
    assert!(
        running_at_ready.load(Ordering::SeqCst),
        "起動中に設定の監視のスレッドが動いていない"
    );
}

fn 一時ファイルに書いて改名して保存しても読み直す() {
    let dir = StateDir::new("settings-watch-fsevents");
    let paths = AppPaths::resolve(None, Some(dir.path().as_os_str()))
        .unwrap_or_else(|e| panic!("置き場所を決められなかった: {e}"));
    let (settings, _) = AppSettings::load(&paths);
    let settings = Arc::new(settings);
    let tables: Arc<Mutex<Vec<ShortcutTable>>> = Arc::new(Mutex::new(Vec::new()));
    let (reloader, seen) = (Arc::clone(&settings), Arc::clone(&tables));
    let watch = SettingsWatch::spawn(
        dir.path().to_path_buf(),
        Arc::new(FsEventsWatcher::new()),
        Arc::new(move || {
            lock(&seen).push(reloader.reload().table);
        }),
    )
    .expect("監視のスレッドを起動できなかった");
    // エディタの保存の形: 同じフォルダの一時ファイルに書いてから、設定ファイルの名前へ改名する。
    // FSEvents の流れが動き出す前の変更は届かない(立ち上がりの時間は Mac の状態で変わる)ので、
    // 届くまで同じ保存を繰り返す。
    let tmp = dir.path().join(".settings.toml.swp");
    let seen_new_key = || {
        lock(&tables)
            .iter()
            .any(|t| menu::accelerator(t, C::TerminalNewTab).as_deref() == Some("Cmd+Shift+T"))
    };
    let deadline = Instant::now() + WAIT;
    let mut saved = Instant::now();
    let mut reloaded = false;
    while !reloaded && Instant::now() < deadline {
        fs::write(
            &tmp,
            "[shortcuts]\n\"terminal.new-tab\" = \"Cmd+Shift+T\"\n",
        )
        .expect("一時ファイルを書けなかった");
        saved = Instant::now();
        fs::rename(&tmp, dir.path().join(SETTINGS)).expect("改名できなかった");
        let retry_at = saved + Duration::from_millis(500);
        while !reloaded && Instant::now() < retry_at {
            reloaded = seen_new_key();
            thread::sleep(Duration::from_millis(10));
        }
    }
    let took = saved.elapsed();
    watch.stop();
    assert!(reloaded, "改名での保存で読み直さなかった");
    println!("    (読み直しまで {took:?})");
    assert!(!watch.is_running());
}

fn main() {
    let tests: [(&str, fn()); 9] = [
        (
            "settings_toml_の変更でキーが替わり_settings_changed_が届く",
            settings_toml_の変更でキーが替わり_settings_changed_が届く,
        ),
        ("state_json_の変更では読み直さない", state_json_の変更では読み直さない),
        ("同じ内容で保存し直しても送らない", 同じ内容で保存し直しても送らない),
        (
            "構文エラーに直すと直前の表のまま知らせだけが替わる",
            構文エラーに直すと直前の表のまま知らせだけが替わる,
        ),
        ("ファイルを消すと既定の表に戻る", ファイルを消すと既定の表に戻る),
        (
            "状態フォルダが無ければ_0700_で作ってから張り設定ファイルは作らない",
            状態フォルダが無ければ_0700_で作ってから張り設定ファイルは作らない,
        ),
        ("張れないときは落とさず止められる", 張れないときは落とさず止められる),
        ("終了のときにスレッドが抜ける", 終了のときにスレッドが抜ける),
        (
            "一時ファイルに書いて改名して保存しても読み直す",
            一時ファイルに書いて改名して保存しても読み直す,
        ),
    ];
    for (name, test) in tests {
        test();
        println!("test {name} ... ok");
    }
    println!("test result: ok. {} passed", tests.len());
}
