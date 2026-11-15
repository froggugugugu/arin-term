//! 起動時の状態ファイルの読み込み、保存、退避の知らせ(計画書 T33a、FR-004・FR-056、決定 (d))を、
//! Tauri の模擬ランタイムと一時の状態フォルダで確かめる。
//!
//! 状態ファイル・ブックマーク・ルートの調査は本物(`arin-platform`)を使い、すべて一時フォルダの中で行う。
//! 時計は、まとめの待ち(0.5 秒)を進めたくないテストだけ止まった偽物に差し替える。
//! アプリの終了は、模擬ランタイムがウィンドウを全部閉じたあとに送る `RunEvent::Exit` を `lifecycle::on_run_event` に渡して起こす。
//!
//! macOS ではメニューの部品をメインスレッドでしか作れないため、`harness = false` で走らせる(`app_builder` と同じ)。

mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use arin_core::persistence::codec::{LoadOutcome, decode};
use arin_core::ports::clock::Clock;
use arin_core::ports::state_file::{StateFileError, StateFilePort, StateFileRead};
use arin_core::ports::{PtyError, PtyPort, PtySize, SpawnedPty};
use arin_core::terminal::ShellLaunchSpec;
use arin_core::workspace::AppSnapshot;
use arin_lib::launch::LaunchConfig;
use arin_lib::persistence::PersistencePorts;
use arin_lib::sessions::{SessionRegistry, ShellDefaults};
use arin_platform::storage::FsStateFile;
use tauri::test::MockRuntime;
use tauri::{AppHandle, Manager, RunEvent};

/// 書かれるのを待つ上限。
const WAIT: Duration = Duration::from_secs(5);
/// 手で書いた状態ファイルのプロジェクトの ID。
const PROJECT_ID: &str = "3f2b8c1e-4d5a-4b6c-8d7e-9f0a1b2c3d4e";

/// シェルを起動しない PTY(このテストはターミナルに接続しない)。
struct NoPty;

impl PtyPort for NoPty {
    fn spawn(&self, _spec: &ShellLaunchSpec, _size: PtySize) -> Result<SpawnedPty, PtyError> {
        Err(PtyError::Spawn("このテストではシェルを起動しない".into()))
    }
}

/// 止まった時計。まとめの待ちが終わらないので、待たずに書く契機(終了・前面でなくなる)だけで書かれる。
struct FrozenClock;

impl Clock for FrozenClock {
    fn monotonic(&self) -> Duration {
        Duration::ZERO
    }

    fn since_unix_epoch(&self) -> Duration {
        // 2026-11-15T09:30:00Z
        Duration::from_secs(1_794_735_000)
    }
}

/// 退避(改名)が必ず失敗し、書いた回数を数える状態ファイル。読むのは本物。
struct NoMoveAside {
    inner: FsStateFile,
    writes: Arc<AtomicUsize>,
}

impl StateFilePort for NoMoveAside {
    fn read(&self) -> Result<StateFileRead, StateFileError> {
        self.inner.read()
    }

    fn write(&self, contents: &[u8]) -> Result<(), StateFileError> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        self.inner.write(contents)
    }

    fn move_aside(&self, _backup_name: &str) -> Result<(), StateFileError> {
        Err(StateFileError::Io("テストで退避を失敗させた".into()))
    }
}

