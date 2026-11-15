//! 表示のコマンド(`src-tauri/src/commands/layout.rs` の `set_display_mode`・`focus_terminal`・`next_tile_page`・
//! `previous_tile_page`・`toggle_tile_maximize`。計画書 T47b)。FR-048(切替と保存)、FR-052(ページ送り)、
//! FR-053・AC-M15(タイルへのフォーカスでプロジェクトが選ばれ、ツリーの監視が張り替わる)、FR-054(拡大)。
//!
//! - 状態フォルダに alpha・beta の 2 プロジェクト(alpha を選択)の `state.json` を置いて起動する
//!   (起動時に各プロジェクトに 1 枚ずつタブができる)。プロジェクトのフォルダは一時フォルダの中に作る
//! - 監視のポートは偽物(`FakeWatcher`。張った場所と、捨てられたかを記録する)。PTY はシェルを起動しない
//! - 写しを送った回数は `app-snapshot` の受け口で数える(状態が変わったときだけ送ることを確かめる)
//! - 1 回の起動の中で、確認を順に行う(前の確認の状態を引き継ぐ。各確認の頭に前提を書く)
//!
//! macOS ではメニューの部品をメインスレッドでしか作れないため、`harness = false` で走らせる(`app_builder` と同じ)。

mod support;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use arin_core::layout::tiles::DisplayMode;
use arin_core::ports::watch::{ActiveWatch, WatchError, WatchSink, WatcherPort};
use arin_core::ports::{PtyError, PtyPort, PtySize, SpawnedPty};
use arin_core::project::ProjectId;
use arin_core::terminal::{ShellLaunchSpec, TerminalId};
use arin_core::workspace::AppSnapshot;
use arin_lib::SNAPSHOT_EVENT;
use arin_lib::launch::LaunchConfig;
use arin_lib::persistence::PersistencePorts;
use arin_lib::projects::ProjectPorts;
use arin_lib::sessions::ShellDefaults;
use arin_platform::desktop::MacPasteboard;
use arin_platform::fs::LocalFileSystem;
use arin_platform::system::UrandomIdSource;
use serde_json::json;
use support::{request, request_with};
use tauri::ipc::InvokeBody;
use tauri::test::{MockRuntime, get_ipc_response};
use tauri::{AppHandle, Listener, Manager, RunEvent, WebviewWindow};

const WAIT: Duration = Duration::from_secs(10);
/// 「来ない」ことを確かめるときに待つ時間。
const QUIET: Duration = Duration::from_millis(200);
const ALPHA_ID: &str = "3f2b8c1e-4d5a-4b6c-8d7e-9f0a1b2c3d4e";
const BETA_ID: &str = "8a7b6c5d-4e3f-4a1b-9c8d-7e6f5a4b3c2d";

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
// 偽の監視・PTY
// ---------------------------------------------------------------------------

/// 張った監視 1 本の記録。
struct Watched {
    root: PathBuf,
    alive: Arc<AtomicBool>,
}

/// 張った場所を記録する監視。
#[derive(Default)]
struct FakeWatcher {
    watches: Mutex<Vec<Watched>>,
}

struct FakeWatch(Arc<AtomicBool>);

impl ActiveWatch for FakeWatch {}

