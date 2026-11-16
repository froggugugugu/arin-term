//! メニューバーの組み立てと、項目が選ばれたときの動き(`menu.rs`、FR-043・FR-044、計画書 T33e・T42e・決定 (b)、設計書 §9.1)を、
//! Tauri の模擬ランタイムで確かめる。
//!
//! - 組み立て: Tauri の既定のメニューではなく本アプリのメニューが付き、「ターミナル」にタブの操作が、「編集」に
//!   切り取り・コピー・ペースト(macOS 標準の項目)と自前の「すべてを選択」が、アプリ名のメニューの最後に自前の「終了」がある
//! - 表から作る(T42e): 自前の項目の ID とアクセラレータはショートカットの表(`arin_core::shortcuts`)の受け口が
//!   「メニューの項目」の行から作り、標準の項目は受け口が「編集の項目」の行から作る。`get_shortcuts` は同じ表を画面へ返す
//! - 選ばれたとき: 模擬ランタイムではメニューを押せないので、本物の受け口(`menu::on_menu_event`)に項目の ID を渡す
//!
//! アクセラレータ(⌘T など)が実際に効くことと、それが WKWebView より先にメニューに届くことは AppKit の仕事なので、
//! 実機で確かめる(T36・T45)。Tauri はアクセラレータの文字列を muda で読めないと黙って捨て、作った項目からは
//! アクセラレータを読み戻せないので、メニューに渡す文字列を muda で読めることをここで確かめる。
//!
//! macOS ではメニューの部品をメインスレッドでしか作れないため、`harness = false` で走らせる(`app_builder` と同じ)。

mod support;

use std::cell::Cell;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::str::FromStr;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use arin_core::layout::tiles::DisplayMode;
use arin_core::ports::{PtyError, PtyPort, PtySize, SpawnedPty};
use arin_core::shortcuts::table::{
    ShortcutBinding, ShortcutCommandId, ShortcutReceiver, ShortcutTable,
};
use arin_core::terminal::ShellLaunchSpec;
use arin_core::workspace::{AppSnapshot, MenuCommand};
use arin_lib::events::MENU_COMMAND_EVENT;
use arin_lib::launch::LaunchConfig;
use arin_lib::menu::{
    self, EDIT_MENU_TITLE, OPEN_SETTINGS_FILE_ID, OPEN_SETTINGS_FILE_LABEL, Shortcuts,
    TERMINAL_MENU_TITLE, VIEW_MENU_TITLE,
};
use arin_lib::sessions::{SessionRegistry, ShellDefaults};
use muda::accelerator::Accelerator;
use serde_json::json;
use tauri::menu::{Menu, MenuItemKind};
use tauri::test::{MockRuntime, get_ipc_response};
use tauri::{App, AppHandle, Listener, Manager, RunEvent};

const WAIT: Duration = Duration::from_secs(5);
const PROJECT_ID: &str = "3f2b8c1e-4d5a-4b6c-8d7e-9f0a1b2c3d4e";

/// 生成した既定の表(画面の偽の Rust 側と Vitest が使う JSON)。
const GENERATED_DEFAULT_SHORTCUTS: &str =
    include_str!("../../src/ipc/generated/defaultShortcuts.json");

use ShortcutCommandId as C;

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

/// メニューのすべての見出しの下にある自前の項目の ID。
fn own_ids(menu: &Menu<MockRuntime>) -> BTreeSet<String> {
    menu.items()
        .expect("メニューの項目を読めなかった")
        .iter()
        .filter_map(MenuItemKind::as_submenu)
        .flat_map(|sub| sub.items().expect("見出しの下の項目を読めなかった"))
        .filter_map(|item| item.as_menuitem().map(|m| m.id().0.clone()))
        .collect()
}

/// 自前の項目 1 つの `(ID, 文言)`。
fn own(command: ShortcutCommandId) -> (String, String) {
    (
        command.as_str().to_owned(),
        menu::label(command)
            .unwrap_or_else(|| panic!("{command} の文言が無い"))
            .to_owned(),
    )
}

fn standard(text: &str) -> (String, String) {
    ("標準".to_owned(), text.to_owned())
}

fn separator() -> (String, String) {
    standard("---")
}

/// 「表示」の末尾の組(タイルの番号キー 9 つ。ADR-014 の 4)。
const FOCUS_TILES: [ShortcutCommandId; 9] = [
    C::ViewFocusTile1,
    C::ViewFocusTile2,
    C::ViewFocusTile3,
    C::ViewFocusTile4,
    C::ViewFocusTile5,
    C::ViewFocusTile6,
    C::ViewFocusTile7,
    C::ViewFocusTile8,
    C::ViewFocusTile9,
];

/// 「表示」の末尾の、区切り線とタイルの番号キーの 9 項目。
fn focus_tile_entries() -> Vec<(String, String)> {
    std::iter::once(separator())
        .chain(FOCUS_TILES.into_iter().map(own))
        .collect()
}

/// 表の外の自前の項目「設定ファイルを開く…」(キーなし。ADR-013 の 6)。
fn open_settings_file() -> (String, String) {
    (
        OPEN_SETTINGS_FILE_ID.to_owned(),
        OPEN_SETTINGS_FILE_LABEL.to_owned(),
    )
}

