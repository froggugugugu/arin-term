//! メニューバーの組み立てと、項目が選ばれたときの動き(`menu.rs`、FR-043・FR-044、計画書 T33e・決定 (b)、設計書 §9.1)を、
//! Tauri の模擬ランタイムで確かめる。
//!
//! - 組み立て: Tauri の既定のメニューではなく本アプリのメニューが付き、「ターミナル」にタブの操作が、「編集」に
//!   コピー・ペースト(macOS 標準の項目)と自前の「すべてを選択」が、アプリ名のメニューの最後に自前の「終了」がある
//! - 選ばれたとき: 模擬ランタイムではメニューを押せないので、本物の受け口(`menu::on_menu_event`)に項目の ID を渡す
//!
//! アクセラレータ(⌘T など)が実際に効くことと、それが WKWebView より先にメニューに届くことは AppKit の仕事なので、
//! 実機で確かめる(T36)。Tauri はアクセラレータの文字列を読めないと黙って捨てるので、文字列は定数の表で確かめる。
//!
//! macOS ではメニューの部品をメインスレッドでしか作れないため、`harness = false` で走らせる(`app_builder` と同じ)。

mod support;

use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use arin_core::ports::{PtyError, PtyPort, PtySize, SpawnedPty};
use arin_core::terminal::ShellLaunchSpec;
use arin_core::workspace::{AppSnapshot, MenuCommand};
use arin_lib::events::MENU_COMMAND_EVENT;
use arin_lib::launch::LaunchConfig;
use arin_lib::menu::{
    self, EDIT_MENU_ITEMS, EDIT_MENU_TITLE, MenuEntry, QUIT_ACCELERATOR, QUIT_ID, SELECT_ALL,
    TERMINAL_MENU_ITEMS, TERMINAL_MENU_TITLE,
};
use arin_lib::sessions::{SessionRegistry, ShellDefaults};
use serde_json::json;
use tauri::menu::{Menu, MenuItemKind};
use tauri::test::MockRuntime;
use tauri::{App, AppHandle, Listener, Manager, RunEvent};

const WAIT: Duration = Duration::from_secs(5);
const PROJECT_ID: &str = "3f2b8c1e-4d5a-4b6c-8d7e-9f0a1b2c3d4e";

/// シェルを起動しない PTY(このテストはターミナルに接続しない)。
struct NoPty;

impl PtyPort for NoPty {
    fn spawn(&self, _spec: &ShellLaunchSpec, _size: PtySize) -> Result<SpawnedPty, PtyError> {
        Err(PtyError::Spawn("このテストではシェルを起動しない".into()))
    }
}

fn build_app(launch: LaunchConfig) -> App<MockRuntime> {
    arin_lib::builder_with_pty::<MockRuntime>(
        launch,
        Arc::new(NoPty),
        Arc::new(|| ShellDefaults {
            shell: PathBuf::from("/bin/zsh"),
            locale: None,
        }),
    )
    .build(tauri::generate_context!())
    .unwrap_or_else(|e| panic!("組み立てに失敗した: {e}"))
}

/// メニューの見出しの一覧(上の段)。
fn titles(menu: &Menu<MockRuntime>) -> Vec<String> {
    menu.items()
        .expect("メニューの項目を読めなかった")
        .iter()
        .map(|item| match item {
            MenuItemKind::Submenu(sub) => sub.text().expect("見出しを読めなかった"),
            other => panic!("上の段に見出し以外の項目がある: {:?}", other.id()),
        })
        .collect()
}

/// 見出しの下の項目。標準の項目は `("標準", 文言)`、自前の項目は `(ID, 文言)`、区切り線は `("標準", "---")`。
fn entries(menu: &Menu<MockRuntime>, title: &str) -> Vec<(String, String)> {
    let items = menu.items().expect("メニューの項目を読めなかった");
    let sub = items
        .iter()
        .filter_map(MenuItemKind::as_submenu)
        .find(|sub| sub.text().is_ok_and(|t| t == title))
        .unwrap_or_else(|| panic!("見出し {title} が無い"));
    sub.items()
        .expect("見出しの下の項目を読めなかった")
        .iter()
        .map(|item| match item {
            MenuItemKind::Predefined(p) => {
                let text = p.text().expect("項目の文言を読めなかった");
                let text = if text.is_empty() {
                    "---".to_owned()
                } else {
                    text
                };
                ("標準".to_owned(), text)
            }
            MenuItemKind::MenuItem(m) => (
                m.id().0.clone(),
                m.text().expect("項目の文言を読めなかった"),
            ),
            other => panic!("{title} に想定外の種類の項目がある: {:?}", other.id()),
        })
        .collect()
}