impl Drop for FakeWatch {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

impl WatcherPort for FakeWatcher {
    fn watch(&self, root: &Path, _sink: WatchSink) -> Result<Box<dyn ActiveWatch>, WatchError> {
        let alive = Arc::new(AtomicBool::new(true));
        lock(&self.watches).push(Watched {
            root: root.to_path_buf(),
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

/// 一時フォルダ(状態フォルダ `state` と、プロジェクトのフォルダ `alpha`・`beta`)。
struct Fixture {
    root: support::TempDir,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let root = support::TempDir::new(name);
        for dir in ["state", "alpha", "beta"] {
            fs::create_dir(root.path().join(dir)).expect("フォルダを作れなかった");
        }
        let fx = Self { root };
        let text = serde_json::to_string_pretty(&json!({
            "schemaVersion": 1,
            "projects": [
                {
                    "id": ALPHA_ID,
                    "bookmark": null,
                    "lastKnownPath": fx.dir("alpha"),
                    "customName": null,
                    "colorId": "blue"
                },
                {
                    "id": BETA_ID,
                    "bookmark": null,
                    "lastKnownPath": fx.dir("beta"),
                    "customName": null,
                    "colorId": "green"
                }
            ],
            "ui": { "selectedProjectId": ALPHA_ID }
        }))
        .expect("JSON にできなかった");
        fs::write(fx.state_dir().join("state.json"), text).expect("状態ファイルを置けなかった");
        fx
    }

    fn dir(&self, name: &str) -> PathBuf {
        self.root.path().join(name)
    }

    fn state_dir(&self) -> PathBuf {
        self.dir("state")
    }

    /// 監視が張られるのと同じ形(実パス)。
    fn real(&self, name: &str) -> PathBuf {
        fs::canonicalize(self.dir(name)).expect("実パスにできなかった")
    }

    fn launch(&self) -> LaunchConfig {
        LaunchConfig::from_inputs(None, Some(self.state_dir().as_os_str()), None)
            .unwrap_or_else(|e| panic!("組み立ての設定を作れなかった: {e}"))
    }

    /// 状態ファイルの `ui`。
    fn saved_ui(&self) -> Option<serde_json::Value> {
        let text = fs::read_to_string(self.state_dir().join("state.json")).ok()?;
        let value: serde_json::Value = serde_json::from_str(&text).ok()?;
        value.get("ui").cloned()
    }
}

/// 確かめるときに使うもの。
struct Ctx<'a> {
    handle: &'a AppHandle<MockRuntime>,
    window: &'a WebviewWindow<MockRuntime>,
    fx: &'a Fixture,
    watcher: &'a FakeWatcher,
    /// `app-snapshot` を受けた回数。
    sent: &'a AtomicUsize,
}

impl Ctx<'_> {
    fn snapshot(&self) -> AppSnapshot {
        get_ipc_response(self.window, request(self.handle, "get_app_snapshot"))
            .unwrap_or_else(|e| panic!("get_app_snapshot が失敗した: {e}"))
            .deserialize()
            .unwrap_or_else(|e| panic!("AppSnapshot として読めなかった: {e}"))
    }

    /// コマンドを呼ぶ。失敗したら文言を返す。
    fn call(&self, cmd: &str, args: serde_json::Value) -> Result<serde_json::Value, String> {
        get_ipc_response(
            self.window,
            request_with(self.handle, cmd, InvokeBody::Json(args)),
        )
        .map(|body| body.deserialize().unwrap_or(serde_json::Value::Null))
        .map_err(|e| e.to_string())
    }

    fn ok(&self, cmd: &str, args: serde_json::Value) -> serde_json::Value {
        self.call(cmd, args)
            .unwrap_or_else(|e| panic!("{cmd} が失敗した: {e}"))
    }

    fn sent(&self) -> usize {
        self.sent.load(Ordering::SeqCst)
    }

    /// `f` のあいだに写しが送られなかったこと。
    fn assert_not_sent(&self, what: &str, f: impl FnOnce()) {
        let before = self.sent();
        f();
        thread::sleep(QUIET);
        assert_eq!(self.sent(), before, "{what}で写しを送った");
    }

    /// `f` のあいだに写しが送られたこと。
    fn assert_sent(&self, what: &str, f: impl FnOnce()) {
        let before = self.sent();
        f();
        assert!(
            wait_until(|| self.sent() > before),
            "{what}で写しを送らない"
        );
    }

    fn tiles_of(&self, project: &str) -> Vec<TerminalId> {
        let id = ProjectId::parse(project).expect("ID が不正");
        self.snapshot()
            .terminals
            .into_iter()
            .filter(|t| t.project_id.as_ref() == Some(&id))
            .map(|t| t.id)
            .collect()
    }

    fn wait_active(&self, expected: &[PathBuf]) {
        assert!(
            wait_until(|| self.watcher.active() == expected),
            "監視の場所が {expected:?} にならない: {:?}",
            self.watcher.active()
        );
    }
}

fn project(id: &str) -> ProjectId {
    ProjectId::parse(id).expect("ID が不正")
}

// ---------------------------------------------------------------------------
// 確認(順に行う)
// ---------------------------------------------------------------------------

/// 前提: 起動直後。alpha に 1 枚足して、alpha 2 枚・beta 1 枚にしておく(タブ表示)。
fn 表示モードの切替で写しが変わり状態ファイルに保存される(c: &Ctx) {
    c.ok("new_terminal", json!({}));
    let before = c.snapshot();
    assert_eq!(before.display.mode, DisplayMode::Tabs);
    assert_eq!(before.terminals.len(), 3, "{before:?}");

    c.assert_sent("タイル表示への切替", || {
        c.ok("set_display_mode", json!({ "mode": "tiles" }));
    });

    let s = c.snapshot();
    assert_eq!(s.display.mode, DisplayMode::Tiles);
    assert_eq!(s.display.tile_ids.len(), 3, "タイルが全部並ばない: {s:?}");
    assert_eq!(s.display.page_count, 1);
    assert!(
        wait_until(|| c
            .fx
            .saved_ui()
            .is_some_and(|ui| ui["displayMode"] == json!("tiles"))),
        "状態ファイルに表示モードが書かれない: {:?}",
        c.fx.saved_ui()
    );
}

/// 前提: タイル表示。
fn 同じ表示モードを送っても写しを送らない(c: &Ctx) {
    c.assert_not_sent("同じ表示モード", || {
        c.ok("set_display_mode", json!({ "mode": "tiles" }));
    });
    assert_eq!(c.snapshot().display.mode, DisplayMode::Tiles);
}

/// 前提: タイル表示。
fn 知らない表示モードの文字列は失敗を返し何も変えない(c: &Ctx) {
    let before = c.snapshot();
    c.assert_not_sent("知らない表示モード", || {
        assert!(
            c.call("set_display_mode", json!({ "mode": "grid" }))
                .is_err(),
            "知らない値を受け付けた"
        );
        assert!(
            c.call("set_display_mode", json!({})).is_err(),
            "値の無い呼び出しを受け付けた"
        );
    });
    assert_eq!(c.snapshot(), before);
}

/// 前提: タイル表示、alpha を選択中(監視は alpha)。
fn タイルへのフォーカスで写しの選択が替わりツリーの監視が新しいルートに張り替わる(
    c: &Ctx,
) {
    c.wait_active(&[c.fx.real("alpha")]);
    let beta_tile = c.tiles_of(BETA_ID)[0].clone();

    c.assert_sent(
        "別のプロジェクトのタイルへのフォーカス",
        || {
            c.ok("focus_terminal", json!({ "terminalId": beta_tile }));
        },
    );

    let s = c.snapshot();
    assert_eq!(s.selected_project_id, Some(project(BETA_ID)));
    assert_eq!(s.display.focused_terminal_id, Some(beta_tile));
    c.wait_active(&[c.fx.real("beta")]);
}

/// 前提: タイル表示、beta を選択中。alpha のタイル 1 枚目へ移ってから、alpha の 2 枚目へ移る。
fn 同じプロジェクトのタイルの間を移っても監視を張り替えない(c: &Ctx) {
    let alpha = c.tiles_of(ALPHA_ID);
    c.ok("focus_terminal", json!({ "terminalId": alpha[0] }));
    c.wait_active(&[c.fx.real("alpha")]);
    thread::sleep(QUIET);
    let watched = c.watcher.count();

    c.assert_sent(
        "同じプロジェクトの別のタイルへのフォーカス",
        || {
            c.ok("focus_terminal", json!({ "terminalId": alpha[1] }));
        },
    );

    let s = c.snapshot();
    assert_eq!(s.selected_project_id, Some(project(ALPHA_ID)));
    assert_eq!(s.display.focused_terminal_id, Some(alpha[1].clone()));
    thread::sleep(QUIET);
    assert_eq!(
        c.watcher.count(),
        watched,
        "同じプロジェクトの中で張り替えた"
    );
    assert_eq!(c.watcher.active(), [c.fx.real("alpha")]);
}

/// 前提: タイル表示、alpha の 2 枚目にフォーカス。
fn フォーカス中のタイルへのフォーカスは写しを送らない(c: &Ctx) {
    let focused = c
        .snapshot()
        .display
        .focused_terminal_id
        .expect("フォーカス中のタイルが無い");
    c.assert_not_sent(
        "フォーカス中のタイルへのフォーカス",
        || {
            c.ok("focus_terminal", json!({ "terminalId": focused }));
        },
    );
}

/// 前提: タイル表示。
fn 知らないターミナルへのフォーカスは失敗を返し何も変えない(c: &Ctx) {
    let before = c.snapshot();
    let watched = c.watcher.count();
    c.assert_not_sent("知らないターミナルへのフォーカス", || {
        let result = c.call(
            "focus_terminal",
            json!({ "terminalId": "terminal-unknown" }),
        );
        assert!(result.is_err(), "知らない ID を受け付けた: {result:?}");
    });
    assert_eq!(c.snapshot(), before);
    assert_eq!(c.watcher.count(), watched);
}

/// 前提: タイル表示、alpha を選択中(alpha 2 枚・beta 1 枚)。alpha に 7 枚足して 10 枚(alpha 9 枚・beta 1 枚)にする。
fn ページ送りと拡大のコマンドで写しの表示が変わる(c: &Ctx) {
    for _ in 0..7 {
        c.ok("new_terminal", json!({}));
    }
    let s = c.snapshot();
    assert_eq!(s.terminals.len(), 10);
    assert_eq!((s.display.page, s.display.page_count), (0, 2), "{s:?}");
    assert_eq!(s.display.tile_ids.len(), 9);
    let beta_tile = c.tiles_of(BETA_ID)[0].clone();

    c.assert_sent("次のページ", || {
        c.ok("next_tile_page", json!({}));
    });
    let s = c.snapshot();
    assert_eq!(s.display.page, 1);
    assert_eq!(s.display.tile_ids, std::slice::from_ref(&beta_tile));
    assert_eq!(s.display.focused_terminal_id, Some(beta_tile));
    assert_eq!(s.selected_project_id, Some(project(BETA_ID)));

    c.assert_not_sent("最後のページの次", || {
        c.ok("next_tile_page", json!({}));
    });

    c.assert_sent("前のページ", || {
        c.ok("previous_tile_page", json!({}));
    });
    let s = c.snapshot();
    assert_eq!(s.display.page, 0);
    let first = c.tiles_of(ALPHA_ID)[0].clone();
    assert_eq!(s.display.focused_terminal_id, Some(first.clone()));

    c.assert_not_sent("最初のページの前", || {
        c.ok("previous_tile_page", json!({}));
    });

    c.assert_sent("拡大", || {
        c.ok("toggle_tile_maximize", json!({}));
    });
    let s = c.snapshot();
    assert!(s.display.maximized);
    assert_eq!(s.display.mode, DisplayMode::Tiles);
    assert_eq!(s.display.focused_terminal_id, Some(first));

    c.assert_sent("拡大を解く", || {
        c.ok("toggle_tile_maximize", json!({}));
    });
    let s = c.snapshot();
    assert!(!s.display.maximized);
    assert_eq!(s.display.page, 0);
}

/// 前提: タイル表示(拡大していない)。タブ表示に戻してから、ページ送りと拡大を呼ぶ。
fn タブ表示ではページ送りと拡大は写しを送らない(c: &Ctx) {
    c.ok("set_display_mode", json!({ "mode": "tabs" }));
    // タブ表示(既定)は `ui.displayMode` を書かない(T46c の【仮定】。読むときは無ければタブ表示)。
    assert!(
        wait_until(|| c
            .fx
            .saved_ui()
            .is_some_and(|ui| ui.get("displayMode").is_none())),
        "状態ファイルの表示モードがタブ表示に戻らない: {:?}",
        c.fx.saved_ui()
    );
    let before = c.snapshot();
    c.assert_not_sent("タブ表示のページ送りと拡大", || {
        c.ok("next_tile_page", json!({}));
        c.ok("previous_tile_page", json!({}));
        c.ok("toggle_tile_maximize", json!({}));
    });
    assert_eq!(c.snapshot(), before);
}

type Check = fn(&Ctx);

const CHECKS: &[(&str, Check)] = &[
    (
        "表示モードの切替で写しが変わり状態ファイルに保存される",
        表示モードの切替で写しが変わり状態ファイルに保存される,
    ),
    (
        "同じ表示モードを送っても写しを送らない",
        同じ表示モードを送っても写しを送らない,
    ),
    (
        "知らない表示モードの文字列は失敗を返し何も変えない",
        知らない表示モードの文字列は失敗を返し何も変えない,
    ),
    (
        "タイルへのフォーカスで写しの選択が替わりツリーの監視が新しいルートに張り替わる",
        タイルへのフォーカスで写しの選択が替わりツリーの監視が新しいルートに張り替わる,
    ),
    (
        "同じプロジェクトのタイルの間を移っても監視を張り替えない",
        同じプロジェクトのタイルの間を移っても監視を張り替えない,
    ),
    (
        "フォーカス中のタイルへのフォーカスは写しを送らない",
        フォーカス中のタイルへのフォーカスは写しを送らない,
    ),
    (
        "知らないターミナルへのフォーカスは失敗を返し何も変えない",
        知らないターミナルへのフォーカスは失敗を返し何も変えない,
    ),
    (
        "ページ送りと拡大のコマンドで写しの表示が変わる",
        ページ送りと拡大のコマンドで写しの表示が変わる,
    ),
    (
        "タブ表示ではページ送りと拡大は写しを送らない",
        タブ表示ではページ送りと拡大は写しを送らない,
    ),
];

fn main() {
    let fx = Fixture::new("display-commands");
    let watcher = Arc::new(FakeWatcher::default());
    let pasteboard = MacPasteboard::named(format!(
        "dev.arin.test.display-commands.{}",
        std::process::id()
    ));
    assert!(
        !pasteboard.is_general(),
        "ユーザーのクリップボードを使おうとした"
    );
    let project_ports = ProjectPorts {
        ids: Arc::new(UrandomIdSource::new()),
        pasteboard: Arc::new(pasteboard.clone()),
        picker: None,
    };
    let app = arin_lib::builder_with_watcher::<MockRuntime>(
        fx.launch(),
        Arc::new(NoPty),
        Arc::new(|| ShellDefaults {
            shell: PathBuf::from("/bin/zsh"),
            locale: None,
        }),
        PersistencePorts::native(fx.state_dir()),
        project_ports,
        Arc::new(LocalFileSystem::new()),
        Arc::clone(&watcher) as Arc<dyn WatcherPort>,
    )
    .build(tauri::generate_context!())
    .unwrap_or_else(|e| panic!("組み立てに失敗した: {e}"));

    let passed = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&passed);
    let exited = Arc::new(AtomicBool::new(false));
    let seen_exit = Arc::clone(&exited);
    let fx = Arc::new(fx);
    let fx_ref = Arc::clone(&fx);
    let watcher_ref = Arc::clone(&watcher);
    app.run_return(move |handle, event| {
        if let RunEvent::Ready = event {
            let window = handle
                .get_webview_window("main")
                .expect("設定にあるウィンドウ main が作られなかった");
            let sent = Arc::new(AtomicUsize::new(0));
            let seen = Arc::clone(&sent);
            handle.listen(SNAPSHOT_EVENT, move |_| {
                seen.fetch_add(1, Ordering::SeqCst);
            });
            let ctx = Ctx {
                handle,
                window: &window,
                fx: &fx_ref,
                watcher: &watcher_ref,
                sent: &sent,
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
        let exiting = matches!(event, RunEvent::Exit);
        arin_lib::lifecycle::on_run_event(handle, event);
        if exiting {
            seen_exit.store(true, Ordering::SeqCst);
        }
    });
    pasteboard.release();
    assert!(exited.load(Ordering::SeqCst), "終了のイベントが来なかった");
    assert_eq!(
        passed.load(Ordering::SeqCst),
        CHECKS.len(),
        "起動の完了まで進まなかった"
    );
    let _ = io::Write::flush(&mut io::stdout());
    println!("test result: ok. {} passed", CHECKS.len());
}