/// 既定の表の `command` の行の受け口だけを替えた表。
fn table_with_receiver(changes: &[(ShortcutCommandId, ShortcutReceiver)]) -> ShortcutTable {
    let rows = ShortcutTable::default_table()
        .bindings()
        .iter()
        .map(|b| {
            let receiver = changes
                .iter()
                .find(|(c, _)| *c == b.command())
                .map_or(b.receiver(), |(_, r)| *r);
            ShortcutBinding::new(b.command(), b.accelerator(), receiver)
        })
        .collect();
    ShortcutTable::from_bindings(rows).unwrap_or_else(|e| panic!("表を作れなかった: {e:?}"))
}

fn アプリのメニューは本アプリの見出しだけで既定のメニューではない() {
    let state_dir = support::TempDir::new("menu-titles");
    let app = build_app(support::launch_in(&state_dir, false));
    let menu = app.menu().expect("アプリにメニューが付いていない");

    let name = app.package_info().name.clone();
    // 既定のメニューにある「File」「View」「Window」「Help」は置かない(⌘W などを本アプリのタブ操作に使うため)。
    // 並びは `見出しの並びは_arin_ターミナル_編集_表示` が確かめる(T47c で「表示」を足した)。
    let given = titles(&menu);
    for default_title in ["File", "Edit", "View", "Window", "Help"] {
        assert!(
            !given.iter().any(|t| t == default_title),
            "既定のメニューの見出し {default_title} がある: {given:?}"
        );
    }
    assert_eq!(given.first(), Some(&name));
    assert_eq!(
        given.len(),
        4,
        "本アプリの見出しのほかに見出しがある: {given:?}"
    );
}

fn 見出しの並びは_arin_ターミナル_編集_表示() {
    let state_dir = support::TempDir::new("menu-title-order");
    let app = build_app(support::launch_in(&state_dir, false));
    let menu = app.menu().expect("アプリにメニューが付いていない");

    // 「表示」は「編集」の右(macOS の並び。【仮定】10)。
    assert_eq!(VIEW_MENU_TITLE, "表示");
    assert_eq!(
        titles(&menu),
        vec![
            app.package_info().name.clone(),
            TERMINAL_MENU_TITLE.to_owned(),
            EDIT_MENU_TITLE.to_owned(),
            VIEW_MENU_TITLE.to_owned(),
        ]
    );
}

fn 表示に切替_前のページ_次のページ_拡大_タイルの番号がこの順である() {
    let state_dir = support::TempDir::new("menu-view");
    let app = build_app(support::launch_in(&state_dir, false));
    let menu = app.menu().expect("アプリにメニューが付いていない");

    // 文言は固定(【仮定】10)。ID は表のコマンドの ID。
    let labels: Vec<(&str, Option<&str>)> = [
        C::ViewToggleDisplayMode,
        C::ViewPreviousPage,
        C::ViewNextPage,
        C::ViewToggleMaximize,
    ]
    .into_iter()
    .map(|c| (c.as_str(), menu::label(c)))
    .collect();
    assert_eq!(
        labels,
        vec![
            (
                "view.toggle-display-mode",
                Some("タイル表示とタブ表示を切り替える")
            ),
            ("view.previous-page", Some("前のページ")),
            ("view.next-page", Some("次のページ")),
            ("view.toggle-maximize", Some("タイルを拡大/元に戻す")),
        ]
    );
    // タイルの番号キーの文言(ADR-014 の 4)。
    for (n, command) in FOCUS_TILES.into_iter().enumerate() {
        let n = n + 1;
        assert_eq!(command.as_str(), format!("view.focus-tile-{n}"));
        assert_eq!(
            menu::label(command),
            Some(format!("タイル {n} にフォーカス").as_str())
        );
    }
    assert_eq!(
        entries(&menu, VIEW_MENU_TITLE),
        [
            vec![
                own(C::ViewToggleDisplayMode),
                separator(),
                own(C::ViewPreviousPage),
                own(C::ViewNextPage),
                separator(),
                own(C::ViewToggleMaximize),
            ],
            // M4.7 のタイルの番号キー(ADR-014 の 4)。
            focus_tile_entries(),
        ]
        .concat()
    );
}

fn 表示の項目のアクセラレータが表の既定のキーと同じ() {
    let table = ShortcutTable::default_table();
    // 【仮定】9 の仮のキー。メニューに渡す文字列は表の行そのもの(menu.rs に直書きしない)。
    let expected = [
        (C::ViewToggleDisplayMode, "Cmd+Shift+G"),
        (C::ViewPreviousPage, "Cmd+Alt+ArrowLeft"),
        (C::ViewNextPage, "Cmd+Alt+ArrowRight"),
        (C::ViewToggleMaximize, "Cmd+Shift+Enter"),
        (C::ViewFocusTile1, "Cmd+1"),
        (C::ViewFocusTile5, "Cmd+5"),
        (C::ViewFocusTile9, "Cmd+9"),
    ];
    for (command, key) in expected {
        let given = menu::accelerator(&table, command)
            .unwrap_or_else(|| panic!("{command} にアクセラレータが無い"));
        assert_eq!(
            given,
            table.get(command).accelerator().to_string(),
            "{command}"
        );
        assert_eq!(given, key, "{command}");
        assert_eq!(
            Accelerator::from_str(&given).expect("muda が読めない"),
            Accelerator::from_str(key).expect("muda が読めない"),
            "{command}"
        );
    }
}

