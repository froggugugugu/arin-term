//! 起動時の設定ファイル(`settings.toml`)の読み込みと、メニュー・`get_shortcuts`・知らせへの反映
//! (`src-tauri/src/settings.rs`・`menu.rs`、計画書 T65b、ADR-012 の 3・5、AC-K01〜K05・K09・K10 の Rust 側)を、
//! Tauri の模擬ランタイムで確かめる。
//!
//! 状態フォルダは一時フォルダに差し替え、本物の `~/Library/Application Support/Arin/settings.toml` を読まない。
//! 設定ファイルは読むだけで、書き換え・退避・削除をしないことも確かめる。
//!
//! 作った項目からはアクセラレータを読み戻せない(`app_menu.rs` の冒頭)ので、メニューのキーは「メニューを作るときに
//! 見る管理状態の表(`Shortcuts`)」と、その文字列を muda が読めることで確かめる。
//!
//! macOS ではメニューの部品をメインスレッドでしか作れないため、`harness = false` で走らせる(`app_builder` と同じ)。

mod support;

use std::cell::Cell;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::str::FromStr;
use std::sync::Arc;

use arin_core::ports::{PtyError, PtyPort, PtySize, SpawnedPty};
use arin_core::shortcuts::key::KeyChord;
use arin_core::shortcuts::table::{
    ShortcutBinding, ShortcutCommandId, ShortcutReceiver, ShortcutTable,
};
use arin_core::terminal::ShellLaunchSpec;
use arin_lib::events::SETTINGS_CHANGED_EVENT;
use arin_lib::launch::LaunchConfig;
use arin_lib::menu::{self, Shortcuts};
use arin_lib::sessions::ShellDefaults;
use arin_lib::settings::{AppSettings, DIAGNOSTIC_PREFIX};
use muda::accelerator::Accelerator;
use tauri::menu::{Menu, MenuItemKind};
use tauri::test::{MockRuntime, get_ipc_response};
use tauri::{App, AppHandle, Manager, RunEvent};

use ShortcutCommandId as C;

/// 状態フォルダの中の設定ファイルの名前。
const SETTINGS: &str = "settings.toml";

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

fn write_settings(dir: &Path, text: &str) {
    fs::write(dir.join(SETTINGS), text).expect("設定ファイルを置けなかった");
}

/// 画面から `get_shortcuts` を呼んだ結果(JSON)。
fn get_shortcuts(handle: &AppHandle<MockRuntime>) -> serde_json::Value {
    let window = handle
        .webview_windows()
        .into_values()
        .next()
        .expect("ウィンドウが無い");
    get_ipc_response(&window, support::request(handle, "get_shortcuts"))
        .unwrap_or_else(|e| panic!("get_shortcuts が失敗した: {e}"))
        .deserialize()
        .unwrap_or_else(|e| panic!("JSON として読めなかった: {e}"))
}

/// 画面から `get_settings_notice` を呼んだ結果(1 行か null)。
fn get_settings_notice(handle: &AppHandle<MockRuntime>) -> Option<String> {
    let window = handle
        .webview_windows()
        .into_values()
        .next()
        .expect("ウィンドウが無い");
    get_ipc_response(&window, support::request(handle, "get_settings_notice"))
        .unwrap_or_else(|e| panic!("get_settings_notice が失敗した: {e}"))
        .deserialize()
        .unwrap_or_else(|e| panic!("1 行か null として読めなかった: {e}"))
}