/// 1 つのテストで使う一時フォルダ。状態フォルダ(`state`)とプロジェクトのフォルダ(`project`)を分けて置く。
struct Fixture {
    root: support::TempDir,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let root = support::TempDir::new(name);
        fs::create_dir(root.path().join("state")).expect("状態フォルダを作れなかった");
        fs::create_dir(root.path().join("project")).expect("プロジェクトのフォルダを作れなかった");
        Self { root }
    }

    fn state_dir(&self) -> PathBuf {
        self.root.path().join("state")
    }

    fn project_dir(&self) -> PathBuf {
        self.root.path().join("project")
    }

    fn state_file(&self) -> PathBuf {
        self.state_dir().join("state.json")
    }

    /// 状態フォルダの中の名前の一覧(並べ替え済み)。
    fn state_entries(&self) -> Vec<String> {
        entries(&self.state_dir())
    }

    /// `bookmark` が null のプロジェクトを 1 件持つ状態ファイルを置く(計画書 M2 の【仮定】13)。中身を返す。
    fn write_project_state(&self) -> Vec<u8> {
        let text = serde_json::to_string_pretty(&serde_json::json!({
            "schemaVersion": 1,
            "projects": [{
                "id": PROJECT_ID,
                "bookmark": null,
                "lastKnownPath": self.project_dir(),
                "customName": null,
                "colorId": "green"
            }],
            "ui": { "selectedProjectId": PROJECT_ID }
        }))
        .expect("JSON にできなかった");
        self.write_state(text.as_bytes())
    }

    fn write_state(&self, bytes: &[u8]) -> Vec<u8> {
        fs::write(self.state_file(), bytes).expect("状態ファイルを置けなかった");
        bytes.to_vec()
    }

    fn launch(&self) -> LaunchConfig {
        LaunchConfig::from_inputs(None, Some(self.state_dir().as_os_str()), None)
            .unwrap_or_else(|e| panic!("組み立ての設定を作れなかった: {e}"))
    }

    fn native_ports(&self) -> PersistencePorts {
        PersistencePorts::native(self.state_dir())
    }

    fn frozen_ports(&self) -> PersistencePorts {
        PersistencePorts {
            clock: Arc::new(FrozenClock),
            ..self.native_ports()
        }
    }
}

fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("フォルダを読めなかった")
        .map(|e| {
            e.expect("項目を読めなかった")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

/// 状態ファイルを読んで、プロジェクトのブックマーク(1 件目)を返す。読めない・無いときは失敗させる。
fn saved_projects(path: &Path) -> Vec<serde_json::Value> {
    let text = fs::read_to_string(path).expect("状態ファイルを読めなかった");
    let value: serde_json::Value = serde_json::from_str(&text).expect("状態ファイルが JSON でない");
    assert_eq!(value["schemaVersion"], 1, "{text}");
    value["projects"]
        .as_array()
        .cloned()
        .unwrap_or_else(|| panic!("projects が配列でない: {text}"))
}

/// 1 件目のプロジェクトにブックマークが入っているか。
fn has_bookmark(path: &Path) -> bool {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|v| v["projects"][0]["bookmark"].as_str().map(|s| !s.is_empty()))
        .unwrap_or(false)
}

fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !done() {
        assert!(Instant::now() < deadline, "{what}まで待てなかった");
        thread::sleep(Duration::from_millis(20));
    }
}

/// 組み立てて起動し、準備ができたら `on_ready` を呼んでからウィンドウを閉じ、終了(`RunEvent::Exit`)まで進める。
/// 終了の処理は本物と同じ `lifecycle::on_run_event` に渡す。
fn run_app(
    launch: LaunchConfig,
    ports: PersistencePorts,
    on_ready: impl FnOnce(&AppHandle<MockRuntime>) + 'static,
) {
    let app = arin_lib::builder_with_ports::<MockRuntime>(
        launch,
        Arc::new(NoPty),
        Arc::new(|| ShellDefaults {
            shell: PathBuf::from("/bin/zsh"),
            locale: None,
        }),
        ports,
    )
    .build(tauri::generate_context!())
    .unwrap_or_else(|e| panic!("組み立てに失敗した: {e}"));

    let mut on_ready = Some(on_ready);
    let exited = Arc::new(Mutex::new(false));
    let seen = Arc::clone(&exited);
    app.run_return(move |handle, event| {
        let is_exit = matches!(event, RunEvent::Exit);
        if let RunEvent::Ready = event
            && let Some(f) = on_ready.take()
        {
            f(handle);
            for window in handle.webview_windows().into_values() {
                window.destroy().expect("ウィンドウを閉じられなかった");
            }
        }
        arin_lib::lifecycle::on_run_event(handle, event);
        if is_exit {
            *seen.lock().unwrap_or_else(PoisonError::into_inner) = true;
        }
    });
    assert!(
        *exited.lock().unwrap_or_else(PoisonError::into_inner),
        "終了まで進まなかった"
    );
}

fn snapshot(handle: &AppHandle<MockRuntime>) -> AppSnapshot {
    handle.state::<SessionRegistry>().snapshot()
}