fn 受け口がメニューの項目の行にはすべて文言がある() {
    // 文言の足し忘れ(`label` が `None`)はメニューから黙って消えるので、ここで捕まえる。
    // 「終了」だけは文言にアプリ名を入れて作るので `label` を持たない(`QUIT_ID`)。
    let table = ShortcutTable::default_table();
    let missing: Vec<&str> = table
        .by_receiver(ShortcutReceiver::MenuItem)
        .map(ShortcutBinding::command)
        .filter(|&c| c != C::AppQuit && menu::label(c).is_none())
        .map(ShortcutCommandId::as_str)
        .collect();
    assert!(missing.is_empty(), "文言が無い: {missing:?}");
}

fn メニューに新規タブ_タブを閉じる_前後のタブがある() {
    let state_dir = support::TempDir::new("menu-terminal");
    let app = build_app(support::launch_in(&state_dir, false));
    let menu = app.menu().expect("アプリにメニューが付いていない");

    let labels: Vec<(&str, Option<&str>)> = [
        C::TerminalNewTab,
        C::TerminalCloseTab,
        C::TerminalPreviousTab,
        C::TerminalNextTab,
    ]
    .into_iter()
    .map(|c| (c.as_str(), menu::label(c)))
    .collect();
    assert_eq!(
        labels,
        vec![
            ("terminal.new-tab", Some("新規タブ")),
            ("terminal.close-tab", Some("タブを閉じる")),
            ("terminal.previous-tab", Some("前のタブ")),
            ("terminal.next-tab", Some("次のタブ")),
        ]
    );
    assert_eq!(
        entries(&menu, TERMINAL_MENU_TITLE),
        vec![
            own(C::TerminalNewTab),
            own(C::TerminalCloseTab),
            separator(),
            own(C::TerminalPreviousTab),
            own(C::TerminalNextTab)
        ]
    );
}

fn 編集に切り取り_コピー_ペースト_すべてを選択がこの順である() {
    let state_dir = support::TempDir::new("menu-edit");
    let app = build_app(support::launch_in(&state_dir, false));
    let menu = app.menu().expect("アプリにメニューが付いていない");

    // 切り取り・コピー・ペーストは macOS 標準の項目(画面の cut・copy・paste イベントになる。M1、T42e)。
    // すべてを選択は自前の項目(標準のものは xterm.js ではなく文書全体を選ぶため)。
    assert_eq!(
        entries(&menu, EDIT_MENU_TITLE),
        vec![
            standard("切り取り"),
            standard("コピー"),
            standard("ペースト"),
            separator(),
            own(C::EditSelectAll)
        ]
    );
}

fn アプリ名のメニューの最後は自前の終了() {
    let state_dir = support::TempDir::new("menu-app");
    let app = build_app(support::launch_in(&state_dir, false));
    let menu = app.menu().expect("アプリにメニューが付いていない");
    let name = app.package_info().name.clone();

    assert_eq!(menu::QUIT_ID, "app.quit");
    let items = entries(&menu, &name);
    // 標準の「終了」(terminate:)は終了の要求を経由せずに終わるので、確認(FR-063)を挟めない。自前の項目にする。
    assert_eq!(
        items.last(),
        Some(&(menu::QUIT_ID.to_owned(), format!("{name} を終了")))
    );
    // 自前の項目は、先頭の「設定…」と「設定ファイルを開く…」と最後の「終了」だけ。
    // M4.6(T68d。ADR-013 の 6)で、M4.5 の「設定ファイルを開く…」(表の行 `app.open-settings`)の意味を「設定…」に替え、
    // 「設定ファイルを開く…」を表の外の項目として足したので、期待を「設定ファイルを開くと終了だけ」から直した。
    assert_eq!(
        items[..items.len() - 1]
            .iter()
            .filter(|(kind, _)| kind != "標準"
                && kind != C::AppOpenSettings.as_str()
                && kind != OPEN_SETTINGS_FILE_ID)
            .count(),
        0,
        "設定と設定ファイルを開くと終了のほかに自前の項目がある: {items:?}"
    );
}

/// M4.5(T65d)の `アプリ名のメニューの先頭に設定ファイルを開くがあり表のキーが付く` を、M4.6 の並び(ADR-013 の 6)に直したもの。
/// 表の行 `app.open-settings` は「設定…」(⌘,)になり、「設定ファイルを開く…」は表の外のキーの無い項目としてその下に置く。
fn アプリ名のメニューの先頭は設定と設定ファイルを開くで_設定ファイルを開くにはキーが無い() {
    let state_dir = support::TempDir::new("menu-open-settings");
    let app = build_app(support::launch_in(&state_dir, false));
    let menu = app.menu().expect("アプリにメニューが付いていない");
    let name = app.package_info().name.clone();

    // 文言は固定(macOS の標準の「設定…」。ADR-013 の 6)。「設定…」の ID は表のコマンドの ID。
    assert_eq!(C::AppOpenSettings.as_str(), "app.open-settings");
    assert_eq!(menu::label(C::AppOpenSettings), Some("設定…"));
    assert_eq!(OPEN_SETTINGS_FILE_ID, "app.open-settings-file");
    assert_eq!(OPEN_SETTINGS_FILE_LABEL, "設定ファイルを開く…");
    // 表の外の項目(コマンドの ID に無い。表の行は M4.7 で 32 行になったが、この項目は表に入れない)。
    assert_eq!(ShortcutCommandId::from_id(OPEN_SETTINGS_FILE_ID), None);
    let items = entries(&menu, &name);
    assert_eq!(
        items[..3],
        [own(C::AppOpenSettings), open_settings_file(), separator()],
        "先頭が設定・設定ファイルを開く・区切り線ではない: {items:?}"
    );
    // 作った項目からアクセラレータを読み戻せないので、メニューを作るときに見る表のキーで確かめる。
    let table = app.state::<Shortcuts>().table();
    let key = menu::accelerator(&table, C::AppOpenSettings).expect("アクセラレータが無い");
    assert_eq!(key, table.get(C::AppOpenSettings).accelerator().to_string());
    assert_eq!(key, "Cmd+,");
    assert_eq!(
        Accelerator::from_str(&key).expect("muda が読めない"),
        Accelerator::from_str("cmd+comma").expect("muda が読めない")
    );
    // 「設定ファイルを開く…」にはキーが無い(表のどの行のキーとも重ならない)。
    assert_eq!(menu::OPEN_SETTINGS_FILE_ACCELERATOR, None);

    // 受け口がメニューの項目でない表では「設定…」を載せない。「設定ファイルを開く…」は残り、区切り線は 1 本だけ。
    let table = table_with_receiver(&[(C::AppOpenSettings, ShortcutReceiver::Screen)]);
    let menu = menu::build_with_table(app.handle(), &table).expect("メニューを作れなかった");
    let items = entries(&menu, &name);
    assert!(
        !own_ids(&menu).contains(C::AppOpenSettings.as_str()),
        "受け口が画面なのに載っている: {items:?}"
    );
    assert_eq!(items[..2], [open_settings_file(), separator()], "{items:?}");
}