fn own(entry: &MenuEntry) -> (String, String) {
    (entry.id.to_owned(), entry.text.to_owned())
}

fn アプリのメニューは本アプリの見出しだけで既定のメニューではない() {
    let state_dir = support::TempDir::new("menu-titles");
    let app = build_app(support::launch_in(&state_dir, false));
    let menu = app.menu().expect("アプリにメニューが付いていない");

    let name = app.package_info().name.clone();
    // 既定のメニューにある「File」「View」「Window」「Help」は置かない(⌘W などを本アプリのタブ操作に使うため)。
    assert_eq!(
        titles(&menu),
        vec![
            name,
            TERMINAL_MENU_TITLE.to_owned(),
            EDIT_MENU_TITLE.to_owned()
        ]
    );
}

fn メニューに新規タブ_タブを閉じる_前後のタブがありアクセラレータが付いている() {
    let state_dir = support::TempDir::new("menu-terminal");
    let app = build_app(support::launch_in(&state_dir, false));
    let menu = app.menu().expect("アプリにメニューが付いていない");

    let table: Vec<(&str, &str, &str)> = TERMINAL_MENU_ITEMS
        .iter()
        .map(|e| (e.id, e.text, e.accelerator))
        .collect();
    assert_eq!(
        table,
        vec![
            ("terminal.new-tab", "新規タブ", "Cmd+T"),
            ("terminal.close-tab", "タブを閉じる", "Cmd+W"),
            ("terminal.previous-tab", "前のタブ", "Cmd+Shift+["),
            ("terminal.next-tab", "次のタブ", "Cmd+Shift+]"),
        ]
    );
    let [new_tab, close_tab, previous, next] = &TERMINAL_MENU_ITEMS;
    let separator = ("標準".to_owned(), "---".to_owned());
    assert_eq!(
        entries(&menu, TERMINAL_MENU_TITLE),
        vec![
            own(new_tab),
            own(close_tab),
            separator,
            own(previous),
            own(next)
        ]
    );
}

fn 編集にはコピーとペーストと自前のすべてを選択がある() {
    let state_dir = support::TempDir::new("menu-edit");
    let app = build_app(support::launch_in(&state_dir, false));
    let menu = app.menu().expect("アプリにメニューが付いていない");

    assert_eq!(EDIT_MENU_ITEMS, ["コピー", "ペースト", "すべてを選択"]);
    assert_eq!(
        (SELECT_ALL.id, SELECT_ALL.text, SELECT_ALL.accelerator),
        ("edit.select-all", "すべてを選択", "Cmd+A")
    );
    let standard = |t: &str| ("標準".to_owned(), t.to_owned());
    // コピーとペーストは macOS 標準の項目のまま(画面の copy・paste イベントになる。M1)。
    assert_eq!(
        entries(&menu, EDIT_MENU_TITLE),
        vec![
            standard("コピー"),
            standard("ペースト"),
            standard("---"),
            own(&SELECT_ALL)
        ]
    );
}

fn アプリ名のメニューの最後は自前の終了() {
    let state_dir = support::TempDir::new("menu-app");
    let app = build_app(support::launch_in(&state_dir, false));
    let menu = app.menu().expect("アプリにメニューが付いていない");
    let name = app.package_info().name.clone();

    assert_eq!((QUIT_ID, QUIT_ACCELERATOR), ("app.quit", "Cmd+Q"));
    let items = entries(&menu, &name);
    // 標準の「終了」(terminate:)は終了の要求を経由せずに終わるので、確認(FR-063)を挟めない。自前の項目にする。
    assert_eq!(
        items.last(),
        Some(&(QUIT_ID.to_owned(), format!("{name} を終了")))
    );
    assert_eq!(
        items[..items.len() - 1]
            .iter()
            .filter(|(kind, _)| kind != "標準")
            .count(),
        0,
        "終了のほかに自前の項目がある: {items:?}"
    );
}