fn 状態ファイルが無ければ空で始まる() {
    let fx = Fixture::new("persist-absent");
    run_app(fx.launch(), fx.native_ports(), |handle| {
        let s = snapshot(handle);
        assert!(s.projects.is_empty(), "{s:?}");
        assert!(s.terminals.is_empty(), "{s:?}");
        assert_eq!(s.notice, None);
        arin_lib::lifecycle::on_window_focus_changed(handle, false);
    });
    assert_eq!(
        fx.state_entries(),
        Vec::<String>::new(),
        "変更が無いのに状態フォルダへ書いた"
    );
}

/// 読めない状態ファイルを置いて起動し、退避・知らせ・空の状態での保存を確かめる。
fn check_quarantine(name: &str, contents: &[u8]) {
    let fx = Fixture::new(name);
    let original = fx.write_state(contents);
    let state_dir = fx.state_dir();
    let expected = original.clone();
    run_app(fx.launch(), fx.native_ports(), move |handle| {
        let s = snapshot(handle);
        assert!(s.projects.is_empty(), "{s:?}");
        let notice = s.notice.expect("知らせが写しに載っていない");
        assert!(
            notice.contains("state.json.broken-"),
            "知らせに退避の名前が無い: {notice}"
        );
        let backups: Vec<String> = entries(&state_dir)
            .into_iter()
            .filter(|n| n.starts_with("state.json.broken-"))
            .collect();
        assert_eq!(backups.len(), 1, "退避のファイルが 1 つでない: {backups:?}");
        assert!(notice.contains(&backups[0]), "知らせの名前が違う: {notice}");
        assert_eq!(
            fs::read(state_dir.join(&backups[0])).expect("退避のファイルを読めなかった"),
            expected,
            "退避のファイルの中身が変わった"
        );
    });
    let names = fx.state_entries();
    assert_eq!(names.len(), 2, "{names:?}");
    assert!(names.contains(&"state.json".to_string()), "{names:?}");
    assert!(
        saved_projects(&fx.state_file()).is_empty(),
        "空の状態で書かれていない"
    );
    match decode(Some(&fs::read(fx.state_file()).expect("読めなかった"))) {
        LoadOutcome::Loaded(loaded) => assert!(loaded.state.projects.is_empty()),
        other => panic!("新しい状態ファイルを読めない: {other:?}"),
    }
}

fn 壊れた状態ファイルは退避され空で始まり知らせが写しに載る() {
    check_quarantine("persist-broken", b"{ \"schemaVersion\": 1, \"projects\": [");
}

fn 版が新しすぎるときも同じ() {
    check_quarantine(
        "persist-too-new",
        b"{ \"schemaVersion\": 99, \"projects\": [] }\n",
    );
}

fn 退避に失敗したらその回は書かない() {
    let fx = Fixture::new("persist-no-move");
    let original = fx.write_state(b"not json");
    let writes = Arc::new(AtomicUsize::new(0));
    let ports = PersistencePorts {
        state_file: Arc::new(NoMoveAside {
            inner: FsStateFile::new(fx.state_dir()),
            writes: Arc::clone(&writes),
        }),
        ..fx.native_ports()
    };
    run_app(fx.launch(), ports, |handle| {
        let s = snapshot(handle);
        assert!(s.projects.is_empty(), "{s:?}");
        let notice = s.notice.expect("知らせが写しに載っていない");
        assert!(notice.contains("保存しません"), "{notice}");
        arin_lib::lifecycle::on_window_focus_changed(handle, false);
        thread::sleep(Duration::from_millis(700));
    });
    assert_eq!(writes.load(Ordering::SeqCst), 0, "退避に失敗した回に書いた");
    assert_eq!(fx.state_entries(), vec!["state.json".to_string()]);
    assert_eq!(
        fs::read(fx.state_file()).expect("読めなかった"),
        original,
        "壊れたファイルが変わった"
    );
}