fn 見出しの並びと既存の項目は変わらない() {
    let state_dir = support::TempDir::new("menu-open-settings-rest");
    let app = build_app(support::launch_in(&state_dir, false));
    let menu = app.menu().expect("アプリにメニューが付いていない");
    let name = app.package_info().name.clone();

    assert_eq!(
        titles(&menu),
        vec![
            name.clone(),
            TERMINAL_MENU_TITLE.to_owned(),
            EDIT_MENU_TITLE.to_owned(),
            VIEW_MENU_TITLE.to_owned(),
        ]
    );
    // アプリ名のメニューは、先頭の 3 つ(設定・設定ファイルを開く・区切り線)を足しただけで、残りは T65d の前と同じ。
    // M4.6(T68d。ADR-013 の 6)で「設定ファイルを開く…」を表の外の項目として「設定…」の下に足したので、期待に 1 つ足した。
    assert_eq!(
        entries(&menu, &name),
        vec![
            own(C::AppOpenSettings),
            open_settings_file(),
            separator(),
            standard(&format!("{name} を隠す")),
            standard("ほかを隠す"),
            standard("すべてを表示"),
            separator(),
            (menu::QUIT_ID.to_owned(), format!("{name} を終了")),
        ]
    );
    assert_eq!(
        entries(&menu, TERMINAL_MENU_TITLE),
        vec![
            own(C::TerminalNewTab),
            own(C::TerminalCloseTab),
            separator(),
            own(C::TerminalPreviousTab),
            own(C::TerminalNextTab)
        ]
    );
    assert_eq!(
        entries(&menu, EDIT_MENU_TITLE),
        vec![
            standard("切り取り"),
            standard("コピー"),
            standard("ペースト"),
            separator(),
            own(C::EditSelectAll)
        ]
    );
    assert_eq!(
        entries(&menu, VIEW_MENU_TITLE),
        [
            vec![
                own(C::ViewToggleDisplayMode),
                separator(),
                own(C::ViewPreviousPage),
                own(C::ViewNextPage),
                separator(),
                own(C::ViewToggleMaximize),
            ],
            // M4.7 のタイルの番号キー(ADR-014 の 4)。
            focus_tile_entries(),
        ]
        .concat()
    );
}

fn メニューの項目のアクセラレータが表の既定のキーと同じ() {
    let table = ShortcutTable::default_table();
    let accelerators: Vec<(&str, Option<String>)> = table
        .bindings()
        .iter()
        .map(|b| (b.command().as_str(), menu::accelerator(&table, b.command())))
        .collect();
    // M2 のメニューのキー(⌘T・⌘W・⌘⇧[・⌘⇧]・⌘A・⌘Q)は変わらない。受け口がメニューの項目でない行はアクセラレータを付けない。
    assert_eq!(
        accelerators,
        vec![
            ("terminal.new-tab", Some("Cmd+T".to_owned())),
            ("terminal.close-tab", Some("Cmd+W".to_owned())),
            ("terminal.previous-tab", Some("Cmd+Shift+[".to_owned())),
            ("terminal.next-tab", Some("Cmd+Shift+]".to_owned())),
            ("edit.select-all", Some("Cmd+A".to_owned())),
            ("app.quit", Some("Cmd+Q".to_owned())),
            // M4.5 で足した行(T64c)。M4.6 で「設定…」に替えた(ADR-013 の 6。アプリ名のメニューの先頭に載せる)。
            ("app.open-settings", Some("Cmd+,".to_owned())),
            ("edit.copy", None),
            ("edit.cut", None),
            ("edit.paste", None),
            ("tree.open", None),
            ("tree.reveal-in-finder", None),
            ("tree.insert-path", None),
            ("tree.copy-relative-path", None),
            ("tree.copy-absolute-path", None),
            ("tree.rename", None),
            ("tree.trash", None),
            ("tree.new-file", None),
            ("tree.new-folder", None),
            // M4 の表示の 4 行(T47a)。受け口がメニューの項目なのでアクセラレータを持つ(メニューに載せるのは T47c)。
            ("view.toggle-display-mode", Some("Cmd+Shift+G".to_owned())),
            ("view.previous-page", Some("Cmd+Alt+ArrowLeft".to_owned())),
            ("view.next-page", Some("Cmd+Alt+ArrowRight".to_owned())),
            ("view.toggle-maximize", Some("Cmd+Shift+Enter".to_owned())),
            // M4.7 のタイルの番号キー(ADR-014 の 4)。
            ("view.focus-tile-1", Some("Cmd+1".to_owned())),
            ("view.focus-tile-2", Some("Cmd+2".to_owned())),
            ("view.focus-tile-3", Some("Cmd+3".to_owned())),
            ("view.focus-tile-4", Some("Cmd+4".to_owned())),
            ("view.focus-tile-5", Some("Cmd+5".to_owned())),
            ("view.focus-tile-6", Some("Cmd+6".to_owned())),
            ("view.focus-tile-7", Some("Cmd+7".to_owned())),
            ("view.focus-tile-8", Some("Cmd+8".to_owned())),
            ("view.focus-tile-9", Some("Cmd+9".to_owned())),
        ]
    );
    for binding in table.by_receiver(ShortcutReceiver::MenuItem) {
        let given = menu::accelerator(&table, binding.command())
            .unwrap_or_else(|| panic!("{} にアクセラレータが無い", binding.command()));
        assert_eq!(given, binding.accelerator().to_string());
    }
}