/// `get_shortcuts` の結果から、コマンドのキーの文字列を引く。
fn accelerator_in(json: &serde_json::Value, command: ShortcutCommandId) -> String {
    json.as_array()
        .expect("表が配列ではない")
        .iter()
        .find(|row| row["command"] == command.as_str())
        .unwrap_or_else(|| panic!("{command} の行が無い"))["accelerator"]
        .as_str()
        .expect("キーが文字列ではない")
        .to_owned()
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

/// 既定の表のときにメニューに載る自前の項目の ID。
fn default_own_ids() -> BTreeSet<String> {
    ShortcutTable::default_table()
        .by_receiver(ShortcutReceiver::MenuItem)
        // 「設定ファイルを開く…」は表に行だけある(T64c)。メニューに載せるのは T65d(そのときこの除外を外す)。
        .filter(|b| b.command() != C::AppOpenSettings)
        .map(|b| b.command().as_str().to_owned())
        .collect()
}

/// メニューに渡すアクセラレータの文字列を muda で読んだもの。
fn muda(text: &str) -> Accelerator {
    Accelerator::from_str(text).unwrap_or_else(|e| panic!("{text:?} を muda が読めない: {e}"))
}

fn 設定ファイルが無いと既定の表でメニューを作り知らせが無い() {
    let state_dir = support::TempDir::new("settings-missing");
    let app = build_app(support::launch_in(&state_dir, false));
    assert_eq!(
        app.state::<Shortcuts>().table(),
        ShortcutTable::default_table()
    );
    let menu = app.menu().expect("アプリにメニューが付いていない");
    assert_eq!(own_ids(&menu), default_own_ids());
    drop(app);

    run_app(support::launch_in(&state_dir, false), |handle| {
        let expected: serde_json::Value =
            serde_json::from_str(&ShortcutTable::default_table().to_json())
                .expect("既定の表の JSON を読めなかった");
        assert_eq!(get_shortcuts(handle), expected);
        assert_eq!(get_settings_notice(handle), None);
    });
    // 設定ファイルを作らない(見本を作るのは「設定ファイルを開く…」だけ。T65d)。
    assert!(
        !state_dir.entries().iter().any(|n| n == SETTINGS),
        "起動で設定ファイルを作った"
    );
}

fn 設定で替えたキーがメニューの項目のアクセラレータと_get_shortcuts_の両方に出る() {
    let state_dir = support::TempDir::new("settings-menu-key");
    write_settings(
        state_dir.path(),
        "[shortcuts]\n\"terminal.new-tab\" = \"Cmd+Shift+T\"\n",
    );
    let app = build_app(support::launch_in(&state_dir, false));
    // メニューは管理状態の表から作る(`menu::build`)。その表の新規タブのキーが替わっている。
    let table = app.state::<Shortcuts>().table();
    let given = menu::accelerator(&table, C::TerminalNewTab).expect("アクセラレータが無い");
    assert_eq!(given, "Cmd+Shift+T");
    assert_eq!(muda(&given), muda("shift+cmd+t"));
    assert_ne!(muda(&given), muda("Cmd+T"));
    // 書かなかった行は既定のまま(AC-K03)。
    assert_eq!(
        menu::accelerator(&table, C::TerminalCloseTab).as_deref(),
        Some("Cmd+W")
    );
    // 項目は既定と同じ ID で載る(on_menu_event は変わらない)。
    let menu = app.menu().expect("アプリにメニューが付いていない");
    assert_eq!(own_ids(&menu), default_own_ids());
    drop(app);

    run_app(support::launch_in(&state_dir, false), |handle| {
        let json = get_shortcuts(handle);
        assert_eq!(accelerator_in(&json, C::TerminalNewTab), "Cmd+Shift+T");
        assert_eq!(accelerator_in(&json, C::TerminalCloseTab), "Cmd+W");
        assert_eq!(get_settings_notice(handle), None);
    });
}

fn 画面で受けるコマンドのキーも_get_shortcuts_に出る() {
    let state_dir = support::TempDir::new("settings-screen-key");
    write_settings(
        state_dir.path(),
        "[shortcuts]\n\"tree.new-file\" = \"Cmd+Alt+N\"\n",
    );
    run_app(support::launch_in(&state_dir, false), |handle| {
        let json = get_shortcuts(handle);
        assert_eq!(accelerator_in(&json, C::TreeNewFile), "Cmd+Alt+N");
        // 受け口は変わらない(ADR-012 の 3 の #5)。
        let row = json
            .as_array()
            .expect("表が配列ではない")
            .iter()
            .find(|row| row["command"] == C::TreeNewFile.as_str())
            .expect("行が無い")
            .clone();
        assert_eq!(row["receiver"], "screen");
        // 画面で受けるコマンドはメニューに載らない。
        let menu = handle.menu().expect("アプリにメニューが付いていない");
        assert!(!own_ids(&menu).contains(C::TreeNewFile.as_str()));
        assert_eq!(get_settings_notice(handle), None);
    });
}

fn 問題のある行は既定に戻り_get_settings_notice_に_1_行出る() {
    let state_dir = support::TempDir::new("settings-bad-rows");
    write_settings(
        state_dir.path(),
        concat!(
            "[shortcuts]\n",
            "\"terminal.new-tab\" = \"Cmd+Shift+T\"\n",
            "\"terminal.close-tab\" = \"Cmd+H\"\n",
            "\"edit.copy\" = \"Cmd+Shift+C\"\n",
            "\"no.such-command\" = \"Cmd+J\"\n",
        ),
    );
    run_app(support::launch_in(&state_dir, false), |handle| {
        let json = get_shortcuts(handle);
        // 問題の無い行は使う。
        assert_eq!(accelerator_in(&json, C::TerminalNewTab), "Cmd+Shift+T");
        // 予約のキー・変えられないコマンドの行は既定に戻る(AC-K04)。
        assert_eq!(accelerator_in(&json, C::TerminalCloseTab), "Cmd+W");
        assert_eq!(accelerator_in(&json, C::EditCopy), "Cmd+C");
        let notice = get_settings_notice(handle).expect("知らせが無い");
        assert!(!notice.contains('\n'), "知らせが 1 行ではない: {notice:?}");
        for id in ["terminal.close-tab", "edit.copy", "no.such-command"] {
            assert!(notice.contains(id), "知らせに {id} が無い: {notice:?}");
        }
        assert!(
            !notice.contains("terminal.new-tab"),
            "問題の無い行が知らせに出た: {notice:?}"
        );
        // 写しの知らせ(状態ファイルの退避用)とは別に持つ(前提 (g))。
        let snapshot = handle
            .state::<arin_lib::sessions::SessionRegistry>()
            .snapshot();
        assert_eq!(snapshot.notice, None);
    });
}

fn 構文エラーなら既定の表で行と列を知らせる() {
    let state_dir = support::TempDir::new("settings-syntax");
    write_settings(
        state_dir.path(),
        "[shortcuts]\n\"terminal.new-tab\" = \"Cmd+Shift+T\"\n\"terminal.close-tab\" = \n",
    );
    run_app(support::launch_in(&state_dir, false), |handle| {
        let expected: serde_json::Value =
            serde_json::from_str(&ShortcutTable::default_table().to_json())
                .expect("既定の表の JSON を読めなかった");
        // ファイル全体を読めないので、読めた行も使わない(起動時の「直前」は既定。AC-K05)。
        assert_eq!(get_shortcuts(handle), expected);
        let notice = get_settings_notice(handle).expect("知らせが無い");
        assert!(!notice.contains('\n'), "知らせが 1 行ではない: {notice:?}");
        assert!(
            notice.contains("3 行") && notice.contains("列目"),
            "行と列が無い: {notice:?}"
        );
        assert!(notice.contains("既定のまま"), "{notice:?}");
        assert!(!notice.contains("Cmd+Shift+T"), "中身が出た: {notice:?}");
    });
    assert_eq!(
        build_app(support::launch_in(&state_dir, false))
            .state::<Shortcuts>()
            .table(),
        ShortcutTable::default_table()
    );
}

fn 起動しても設定ファイルの中身と更新日時が変わらず退避のファイルもできない() {
    let state_dir = support::TempDir::new("settings-untouched");
    // 構文エラーのファイル(状態ファイルなら退避される種類の失敗)と、読める行のあるファイルの両方で確かめる。
    for text in [
        "[shortcuts\n\"terminal.new-tab\" = \"Cmd+Shift+T\"\n",
        "[shortcuts]\n\"terminal.new-tab\" = \"Cmd+Shift+T\"\n\"edit.cut\" = 1\n",
    ] {
        write_settings(state_dir.path(), text);
        let path = state_dir.path().join(SETTINGS);
        let before = fs::metadata(&path)
            .and_then(|m| m.modified())
            .expect("更新日時を読めなかった");
        run_app(support::launch_in(&state_dir, false), |handle| {
            let _ = get_settings_notice(handle);
        });
        assert_eq!(
            fs::read_to_string(&path).expect("設定ファイルを読めなかった"),
            text,
            "設定ファイルの中身が変わった"
        );
        assert_eq!(
            fs::metadata(&path)
                .and_then(|m| m.modified())
                .expect("更新日時を読めなかった"),
            before,
            "設定ファイルの更新日時が変わった"
        );
        // 状態ファイル(state.json と書き込み途中のもの)を除くと、設定ファイルだけが残る(退避のファイルが無い)。
        assert_eq!(state_dir.entries_except_state(), vec![SETTINGS.to_owned()]);
    }
}

fn 差し替えた状態フォルダの設定だけを読む() {
    let state_dir = support::TempDir::new("settings-override");
    let other = support::TempDir::new("settings-other");
    // 差し替え先でないフォルダの設定は読まない。
    write_settings(
        other.path(),
        "[shortcuts]\n\"terminal.close-tab\" = \"Cmd+Shift+W\"\n",
    );
    write_settings(
        state_dir.path(),
        "[shortcuts]\n\"terminal.new-tab\" = \"Cmd+Shift+T\"\n",
    );
    let launch = support::launch_in(&state_dir, false);
    assert_eq!(
        launch.paths().settings_file(),
        state_dir.path().join(SETTINGS)
    );
    let app = build_app(launch);
    let table = app.state::<Shortcuts>().table();
    assert_eq!(
        table.get(C::TerminalNewTab).accelerator().to_string(),
        "Cmd+Shift+T"
    );
    assert_eq!(
        table.get(C::TerminalCloseTab).accelerator().to_string(),
        "Cmd+W"
    );
}

fn 標準エラーの行は_arin_settings_で始まり設定の行の全文を含まない() {
    let state_dir = support::TempDir::new("settings-diagnostics");
    let paths = support::launch_in(&state_dir, false).paths().clone();
    assert_eq!(DIAGNOSTIC_PREFIX, "[arin:settings]");

    // 行の問題: 問題ごとに 1 行。
    let line_reserved = "\"terminal.close-tab\" = \"Cmd+H\" # 隠しておきたいメモ";
    let line_not_text = "\"edit.select-all\" = 12345";
    write_settings(
        state_dir.path(),
        &format!(
            "[shortcuts]\n{line_reserved}\n{line_not_text}\n\"terminal.new-tab\" = \"Cmd+Shift+T\"\n"
        ),
    );
    let (_settings, resolved) = AppSettings::load(&paths);
    assert_eq!(resolved.diagnostics.len(), 2, "{:?}", resolved.diagnostics);
    for line in &resolved.diagnostics {
        assert!(
            line.starts_with("[arin:settings] "),
            "頭の文字列が無い: {line:?}"
        );
        assert!(!line.contains('\n'), "1 行ではない: {line:?}");
        for secret in [line_reserved, line_not_text, "隠しておきたいメモ", "12345"] {
            assert!(!line.contains(secret), "設定の行の中身が出た: {line:?}");
        }
        assert!(
            !line.contains(&state_dir.path().to_string_lossy().into_owned()),
            "絶対パスが出た: {line:?}"
        );
    }
    assert!(resolved.diagnostics[0].contains("terminal.close-tab"));
    assert!(resolved.diagnostics[1].contains("edit.select-all"));

    // ファイル全体の問題(構文エラー): 行と列だけ。
    write_settings(
        state_dir.path(),
        "[shortcuts]\n\"terminal.new-tab\" = \"Cmd+Shift+T\" 余計な文字列\n",
    );
    let (_settings, resolved) = AppSettings::load(&paths);
    assert_eq!(resolved.table, ShortcutTable::default_table());
    assert_eq!(resolved.diagnostics.len(), 1, "{:?}", resolved.diagnostics);
    let line = &resolved.diagnostics[0];
    assert!(line.starts_with("[arin:settings] "), "{line:?}");
    assert!(line.contains("2 行"), "行が無い: {line:?}");
    for secret in ["余計な文字列", "Cmd+Shift+T", "terminal.new-tab"] {
        assert!(!line.contains(secret), "設定の行の中身が出た: {line:?}");
    }

    // 問題が無ければ標準エラーに出さない。
    write_settings(
        state_dir.path(),
        "[shortcuts]\n\"terminal.new-tab\" = \"Cmd+Shift+T\"\n",
    );
    let (settings, resolved) = AppSettings::load(&paths);
    assert!(
        resolved.diagnostics.is_empty(),
        "{:?}",
        resolved.diagnostics
    );
    assert_eq!(resolved.notice, None);
    assert_eq!(settings.notice(), None);
}

fn 表を替えて作り直すとメニューのアクセラレータが替わり項目の_id_は変わらない() {
    let state_dir = support::TempDir::new("settings-rebuild");
    let app = build_app(support::launch_in(&state_dir, false));
    let handle = app.handle();
    let before = app.menu().expect("アプリにメニューが付いていない");
    let before_ids = own_ids(&before);

    // 新規タブを ⇧⌘T に、表示の切替を ⌘⌥G にした表。
    let rows = ShortcutTable::default_table()
        .bindings()
        .iter()
        .map(|b| match b.command() {
            C::TerminalNewTab => ShortcutBinding::new(
                b.command(),
                KeyChord::parse("Cmd+Shift+T").expect("キーを読めない"),
                b.receiver(),
            ),
            C::ViewToggleDisplayMode => ShortcutBinding::new(
                b.command(),
                KeyChord::parse("Cmd+Alt+G").expect("キーを読めない"),
                b.receiver(),
            ),
            _ => *b,
        })
        .collect();
    let table =
        ShortcutTable::from_bindings(rows).unwrap_or_else(|e| panic!("表を作れない: {e:?}"));

    let shortcuts = app.state::<Shortcuts>();
    assert!(
        shortcuts.replace(table.clone()),
        "替えたのに変わったと言わない"
    );
    assert!(
        !shortcuts.replace(table.clone()),
        "同じ表で変わったと言った"
    );
    menu::rebuild(handle).expect("メニューを作り直せなかった");

    let after = app.menu().expect("作り直したメニューが付いていない");
    assert_ne!(after.id(), before.id(), "メニューが差し替わっていない");
    assert_eq!(own_ids(&after), before_ids, "項目の ID が変わった");
    let given = shortcuts.table();
    assert_eq!(given, table);
    assert_eq!(
        menu::accelerator(&given, C::TerminalNewTab).as_deref(),
        Some("Cmd+Shift+T")
    );
    assert_eq!(
        menu::accelerator(&given, C::ViewToggleDisplayMode).as_deref(),
        Some("Cmd+Alt+G")
    );
    assert_eq!(muda("Cmd+Alt+G"), muda("alt+cmd+g"));
    // 作り直したあとも、項目が選ばれたときの受け口は同じ ID で動く(タブの項目はプロジェクトが無ければ何もしない)。
    menu::on_menu_event(handle, C::TerminalNewTab.as_str());
}

fn 設定が変わったイベントの名前は_settings_changed() {
    // 画面の `src/ipc/client.ts` の `EventMap` と名前を合わせる(T65e)。中身は無い。
    assert_eq!(SETTINGS_CHANGED_EVENT, "settings-changed");
}

fn main() {
    let tests: [(&str, fn()); 10] = [
        (
            "設定ファイルが無いと既定の表でメニューを作り知らせが無い",
            設定ファイルが無いと既定の表でメニューを作り知らせが無い,
        ),
        (
            "設定で替えたキーがメニューの項目のアクセラレータと_get_shortcuts_の両方に出る",
            設定で替えたキーがメニューの項目のアクセラレータと_get_shortcuts_の両方に出る,
        ),
        (
            "画面で受けるコマンドのキーも_get_shortcuts_に出る",
            画面で受けるコマンドのキーも_get_shortcuts_に出る,
        ),
        (
            "問題のある行は既定に戻り_get_settings_notice_に_1_行出る",
            問題のある行は既定に戻り_get_settings_notice_に_1_行出る,
        ),
        (
            "構文エラーなら既定の表で行と列を知らせる",
            構文エラーなら既定の表で行と列を知らせる,
        ),
        (
            "起動しても設定ファイルの中身と更新日時が変わらず退避のファイルもできない",
            起動しても設定ファイルの中身と更新日時が変わらず退避のファイルもできない,
        ),
        (
            "差し替えた状態フォルダの設定だけを読む",
            差し替えた状態フォルダの設定だけを読む,
        ),
        (
            "標準エラーの行は_arin_settings_で始まり設定の行の全文を含まない",
            標準エラーの行は_arin_settings_で始まり設定の行の全文を含まない,
        ),
        (
            "表を替えて作り直すとメニューのアクセラレータが替わり項目の_id_は変わらない",
            表を替えて作り直すとメニューのアクセラレータが替わり項目の_id_は変わらない,
        ),
        (
            "設定が変わったイベントの名前は_settings_changed",
            設定が変わったイベントの名前は_settings_changed,
        ),
    ];
    for (name, test) in tests {
        test();
        println!("test {name} ... ok");
    }
    println!("test result: ok. {} passed", tests.len());
}