/// プロジェクトを 1 件持つ状態ファイルを置いた一時フォルダ(状態フォルダとプロジェクトのフォルダ)。
struct ProjectFixture {
    root: support::TempDir,
}

impl ProjectFixture {
    fn new(name: &str) -> Self {
        let root = support::TempDir::new(name);
        let state = root.path().join("state");
        let project = root.path().join("project");
        fs::create_dir(&state).expect("状態フォルダを作れなかった");
        fs::create_dir(&project).expect("プロジェクトのフォルダを作れなかった");
        write_project_state(&state, &project);
        Self { root }
    }

    fn launch(&self) -> LaunchConfig {
        LaunchConfig::from_inputs(None, Some(self.root.path().join("state").as_os_str()), None)
            .unwrap_or_else(|e| panic!("組み立ての設定を作れなかった: {e}"))
    }
}

fn write_project_state(state_dir: &Path, root: &Path) {
    let text = serde_json::to_string_pretty(&json!({
        "schemaVersion": 1,
        "projects": [{
            "id": PROJECT_ID,
            "bookmark": null,
            "lastKnownPath": root,
            "customName": null,
            "colorId": "green"
        }],
        "ui": { "selectedProjectId": PROJECT_ID }
    }))
    .expect("JSON にできなかった");
    fs::write(state_dir.join("state.json"), text).expect("状態ファイルを置けなかった");
}

fn snapshot(handle: &AppHandle<MockRuntime>) -> AppSnapshot {
    handle.state::<SessionRegistry>().snapshot()
}

/// (タブの順の ID, 表示中の ID)。
fn tabs(handle: &AppHandle<MockRuntime>) -> (Vec<String>, Option<String>) {
    let s = snapshot(handle);
    let project = &s.projects[0];
    (
        project
            .terminal_ids
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect(),
        project
            .active_terminal_id
            .as_ref()
            .map(|id| id.as_str().to_owned()),
    )
}

fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !done() {
        assert!(Instant::now() < deadline, "{what}まで待てなかった");
        thread::sleep(Duration::from_millis(5));
    }
}

/// 組み立てて起動し、準備ができたら `on_ready` を呼んでからウィンドウを閉じ、終了まで進める。
fn run_app(launch: LaunchConfig, on_ready: impl FnOnce(&AppHandle<MockRuntime>) + 'static) {
    let app = build_app(launch);
    let mut on_ready = Some(on_ready);
    let exited = Rc::new(Cell::new(false));
    let seen = Rc::clone(&exited);
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
            seen.set(true);
        }
    });
    assert!(exited.get(), "終了まで進まなかった");
}

fn 新規タブの項目で選択中のプロジェクトにタブが増え_前後のタブで表示中が回る() {
    let fx = ProjectFixture::new("menu-new-tab");
    run_app(fx.launch(), |handle| {
        let [new_tab, _, previous, next] = &TERMINAL_MENU_ITEMS;
        let (before, first_active) = tabs(handle);
        assert_eq!(before.len(), 1, "起動時に 1 タブ開いていない");

        menu::on_menu_event(handle, new_tab.id);
        let (after, active) = tabs(handle);
        assert_eq!(after.len(), 2, "新規タブでタブが増えない");
        assert_eq!(after[0], before[0]);
        assert_eq!(
            active.as_deref(),
            Some(after[1].as_str()),
            "新しいタブが表示中でない"
        );

        menu::on_menu_event(handle, previous.id);
        assert_eq!(tabs(handle).1, first_active, "前のタブに移らない");
        // 先頭の前は末尾に回り込む(【仮定】17)。
        menu::on_menu_event(handle, previous.id);
        assert_eq!(tabs(handle).1.as_deref(), Some(after[1].as_str()));
        menu::on_menu_event(handle, next.id);
        assert_eq!(tabs(handle).1, first_active, "次のタブで先頭に回り込まない");
    });
}