fn 表のキーの文字列はすべて_muda_のアクセラレータとして読める() {
    // Tauri は MenuItemBuilder::accelerator の文字列を muda の Accelerator::from_str で読み、読めなければ黙って捨てる。
    // 受け口が画面の行も、M4.5 や【仮定】21 で受け口をメニューの項目に替えられるよう、全行を確かめる。
    let table = ShortcutTable::default_table();
    for binding in table.bindings() {
        let text = binding.accelerator().to_string();
        let parsed = Accelerator::from_str(&text).unwrap_or_else(|e| {
            panic!(
                "{} のキー {text:?} を muda が読めない: {e}",
                binding.command()
            )
        });
        // 修飾キーの順を変えた別の書き方も、muda では同じアクセラレータになる(正規の書式で比べてよい)。
        assert_eq!(
            parsed,
            Accelerator::from_str(&text.to_lowercase())
                .unwrap_or_else(|e| panic!("{text:?} の小文字を muda が読めない: {e}")),
        );
    }
    // メニューの項目の行の、muda が読んだ値が M2 のアクセラレータと同じ。
    let m2 = [
        (C::TerminalNewTab, "Cmd+T"),
        (C::TerminalCloseTab, "Cmd+W"),
        (C::TerminalPreviousTab, "Cmd+Shift+["),
        (C::TerminalNextTab, "Cmd+Shift+]"),
        (C::EditSelectAll, "Cmd+A"),
        (C::AppQuit, "Cmd+Q"),
    ];
    for (command, expected) in m2 {
        let given = menu::accelerator(&table, command).expect("アクセラレータが無い");
        assert_eq!(
            Accelerator::from_str(&given).expect("muda が読めない"),
            Accelerator::from_str(expected).expect("muda が読めない"),
            "{command}"
        );
    }
}

fn 受け口がメニューの項目のものだけがメニューに載る() {
    let state_dir = support::TempDir::new("menu-receivers");
    let app = build_app(support::launch_in(&state_dir, false));

    // 既定の表: 自前の項目はちょうど受け口がメニューの項目の行(表示の 4 行を含む。T47c)と、表の外の「設定ファイルを開く…」
    // (M4.6 の T68d で足した。ADR-013 の 6)。画面で受けるツリーのキーは載らない(【仮定】21)。
    let menu = app.menu().expect("アプリにメニューが付いていない");
    let mut expected: BTreeSet<String> = ShortcutTable::default_table()
        .by_receiver(ShortcutReceiver::MenuItem)
        .map(|b| b.command().as_str().to_owned())
        .collect();
    assert!(
        expected.contains(C::AppOpenSettings.as_str()),
        "設定が期待に無い"
    );
    expected.insert(OPEN_SETTINGS_FILE_ID.to_owned());
    assert_eq!(own_ids(&menu), expected);
    for view in [
        C::ViewToggleDisplayMode,
        C::ViewPreviousPage,
        C::ViewNextPage,
        C::ViewToggleMaximize,
    ] {
        assert!(expected.contains(view.as_str()), "{view} が期待に無い");
    }

    // 受け口を画面に替えた行はメニューに載らず、区切り線が余らない。
    let table = table_with_receiver(&[
        (C::TerminalPreviousTab, ShortcutReceiver::Screen),
        (C::TerminalNextTab, ShortcutReceiver::Screen),
        (C::EditSelectAll, ShortcutReceiver::Screen),
        (C::EditCut, ShortcutReceiver::Screen),
        (C::ViewPreviousPage, ShortcutReceiver::Screen),
        (C::ViewNextPage, ShortcutReceiver::Screen),
    ]);
    let menu = menu::build_with_table(app.handle(), &table).expect("メニューを作れなかった");
    let ids = own_ids(&menu);
    for gone in [
        C::TerminalPreviousTab,
        C::TerminalNextTab,
        C::EditSelectAll,
        C::ViewPreviousPage,
        C::ViewNextPage,
    ] {
        assert!(!ids.contains(gone.as_str()), "{gone} が載っている: {ids:?}");
    }
    assert_eq!(
        entries(&menu, TERMINAL_MENU_TITLE),
        vec![own(C::TerminalNewTab), own(C::TerminalCloseTab)]
    );
    assert_eq!(
        entries(&menu, EDIT_MENU_TITLE),
        vec![standard("コピー"), standard("ペースト")]
    );
    // 「表示」の真ん中の組が空になっても、区切り線は 1 本だけ。
    assert_eq!(
        entries(&menu, VIEW_MENU_TITLE),
        [
            vec![
                own(C::ViewToggleDisplayMode),
                separator(),
                own(C::ViewToggleMaximize),
            ],
            focus_tile_entries(),
        ]
        .concat()
    );
}