fn 変更から少したつと状態ファイルが書かれる() {
    let fx = Fixture::new("persist-debounce");
    fx.write_project_state();
    let state_file = fx.state_file();
    let project_dir = fx.project_dir();
    run_app(fx.launch(), fx.native_ports(), move |handle| {
        let s = snapshot(handle);
        assert_eq!(s.projects.len(), 1, "{s:?}");
        assert!(!s.projects[0].missing, "{s:?}");
        assert_eq!(
            s.terminals.len(),
            1,
            "解決できたプロジェクトに 1 タブ開いていない"
        );
        assert_eq!(s.notice, None);
        // 終了を待たずに、まとめの時間がたてば書かれる(ブックマークを作り直したので保存対象が変わった)。
        wait_until(
            "ブックマークが状態ファイルに書かれる",
            || has_bookmark(&state_file),
        );
        assert_eq!(entries(&project_dir), Vec::<String>::new());
    });
    let projects = saved_projects(&fx.state_file());
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0]["id"], PROJECT_ID);
    let canonical = fs::canonicalize(fx.project_dir()).expect("正規化できなかった");
    assert_eq!(
        projects[0]["lastKnownPath"],
        canonical.to_string_lossy().as_ref()
    );
    assert_eq!(
        entries(&fx.project_dir()),
        Vec::<String>::new(),
        "プロジェクトのフォルダに何か作った"
    );
    assert_eq!(fx.state_entries(), vec!["state.json".to_string()]);
}

fn 終了のときに書かれる() {
    let fx = Fixture::new("persist-exit");
    let original = fx.write_project_state();
    let state_file = fx.state_file();
    run_app(fx.launch(), fx.frozen_ports(), move |handle| {
        assert_eq!(snapshot(handle).projects.len(), 1);
        // 時計が止まっているので、まとめの待ちでは書かれない。
        thread::sleep(Duration::from_millis(700));
        assert_eq!(
            fs::read(&state_file).expect("読めなかった"),
            original,
            "終了の前に書いた"
        );
    });
    assert!(has_bookmark(&fx.state_file()), "終了のときに書かれていない");
}

fn ウィンドウが前面でなくなったら待たずに書かれる() {
    let fx = Fixture::new("persist-blur");
    fx.write_project_state();
    let state_file = fx.state_file();
    run_app(fx.launch(), fx.frozen_ports(), move |handle| {
        thread::sleep(Duration::from_millis(100));
        assert!(!has_bookmark(&state_file), "前面でなくなる前に書いた");
        arin_lib::lifecycle::on_window_focus_changed(handle, true);
        thread::sleep(Duration::from_millis(100));
        assert!(!has_bookmark(&state_file), "前面になったときに書いた");
        arin_lib::lifecycle::on_window_focus_changed(handle, false);
        wait_until("前面でなくなったときに書かれる", || {
            has_bookmark(&state_file)
        });
    });
}

fn 検証画面では読まず書かない() {
    let fx = Fixture::new("persist-verify");
    let original = fx.write_project_state();
    let launch = fx
        .launch()
        .with_launch_options(arin_core::launch_options::LaunchOptions {
            verify_grid: true,
            ..arin_core::launch_options::LaunchOptions::default()
        });
    run_app(launch, fx.frozen_ports(), |handle| {
        let s = snapshot(handle);
        assert!(
            s.projects.is_empty(),
            "検証画面で状態ファイルを読んだ: {s:?}"
        );
        assert_eq!(s.terminals.len(), 9);
        assert_eq!(s.notice, None);
        arin_lib::lifecycle::on_window_focus_changed(handle, false);
        thread::sleep(Duration::from_millis(200));
    });
    assert_eq!(fx.state_entries(), vec!["state.json".to_string()]);
    assert_eq!(
        fs::read(fx.state_file()).expect("読めなかった"),
        original,
        "検証画面で状態ファイルを書いた"
    );
}

fn main() {
    let tests: [(&str, fn()); 8] = [
        (
            "状態ファイルが無ければ空で始まる",
            状態ファイルが無ければ空で始まる,
        ),
        (
            "壊れた状態ファイルは退避され空で始まり知らせが写しに載る",
            壊れた状態ファイルは退避され空で始まり知らせが写しに載る,
        ),
        ("版が新しすぎるときも同じ", 版が新しすぎるときも同じ),
        (
            "退避に失敗したらその回は書かない",
            退避に失敗したらその回は書かない,
        ),
        (
            "変更から少したつと状態ファイルが書かれる",
            変更から少したつと状態ファイルが書かれる,
        ),
        ("終了のときに書かれる", 終了のときに書かれる),
        (
            "ウィンドウが前面でなくなったら待たずに書かれる",
            ウィンドウが前面でなくなったら待たずに書かれる,
        ),
        ("検証画面では読まず書かない", 検証画面では読まず書かない),
    ];
    for (name, test) in tests {
        test();
        println!("test {name} ... ok");
    }
}