fn タブを閉じるの項目で表示中のタブが閉じ_0_枚のときは何もしない() {
    let fx = ProjectFixture::new("menu-close-tab");
    run_app(fx.launch(), |handle| {
        let [new_tab, close_tab, previous, next] = &TERMINAL_MENU_ITEMS;
        menu::on_menu_event(handle, new_tab.id);
        let (two, _) = tabs(handle);
        assert_eq!(two.len(), 2);

        // 表示中(新しいほう)が閉じ、左隣が表示中になる(【仮定】17)。閉じるのは待ってよいスレッドで行う。
        menu::on_menu_event(handle, close_tab.id);
        wait_until("表示中のタブが閉じる", || {
            tabs(handle).0.len() == 1
        });
        assert_eq!(tabs(handle), (vec![two[0].clone()], Some(two[0].clone())));

        menu::on_menu_event(handle, close_tab.id);
        wait_until("最後のタブが閉じる", || tabs(handle).0.is_empty());

        // 0 枚: 閉じる・前後は何もしない。新しいタブは自動で開かない(C-21)。
        menu::on_menu_event(handle, close_tab.id);
        menu::on_menu_event(handle, previous.id);
        menu::on_menu_event(handle, next.id);
        thread::sleep(Duration::from_millis(100));
        assert_eq!(tabs(handle), (vec![], None));
        assert!(snapshot(handle).terminals.is_empty());

        // 0 枚から新規タブで開ける(C-21)。
        menu::on_menu_event(handle, new_tab.id);
        assert_eq!(tabs(handle).0.len(), 1);
    });
}

fn プロジェクトが無いときタブの項目は何もしない() {
    let state_dir = support::TempDir::new("menu-no-project");
    run_app(support::launch_in(&state_dir, false), |handle| {
        for entry in &TERMINAL_MENU_ITEMS {
            menu::on_menu_event(handle, entry.id);
        }
        menu::on_menu_event(handle, "no-such-item");
        thread::sleep(Duration::from_millis(100));
        let s = snapshot(handle);
        assert!(s.projects.is_empty());
        assert!(s.terminals.is_empty(), "タブが開いた: {s:?}");
    });
}

fn すべてを選択は画面へ_menu_command_を送りタブに触れない() {
    let fx = ProjectFixture::new("menu-select-all");
    run_app(fx.launch(), |handle| {
        let received: Arc<Mutex<Vec<String>>> = Arc::default();
        let sink = Arc::clone(&received);
        handle.listen(MENU_COMMAND_EVENT, move |event| {
            sink.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(event.payload().to_owned());
        });
        let before = snapshot(handle);

        menu::on_menu_event(handle, SELECT_ALL.id);

        wait_until("menu-command が届く", || {
            !received
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .is_empty()
        });
        let payloads = received
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        assert_eq!(MENU_COMMAND_EVENT, "menu-command");
        assert_eq!(payloads, vec!["\"select-all\"".to_owned()]);
        let command: MenuCommand =
            serde_json::from_str(&payloads[0]).expect("MenuCommand として読めなかった");
        assert_eq!(command, MenuCommand::SelectAll);
        assert_eq!(snapshot(handle), before, "すべてを選択で状態が変わった");
    });
}

fn main() {
    let tests: [(&str, fn()); 8] = [
        (
            "アプリのメニューは本アプリの見出しだけで既定のメニューではない",
            アプリのメニューは本アプリの見出しだけで既定のメニューではない,
        ),
        (
            "メニューに新規タブ_タブを閉じる_前後のタブがありアクセラレータが付いている",
            メニューに新規タブ_タブを閉じる_前後のタブがありアクセラレータが付いている,
        ),
        (
            "編集にはコピーとペーストと自前のすべてを選択がある",
            編集にはコピーとペーストと自前のすべてを選択がある,
        ),
        ("アプリ名のメニューの最後は自前の終了", アプリ名のメニューの最後は自前の終了),
        (
            "新規タブの項目で選択中のプロジェクトにタブが増え_前後のタブで表示中が回る",
            新規タブの項目で選択中のプロジェクトにタブが増え_前後のタブで表示中が回る,
        ),
        (
            "タブを閉じるの項目で表示中のタブが閉じ_0_枚のときは何もしない",
            タブを閉じるの項目で表示中のタブが閉じ_0_枚のときは何もしない,
        ),
        (
            "プロジェクトが無いときタブの項目は何もしない",
            プロジェクトが無いときタブの項目は何もしない,
        ),
        (
            "すべてを選択は画面へ_menu_command_を送りタブに触れない",
            すべてを選択は画面へ_menu_command_を送りタブに触れない,
        ),
    ];
    for (name, test) in tests {
        test();
        println!("test {name} ... ok");
    }
}