fn メニューと_get_shortcuts_は管理状態の同じ表を見る() {
    let state_dir = support::TempDir::new("menu-shortcuts-state");
    let app = build_app(support::launch_in(&state_dir, false));
    // 設定ファイルの無い状態フォルダなので既定の表(設定を当てた表は app_settings.rs。T65b)。
    let shortcuts = app.state::<Shortcuts>();
    assert_eq!(shortcuts.table(), ShortcutTable::default_table());
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

fn get_shortcuts_が既定の表を返す() {
    let state_dir = support::TempDir::new("menu-get-shortcuts");
    run_app(support::launch_in(&state_dir, false), |handle| {
        let window = handle
            .webview_windows()
            .into_values()
            .next()
            .expect("ウィンドウが無い");
        let body = get_ipc_response(&window, support::request(handle, "get_shortcuts"))
            .unwrap_or_else(|e| panic!("get_shortcuts が失敗した: {e}"));
        let given: serde_json::Value = body
            .deserialize()
            .unwrap_or_else(|e| panic!("JSON として読めなかった: {e}"));
        let expected: serde_json::Value =
            serde_json::from_str(&ShortcutTable::default_table().to_json())
                .expect("既定の表の JSON を読めなかった");
        assert_eq!(given, expected);
        // 画面の偽の Rust 側と Vitest が使う生成物とも同じ(生成物が古ければ generated_types.rs も失敗する)。
        let generated: serde_json::Value = serde_json::from_str(GENERATED_DEFAULT_SHORTCUTS)
            .expect("生成した既定の表を読めなかった");
        assert_eq!(given, generated);
        assert_eq!(
            given[0],
            json!({ "command": "terminal.new-tab", "accelerator": "Cmd+T", "receiver": "menu-item" })
        );
    });
}

fn 新規タブの項目で選択中のプロジェクトにタブが増え_前後のタブで表示中が回る() {
    let fx = ProjectFixture::new("menu-new-tab");
    run_app(fx.launch(), |handle| {
        let (before, first_active) = tabs(handle);
        assert_eq!(before.len(), 1, "起動時に 1 タブ開いていない");

        menu::on_menu_event(handle, C::TerminalNewTab.as_str());
        let (after, active) = tabs(handle);
        assert_eq!(after.len(), 2, "新規タブでタブが増えない");
        assert_eq!(after[0], before[0]);
        assert_eq!(
            active.as_deref(),
            Some(after[1].as_str()),
            "新しいタブが表示中でない"
        );

        menu::on_menu_event(handle, C::TerminalPreviousTab.as_str());
        assert_eq!(tabs(handle).1, first_active, "前のタブに移らない");
        // 先頭の前は末尾に回り込む(【仮定】17)。
        menu::on_menu_event(handle, C::TerminalPreviousTab.as_str());
        assert_eq!(tabs(handle).1.as_deref(), Some(after[1].as_str()));
        menu::on_menu_event(handle, C::TerminalNextTab.as_str());
        assert_eq!(tabs(handle).1, first_active, "次のタブで先頭に回り込まない");
    });
}

fn タブを閉じるの項目で表示中のタブが閉じ_0_枚のときは何もしない() {
    let fx = ProjectFixture::new("menu-close-tab");
    run_app(fx.launch(), |handle| {
        let close_tab = C::TerminalCloseTab.as_str();
        menu::on_menu_event(handle, C::TerminalNewTab.as_str());
        let (two, _) = tabs(handle);
        assert_eq!(two.len(), 2);

        // 表示中(新しいほう)が閉じ、左隣が表示中になる(【仮定】17)。閉じるのは待ってよいスレッドで行う。
        menu::on_menu_event(handle, close_tab);
        wait_until("表示中のタブが閉じる", || {
            tabs(handle).0.len() == 1
        });
        assert_eq!(tabs(handle), (vec![two[0].clone()], Some(two[0].clone())));

        menu::on_menu_event(handle, close_tab);
        wait_until("最後のタブが閉じる", || tabs(handle).0.is_empty());

        // 0 枚: 閉じる・前後は何もしない。新しいタブは自動で開かない(C-21)。
        menu::on_menu_event(handle, close_tab);
        menu::on_menu_event(handle, C::TerminalPreviousTab.as_str());
        menu::on_menu_event(handle, C::TerminalNextTab.as_str());
        thread::sleep(Duration::from_millis(100));
        assert_eq!(tabs(handle), (vec![], None));
        assert!(snapshot(handle).terminals.is_empty());

        // 0 枚から新規タブで開ける(C-21)。
        menu::on_menu_event(handle, C::TerminalNewTab.as_str());
        assert_eq!(tabs(handle).0.len(), 1);
    });
}

fn プロジェクトが無いときタブの項目は何もしない() {
    let state_dir = support::TempDir::new("menu-no-project");
    run_app(support::launch_in(&state_dir, false), |handle| {
        for command in [
            C::TerminalNewTab,
            C::TerminalCloseTab,
            C::TerminalPreviousTab,
            C::TerminalNextTab,
        ] {
            menu::on_menu_event(handle, command.as_str());
        }
        menu::on_menu_event(handle, "no-such-item");
        // 画面で受けるコマンドの ID がメニューから届いても何もしない。
        menu::on_menu_event(handle, C::TreeNewFile.as_str());
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

        menu::on_menu_event(handle, C::EditSelectAll.as_str());

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

/// 「設定…」(キーでもメニューでも同じ受け口)は画面へ `menu-command` の `"open-settings"` を送る(ADR-013 の 6。AC-F01 の Rust 側)。
/// 設定ファイルは開かず・作らず、状態も変えない。
fn 設定を選ぶと_open_settings_のメニューの操作が届く() {
    let fx = ProjectFixture::new("menu-open-settings-command");
    let settings_file = fx.root.path().join("state").join("settings.toml");
    run_app(fx.launch(), move |handle| {
        let received: Arc<Mutex<Vec<String>>> = Arc::default();
        let sink = Arc::clone(&received);
        handle.listen(MENU_COMMAND_EVENT, move |event| {
            sink.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(event.payload().to_owned());
        });
        let before = snapshot(handle);

        menu::on_menu_event(handle, C::AppOpenSettings.as_str());

        wait_until("menu-command が届く", || {
            !received
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .is_empty()
        });
        // 見本を作る処理は別のスレッドで動くので、少し待ってから作っていないことを確かめる。
        thread::sleep(Duration::from_millis(200));
        let payloads = received
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        assert_eq!(payloads, vec!["\"open-settings\"".to_owned()]);
        let command: MenuCommand =
            serde_json::from_str(&payloads[0]).expect("MenuCommand として読めなかった");
        assert_eq!(command, MenuCommand::OpenSettings);
        assert_eq!(snapshot(handle), before, "設定で状態が変わった");
        assert!(!settings_file.exists(), "設定で設定ファイル(見本)を作った");
    });
}

fn display(handle: &AppHandle<MockRuntime>) -> arin_core::workspace::DisplaySnapshot {
    snapshot(handle).display
}

/// 選択中のプロジェクトのタブを、新規タブの項目で `count` 枚になるまで開く。
fn open_tabs(handle: &AppHandle<MockRuntime>, count: usize) {
    while tabs(handle).0.len() < count {
        menu::on_menu_event(handle, C::TerminalNewTab.as_str());
    }
}

fn 表示の項目を選ぶと写しの表示が変わる() {
    let fx = ProjectFixture::new("menu-view-select");
    run_app(fx.launch(), |handle| {
        // 10 枚で 2 ページ(1 ページは 9 枚。C-11)。
        open_tabs(handle, 10);
        assert_eq!(display(handle).mode, DisplayMode::Tabs);

        // タイル表示とタブ表示を切り替える。フォーカス中(最後に開いた 10 枚目)のページで開く。
        menu::on_menu_event(handle, C::ViewToggleDisplayMode.as_str());
        let d = display(handle);
        assert_eq!(d.mode, DisplayMode::Tiles);
        assert_eq!((d.page, d.page_count), (1, 2));

        // 前のページ・次のページ。端では何もしない(【仮定】3)。
        menu::on_menu_event(handle, C::ViewPreviousPage.as_str());
        assert_eq!(display(handle).page, 0, "前のページに移らない");
        menu::on_menu_event(handle, C::ViewPreviousPage.as_str());
        assert_eq!(display(handle).page, 0, "最初のページの前に移った");
        menu::on_menu_event(handle, C::ViewNextPage.as_str());
        assert_eq!(display(handle).page, 1, "次のページに移らない");
        let last = display(handle);
        menu::on_menu_event(handle, C::ViewNextPage.as_str());
        assert_eq!(display(handle), last, "最後のページの次で変わった");

        // 拡大と元に戻す(同じ項目)。
        menu::on_menu_event(handle, C::ViewToggleMaximize.as_str());
        let d = display(handle);
        assert!(d.maximized, "拡大しない");
        assert_eq!(d.focused_terminal_id, last.focused_terminal_id);
        menu::on_menu_event(handle, C::ViewToggleMaximize.as_str());
        assert!(!display(handle).maximized, "元に戻らない");

        // タブ表示に戻す。
        menu::on_menu_event(handle, C::ViewToggleDisplayMode.as_str());
        assert_eq!(display(handle).mode, DisplayMode::Tabs);
    });
}

fn タブ表示でページ送りと拡大の項目を選んでも何も変わらない() {
    let fx = ProjectFixture::new("menu-view-tabs");
    run_app(fx.launch(), |handle| {
        open_tabs(handle, 10);
        let before = snapshot(handle);
        assert_eq!(before.display.mode, DisplayMode::Tabs);

        // 項目は無効にせず並べる。タブ表示では選んでも何もしない(【仮定】9)。
        for command in [C::ViewPreviousPage, C::ViewNextPage, C::ViewToggleMaximize] {
            menu::on_menu_event(handle, command.as_str());
        }
        assert_eq!(snapshot(handle), before, "タブ表示で状態が変わった");
    });
}

fn タイルの番号の項目を選ぶと表示中のページのそのタイルにフォーカスが移る() {
    let fx = ProjectFixture::new("menu-view-focus-tile");
    run_app(fx.launch(), |handle| {
        // 12 枚で 2 ページ。
        open_tabs(handle, 12);
        let ids = tabs(handle).0;

        // タブ表示では何もしない(ADR-014 の 4)。
        let before = snapshot(handle);
        menu::on_menu_event(handle, C::ViewFocusTile1.as_str());
        assert_eq!(snapshot(handle), before, "タブ表示で状態が変わった");

        // タイル表示。フォーカス中(最後に開いた 12 枚目)のページ(2 ページ目)で開く。
        menu::on_menu_event(handle, C::ViewToggleDisplayMode.as_str());
        let d = display(handle);
        assert_eq!((d.mode, d.page), (DisplayMode::Tiles, 1));

        // 2 ページ目の 1 番は全体の 10 枚目。
        menu::on_menu_event(handle, C::ViewFocusTile1.as_str());
        let d = display(handle);
        assert_eq!(
            d.focused_terminal_id
                .as_ref()
                .map(|t| t.as_str().to_owned()),
            Some(ids[9].clone())
        );
        assert_eq!(d.page, 1);

        // 無い番号(2 ページ目は 3 枚)は何もしない。
        let before = snapshot(handle);
        menu::on_menu_event(handle, C::ViewFocusTile4.as_str());
        menu::on_menu_event(handle, C::ViewFocusTile9.as_str());
        assert_eq!(snapshot(handle), before, "無い番号で状態が変わった");

        // 前のページの 9 番は全体の 9 枚目。
        menu::on_menu_event(handle, C::ViewPreviousPage.as_str());
        menu::on_menu_event(handle, C::ViewFocusTile9.as_str());
        let d = display(handle);
        assert_eq!(
            d.focused_terminal_id
                .as_ref()
                .map(|t| t.as_str().to_owned()),
            Some(ids[8].clone())
        );

        // 拡大中は何もしない。
        menu::on_menu_event(handle, C::ViewToggleMaximize.as_str());
        let before = snapshot(handle);
        assert!(before.display.maximized);
        menu::on_menu_event(handle, C::ViewFocusTile1.as_str());
        assert_eq!(snapshot(handle), before, "拡大中に状態が変わった");
    });
}

fn main() {
    let tests: [(&str, fn()); 23] = [
        (
            "アプリ名のメニューの先頭は設定と設定ファイルを開くで_設定ファイルを開くにはキーが無い",
            アプリ名のメニューの先頭は設定と設定ファイルを開くで_設定ファイルを開くにはキーが無い,
        ),
        (
            "設定を選ぶと_open_settings_のメニューの操作が届く",
            設定を選ぶと_open_settings_のメニューの操作が届く,
        ),
        (
            "見出しの並びと既存の項目は変わらない",
            見出しの並びと既存の項目は変わらない,
        ),
        (
            "アプリのメニューは本アプリの見出しだけで既定のメニューではない",
            アプリのメニューは本アプリの見出しだけで既定のメニューではない,
        ),
        (
            "見出しの並びは_arin_ターミナル_編集_表示",
            見出しの並びは_arin_ターミナル_編集_表示,
        ),
        (
            "メニューに新規タブ_タブを閉じる_前後のタブがある",
            メニューに新規タブ_タブを閉じる_前後のタブがある,
        ),
        (
            "編集に切り取り_コピー_ペースト_すべてを選択がこの順である",
            編集に切り取り_コピー_ペースト_すべてを選択がこの順である,
        ),
        ("アプリ名のメニューの最後は自前の終了", アプリ名のメニューの最後は自前の終了),
        (
            "メニューの項目のアクセラレータが表の既定のキーと同じ",
            メニューの項目のアクセラレータが表の既定のキーと同じ,
        ),
        (
            "表示に切替_前のページ_次のページ_拡大_タイルの番号がこの順である",
            表示に切替_前のページ_次のページ_拡大_タイルの番号がこの順である,
        ),
        (
            "表示の項目のアクセラレータが表の既定のキーと同じ",
            表示の項目のアクセラレータが表の既定のキーと同じ,
        ),
        (
            "受け口がメニューの項目の行にはすべて文言がある",
            受け口がメニューの項目の行にはすべて文言がある,
        ),
        (
            "表のキーの文字列はすべて_muda_のアクセラレータとして読める",
            表のキーの文字列はすべて_muda_のアクセラレータとして読める,
        ),
        (
            "受け口がメニューの項目のものだけがメニューに載る",
            受け口がメニューの項目のものだけがメニューに載る,
        ),
        (
            "メニューと_get_shortcuts_は管理状態の同じ表を見る",
            メニューと_get_shortcuts_は管理状態の同じ表を見る,
        ),
        ("get_shortcuts_が既定の表を返す", get_shortcuts_が既定の表を返す),
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
        (
            "表示の項目を選ぶと写しの表示が変わる",
            表示の項目を選ぶと写しの表示が変わる,
        ),
        (
            "タブ表示でページ送りと拡大の項目を選んでも何も変わらない",
            タブ表示でページ送りと拡大の項目を選んでも何も変わらない,
        ),
        (
            "タイルの番号の項目を選ぶと表示中のページのそのタイルにフォーカスが移る",
            タイルの番号の項目を選ぶと表示中のページのそのタイルにフォーカスが移る,
        ),
    ];
    for (name, test) in tests {
        test();
        println!("test {name} ... ok");
    }
}
