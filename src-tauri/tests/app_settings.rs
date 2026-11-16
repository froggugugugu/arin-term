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
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::str::FromStr;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::{self, ThreadId};
use std::time::{Duration, Instant, SystemTime};

use arin_core::ports::desktop::{DesktopError, DesktopPort};
use arin_core::ports::watch::{ActiveWatch, WatchError, WatchSink, WatcherPort};
use arin_core::ports::{PtyError, PtyPort, PtySize, SpawnedPty};
use arin_core::shortcuts::key::KeyChord;
use arin_core::shortcuts::sample::sample_settings_toml;
use arin_core::shortcuts::table::{
    ShortcutBinding, ShortcutCommandId, ShortcutReceiver, ShortcutTable,
};
use arin_core::terminal::ShellLaunchSpec;
use arin_lib::TreePorts;
use arin_lib::events::SETTINGS_CHANGED_EVENT;
use arin_lib::launch::LaunchConfig;
use arin_lib::menu::{self, OPEN_SETTINGS_FILE_ID, Shortcuts};
use arin_lib::persistence::PersistencePorts;
use arin_lib::projects::ProjectPorts;
use arin_lib::sessions::ShellDefaults;
use arin_lib::settings::{
    AppSettings, DIAGNOSTIC_PREFIX, NOTICE_NO_APP_REVEALED, NOTICE_NOT_OPENED,
    NOTICE_NOT_PLAIN_REVEALED, OpenOutcome,
};
use arin_platform::desktop::MacFilePasteboard;
use arin_platform::fs::LocalFileSystem;
use muda::accelerator::Accelerator;
use tauri::menu::{Menu, MenuItemKind};
use tauri::test::{MockRuntime, get_ipc_response};
use tauri::{App, AppHandle, Listener, Manager, RunEvent};

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
    finish(arin_lib::builder_with_pty::<MockRuntime>(
        launch,
        Arc::new(NoPty),
        Arc::new(|| ShellDefaults {
            shell: PathBuf::from("/bin/zsh"),
            locale: None,
        }),
    ))
}

/// 組み立てを仕上げる(`generate_context!` は 1 つのテストのバイナリで 1 か所だけ書ける)。
fn finish(builder: tauri::Builder<MockRuntime>) -> App<MockRuntime> {
    builder
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

/// 既定の表のときにメニューに載る自前の項目の ID(「設定…」を含む)。
///
/// M4.6(T68d。ADR-013 の 6)で、表の外のキーの無い項目「設定ファイルを開く…」(`app.open-settings-file`)が増えたので足した
/// (M4.5 では表の行 `app.open-settings` が「設定ファイルを開く…」で、表の行だけだった)。
fn default_own_ids() -> BTreeSet<String> {
    let mut ids: BTreeSet<String> = ShortcutTable::default_table()
        .by_receiver(ShortcutReceiver::MenuItem)
        .map(|b| b.command().as_str().to_owned())
        .collect();
    ids.insert(OPEN_SETTINGS_FILE_ID.to_owned());
    ids
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

/// AC-F11(ADR-013 の 6): `"app.open-settings" = "Cmd+Shift+,"` で、「設定…」の項目のキーが ⇧⌘, になり、⌘, はどの項目にも付かない。
/// 「設定ファイルを開く…」はキーが無いまま(設定ファイルの行で変えられない。表の外の項目)。
fn 設定で替えたキーが設定のメニューの項目に付く() {
    let state_dir = support::TempDir::new("settings-open-settings-key");
    write_settings(
        state_dir.path(),
        "[shortcuts]\n\"app.open-settings\" = \"Cmd+Shift+,\"\n",
    );
    let app = build_app(support::launch_in(&state_dir, false));
    let table = app.state::<Shortcuts>().table();
    let given = menu::accelerator(&table, C::AppOpenSettings).expect("アクセラレータが無い");
    assert_eq!(given, "Cmd+Shift+,");
    assert_eq!(muda(&given), muda("shift+cmd+comma"));
    let plain = muda("Cmd+,");
    for binding in table.by_receiver(ShortcutReceiver::MenuItem) {
        let key = menu::accelerator(&table, binding.command()).expect("アクセラレータが無い");
        assert_ne!(muda(&key), plain, "{} に ⌘, が付いた", binding.command());
    }
    assert_eq!(menu::OPEN_SETTINGS_FILE_ACCELERATOR, None);
    // 項目は既定と同じ ID で載る。
    let menu = app.menu().expect("アプリにメニューが付いていない");
    assert_eq!(own_ids(&menu), default_own_ids());
    drop(app);

    run_app(support::launch_in(&state_dir, false), |handle| {
        let json = get_shortcuts(handle);
        assert_eq!(accelerator_in(&json, C::AppOpenSettings), "Cmd+Shift+,");
        assert_eq!(
            get_settings_notice(handle),
            None,
            "問題の無い行が知らせに出た"
        );
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

// ---------------------------------------------------------------------------
// 「設定ファイルを開く…」(T65d、ADR-012 の 6、AC-K08)
// ---------------------------------------------------------------------------

const WAIT: Duration = Duration::from_secs(5);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// 偽のデスクトップに届いた頼み。
#[derive(Debug, Clone, PartialEq, Eq)]
enum DesktopCall {
    Reveal(PathBuf),
    Open(PathBuf),
}

/// Finder もアプリも開かず、頼まれたパスと呼ばれたスレッドを記録する。`open_fails`・`reveal_fails` が真なら失敗を返す。
#[derive(Default)]
struct FakeDesktop {
    calls: Mutex<Vec<DesktopCall>>,
    threads: Mutex<Vec<ThreadId>>,
    open_fails: bool,
    reveal_fails: bool,
}

impl FakeDesktop {
    fn failing_open() -> Self {
        Self {
            open_fails: true,
            ..Self::default()
        }
    }

    fn calls(&self) -> Vec<DesktopCall> {
        lock(&self.calls).clone()
    }

    fn record(&self, call: DesktopCall, fails: bool) -> Result<(), DesktopError> {
        lock(&self.calls).push(call);
        lock(&self.threads).push(thread::current().id());
        // 本物の失敗の文言は絶対パスを含む。知らせに出ないことを確かめるため、同じ形にする。
        if fails {
            Err(DesktopError::Failed("/Users/someone/秘密のパス".into()))
        } else {
            Ok(())
        }
    }
}

impl DesktopPort for FakeDesktop {
    fn reveal_in_finder(&self, path: &Path) -> Result<(), DesktopError> {
        self.record(DesktopCall::Reveal(path.to_path_buf()), self.reveal_fails)
    }

    fn open_with_default_app(&self, path: &Path) -> Result<(), DesktopError> {
        self.record(DesktopCall::Open(path.to_path_buf()), self.open_fails)
    }
}

/// どこにも張らない監視(ツリーと設定ファイルの監視。読み直しは各テストが自分で頼む)。
struct NoWatcher;

impl WatcherPort for NoWatcher {
    fn watch(&self, _root: &Path, _sink: WatchSink) -> Result<Box<dyn ActiveWatch>, WatchError> {
        Err(WatchError("このテストでは監視しない".into()))
    }
}

/// デスクトップを偽物にし、監視を張らずに組み立てて起動する。準備ができたら `on_ready` を呼び、終了まで進める。
fn run_with_desktop(
    state_dir: &Path,
    desktop: Arc<FakeDesktop>,
    on_ready: impl FnOnce(&AppHandle<MockRuntime>) + 'static,
) {
    let launch = LaunchConfig::from_inputs(None, Some(state_dir.as_os_str()), None)
        .unwrap_or_else(|e| panic!("組み立ての設定を作れなかった: {e}"));
    let app = finish(arin_lib::builder_with_settings_watcher::<MockRuntime>(
        launch,
        Arc::new(NoPty),
        Arc::new(|| ShellDefaults {
            shell: PathBuf::from("/bin/zsh"),
            locale: None,
        }),
        PersistencePorts::native(state_dir.to_path_buf()),
        ProjectPorts::native(),
        TreePorts {
            fs: Arc::new(LocalFileSystem::new()),
            watcher: Arc::new(NoWatcher),
            file_pasteboard: Arc::new(MacFilePasteboard::general()),
        },
        desktop as Arc<dyn DesktopPort>,
        Arc::new(NoWatcher),
    ));
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

fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !done() {
        assert!(Instant::now() < deadline, "{what}まで待てなかった");
        thread::sleep(Duration::from_millis(5));
    }
}

/// `settings-changed` を数える。
fn count_settings_changed(handle: &AppHandle<MockRuntime>) -> Arc<Mutex<usize>> {
    let count = Arc::new(Mutex::new(0));
    let seen = Arc::clone(&count);
    handle.listen_any(SETTINGS_CHANGED_EVENT, move |_| *lock(&seen) += 1);
    count
}

/// メニューの「設定ファイルを開く…」を選び、偽のデスクトップに `calls` 件の頼みが届くまで待つ。
/// Finder・アプリへの受け渡しはメインスレッドの外で行う(待つことがあるため)。
///
/// M4.6(T68d。ADR-013 の 6)で、項目の ID を表の行 `app.open-settings`(いまは「設定…」)から、表の外の
/// `app.open-settings-file` に替えた。
fn choose_open_settings(handle: &AppHandle<MockRuntime>, desktop: &FakeDesktop, calls: usize) {
    menu::on_menu_event(handle, OPEN_SETTINGS_FILE_ID);
    wait_until("デスクトップへの頼みが届く", || {
        desktop.calls().len() >= calls
    });
    let main = thread::current().id();
    assert!(
        lock(&desktop.threads).iter().all(|id| *id != main),
        "メインスレッドで Finder・アプリに渡した"
    );
}

fn modified(path: &Path) -> SystemTime {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .expect("更新日時を読めなかった")
}

fn assert_no_path(notice: &str, state_dir: &Path) {
    assert!(
        !notice.contains(&*state_dir.to_string_lossy()) && !notice.contains('/'),
        "知らせにパスが出た: {notice:?}"
    );
}

/// AC-K08・AC-F10。M4.5 の `無いときは見本を作って開く` を、表の外の項目の ID(`choose_open_settings`)で選ぶ形にしたもの。
fn 設定ファイルを開くを選ぶと無いときは見本を作って開く() {
    let state_dir = support::TempDir::new("open-settings-missing");
    let path = state_dir.path().join(SETTINGS);
    let desktop = Arc::new(FakeDesktop::default());
    let fake = Arc::clone(&desktop);
    run_with_desktop(state_dir.path(), Arc::clone(&desktop), move |handle| {
        choose_open_settings(handle, &fake, 1);
        assert_eq!(get_settings_notice(handle), None, "開けたのに知らせが出た");
    });
    assert_eq!(desktop.calls(), vec![DesktopCall::Open(path.clone())]);
    // 見本は既定の表から作った全文(ADR-012 の 6)。0600。
    assert_eq!(
        fs::read_to_string(&path).expect("見本を読めなかった"),
        sample_settings_toml(&ShortcutTable::default_table())
    );
    assert_eq!(
        fs::metadata(&path)
            .expect("見本の情報を読めなかった")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    // 見本をそのまま読むと既定の表で、知らせが無い。
    let paths = support::launch_in(&state_dir, false).paths().clone();
    let (_settings, resolved) = AppSettings::load(&paths);
    assert_eq!(resolved.table, ShortcutTable::default_table());
    assert_eq!(resolved.notice, None);
}

fn あるときは中身と更新日時を変えずに開く() {
    let state_dir = support::TempDir::new("open-settings-existing");
    let text = "[shortcuts]\n\"terminal.new-tab\" = \"Cmd+Shift+T\" # 自分のメモ\n";
    write_settings(state_dir.path(), text);
    let path = state_dir.path().join(SETTINGS);
    // 権限も変えない(見本の 0600 に揃えない)。
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("権限を変えられなかった");
    let before = modified(&path);
    thread::sleep(Duration::from_millis(20));

    let desktop = Arc::new(FakeDesktop::default());
    let fake = Arc::clone(&desktop);
    run_with_desktop(state_dir.path(), Arc::clone(&desktop), move |handle| {
        choose_open_settings(handle, &fake, 1);
        assert_eq!(get_settings_notice(handle), None);
        // 開いても表は読み込んだときのまま。
        assert_eq!(
            menu::accelerator(&handle.state::<Shortcuts>().table(), C::TerminalNewTab).as_deref(),
            Some("Cmd+Shift+T")
        );
    });
    assert_eq!(desktop.calls(), vec![DesktopCall::Open(path.clone())]);
    assert_eq!(fs::read_to_string(&path).expect("読めなかった"), text);
    assert_eq!(modified(&path), before, "更新日時が変わった");
    assert_eq!(
        fs::metadata(&path)
            .expect("情報を読めなかった")
            .permissions()
            .mode()
            & 0o777,
        0o644
    );
    assert_eq!(state_dir.entries_except_state(), vec![SETTINGS.to_owned()]);
}

/// 設定画面のボタンが呼ぶコマンド `open_settings_file`(引数なし。ADR-013 の 6、AC-F10)。メニューの項目と同じ処理で、
/// 無いときは見本を作って開き、2 回目は中身を変えずに開く。Finder・アプリへの受け渡しはメインスレッドの外で行う。
fn open_settings_file_のコマンドも同じく開く() {
    let state_dir = support::TempDir::new("open-settings-command");
    let path = state_dir.path().join(SETTINGS);
    let desktop = Arc::new(FakeDesktop::default());
    let fake = Arc::clone(&desktop);
    let sample_path = path.clone();
    run_with_desktop(state_dir.path(), Arc::clone(&desktop), move |handle| {
        let window = handle
            .webview_windows()
            .into_values()
            .next()
            .expect("ウィンドウが無い");
        let call = || {
            get_ipc_response(&window, support::request(handle, "open_settings_file"))
                .unwrap_or_else(|e| panic!("open_settings_file が失敗した: {e}"));
        };
        call();
        wait_until("デスクトップへの頼みが届く", || {
            !fake.calls().is_empty()
        });
        let first = fs::read_to_string(&sample_path).expect("見本ができていない");
        assert_eq!(first, sample_settings_toml(&ShortcutTable::default_table()));
        let before = modified(&sample_path);
        thread::sleep(Duration::from_millis(20));

        call();
        wait_until("2 回目の頼みが届く", || fake.calls().len() >= 2);
        assert_eq!(
            fs::read_to_string(&sample_path).expect("読めなかった"),
            first,
            "2 回目で中身が変わった"
        );
        assert_eq!(modified(&sample_path), before, "2 回目で更新日時が変わった");
        let main = thread::current().id();
        assert!(
            lock(&fake.threads).iter().all(|id| *id != main),
            "メインスレッドで Finder・アプリに渡した"
        );
        assert_eq!(get_settings_notice(handle), None);
    });
    assert_eq!(
        desktop.calls(),
        vec![DesktopCall::Open(path.clone()), DesktopCall::Open(path)]
    );
}

/// 「設定…」(表の行 `app.open-settings`)は画面へ設定画面を開く操作を送るだけで、設定ファイルを作らず開かない
/// (M4.5 ではこの ID が「設定ファイルを開く…」だった。ADR-013 の 6)。
fn 設定を選んでも設定ファイルは作らず開かない() {
    let state_dir = support::TempDir::new("open-settings-not-file");
    let desktop = Arc::new(FakeDesktop::default());
    run_with_desktop(state_dir.path(), Arc::clone(&desktop), |handle| {
        menu::on_menu_event(handle, C::AppOpenSettings.as_str());
        thread::sleep(Duration::from_millis(200));
    });
    assert!(desktop.calls().is_empty(), "{:?}", desktop.calls());
    assert!(
        !state_dir.entries().iter().any(|n| n == SETTINGS),
        "設定ファイルができた"
    );
}

fn 開けないと_finder_で表示し知らせる() {
    let state_dir = support::TempDir::new("open-settings-no-app");
    let path = state_dir.path().join(SETTINGS);
    let desktop = Arc::new(FakeDesktop::failing_open());
    let fake = Arc::clone(&desktop);
    let dir = state_dir.path().to_path_buf();
    run_with_desktop(state_dir.path(), Arc::clone(&desktop), move |handle| {
        let changed = count_settings_changed(handle);
        choose_open_settings(handle, &fake, 2);
        // 知らせが替わったので画面に取り直させる(中身は無い)。
        wait_until("settings-changed が届く", || *lock(&changed) == 1);
        let notice = get_settings_notice(handle).expect("知らせが無い");
        assert_eq!(notice, NOTICE_NO_APP_REVEALED);
        assert_eq!(
            notice,
            "設定ファイルを開けるアプリが無いため、Finder で表示しました"
        );
        assert_no_path(&notice, &dir);
    });
    assert_eq!(
        desktop.calls(),
        vec![DesktopCall::Open(path.clone()), DesktopCall::Reveal(path)]
    );

    // Finder でも表示できなければ、そう知らせる(パスと OS の文言は出さない)。
    let state_dir = support::TempDir::new("open-settings-no-finder");
    let desktop = Arc::new(FakeDesktop {
        open_fails: true,
        reveal_fails: true,
        ..FakeDesktop::default()
    });
    let fake = Arc::clone(&desktop);
    let dir = state_dir.path().to_path_buf();
    run_with_desktop(state_dir.path(), Arc::clone(&desktop), move |handle| {
        choose_open_settings(handle, &fake, 2);
        wait_until("知らせが出る", || {
            get_settings_notice(handle).is_some()
        });
        let notice = get_settings_notice(handle).expect("知らせが無い");
        assert_eq!(notice, NOTICE_NOT_OPENED);
        assert_no_path(&notice, &dir);
    });
}

fn 見本を作れないときは開かず知らせる() {
    let state_dir = support::TempDir::new("open-settings-readonly");
    // 書けない状態フォルダ(見本を作れない)。終わったら戻す(一時フォルダを片付けられるように)。
    fs::set_permissions(state_dir.path(), fs::Permissions::from_mode(0o500))
        .expect("権限を変えられなかった");
    let desktop = Arc::new(FakeDesktop::default());
    let dir = state_dir.path().to_path_buf();
    run_with_desktop(state_dir.path(), Arc::clone(&desktop), move |handle| {
        let changed = count_settings_changed(handle);
        // M4.6(T68d)で項目の ID を表の外の `app.open-settings-file` に替えた(`choose_open_settings` と同じ)。
        menu::on_menu_event(handle, OPEN_SETTINGS_FILE_ID);
        wait_until("知らせが出る", || {
            get_settings_notice(handle).is_some()
        });
        wait_until("settings-changed が届く", || *lock(&changed) == 1);
        let notice = get_settings_notice(handle).expect("知らせが無い");
        assert!(
            notice.starts_with("設定ファイルの見本を作れなかったため、開きませんでした"),
            "{notice:?}"
        );
        assert_no_path(&notice, &dir);
    });
    fs::set_permissions(state_dir.path(), fs::Permissions::from_mode(0o700))
        .expect("権限を戻せなかった");
    assert!(
        desktop.calls().is_empty(),
        "見本が無いのに開いた: {:?}",
        desktop.calls()
    );
    assert!(
        !state_dir.entries().iter().any(|n| n == SETTINGS),
        "設定ファイルができた"
    );
}

fn 開けなかった知らせは設定ファイルの問題の知らせを消さず次の読み直しで戻る() {
    let state_dir = support::TempDir::new("open-settings-notice");
    let paths = support::launch_in(&state_dir, false).paths().clone();
    // 予約のキーの行(行の問題)があるファイル。
    write_settings(
        state_dir.path(),
        "[shortcuts]\n\"terminal.close-tab\" = \"Cmd+H\"\n",
    );
    let (settings, resolved) = AppSettings::load(&paths);
    let file_notice = resolved.notice.clone().expect("行の問題の知らせが無い");
    assert_eq!(settings.notice().as_deref(), Some(file_notice.as_str()));

    // 開けなかった: 開けなかった知らせと、設定ファイルの問題の知らせの両方が 1 行に出る。
    let report = settings.open_file(&FakeDesktop::failing_open());
    assert_eq!(report.outcome, OpenOutcome::Revealed);
    assert!(report.notice_changed);
    let both = settings.notice().expect("知らせが無い");
    assert!(both.contains(NOTICE_NO_APP_REVEALED), "{both:?}");
    assert!(
        both.contains(&file_notice),
        "設定ファイルの問題の知らせが消えた: {both:?}"
    );
    assert!(!both.contains('\n'));

    // 中身の変わらない読み直し(見本の作成や、保存し直し)では、どちらの知らせも消さず、送らない。
    let current = resolved.table.clone();
    assert_eq!(settings.reload_if_changed(&current), None);
    assert_eq!(settings.notice().as_deref(), Some(both.as_str()));

    // 中身が変わった読み直し: 設定ファイルの問題の知らせだけに戻る(開けなかった知らせは古いので外す)。
    write_settings(
        state_dir.path(),
        "[shortcuts]\n\"terminal.close-tab\" = \"Cmd+H\"\n\"edit.select-all\" = 1\n",
    );
    let reloaded = settings
        .reload_if_changed(&current)
        .expect("知らせが変わったのに変わらないと言った");
    assert_eq!(settings.notice(), reloaded.notice);
    let new_file_notice = reloaded.notice.clone().expect("行の問題の知らせが無い");
    assert!(!new_file_notice.contains(NOTICE_NO_APP_REVEALED));

    // 構文エラーに直したときも、構文エラーの知らせに正しく替わる。
    let _ = settings.open_file(&FakeDesktop::failing_open());
    write_settings(state_dir.path(), "[shortcuts\n");
    let broken = settings
        .reload_if_changed(&reloaded.table)
        .expect("構文エラーなのに変わらないと言った");
    let syntax = broken.notice.clone().expect("構文エラーの知らせが無い");
    assert!(syntax.contains("1 行"), "{syntax:?}");
    assert_eq!(settings.notice(), Some(syntax));

    // 開けたら開けなかった知らせだけを外す(設定ファイルの問題の知らせは残る)。
    let _ = settings.open_file(&FakeDesktop::failing_open());
    assert_ne!(settings.notice(), broken.notice);
    let report = settings.open_file(&FakeDesktop::default());
    assert_eq!(report.outcome, OpenOutcome::Opened { created: false });
    assert!(report.notice_changed);
    assert_eq!(settings.notice(), broken.notice);
    let report = settings.open_file(&FakeDesktop::default());
    assert!(!report.notice_changed, "知らせが同じなのに変わったと言った");
}

fn 実行できるかふつうのファイルでない設定ファイルは開かずに_finder_で表示する() {
    let state_dir = support::TempDir::new("open-settings-not-plain");
    let paths = support::launch_in(&state_dir, false).paths().clone();
    let path = state_dir.path().join(SETTINGS);
    // 実行の権限がある(既定のアプリが実行しうる)。中身と権限は変えない。
    write_settings(state_dir.path(), "[shortcuts]\n");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("権限を変えられなかった");
    let (settings, _) = AppSettings::load(&paths);
    let desktop = FakeDesktop::default();
    let report = settings.open_file(&desktop);
    assert_eq!(report.outcome, OpenOutcome::Revealed);
    assert_eq!(desktop.calls(), vec![DesktopCall::Reveal(path.clone())]);
    assert_eq!(
        settings.notice().as_deref(),
        Some(NOTICE_NOT_PLAIN_REVEALED)
    );
    assert_eq!(
        fs::read_to_string(&path).expect("読めなかった"),
        "[shortcuts]\n"
    );
    assert_eq!(
        fs::metadata(&path)
            .expect("情報を読めなかった")
            .permissions()
            .mode()
            & 0o777,
        0o755
    );

    // フォルダ(見本で上書きしない)。
    let state_dir = support::TempDir::new("open-settings-folder");
    let paths = support::launch_in(&state_dir, false).paths().clone();
    let path = state_dir.path().join(SETTINGS);
    fs::create_dir(&path).expect("フォルダを作れなかった");
    let (settings, _) = AppSettings::load(&paths);
    let desktop = FakeDesktop::default();
    assert_eq!(settings.open_file(&desktop).outcome, OpenOutcome::Revealed);
    assert_eq!(desktop.calls(), vec![DesktopCall::Reveal(path.clone())]);
    assert!(path.is_dir(), "フォルダが替わった");
    assert_eq!(fs::read_dir(&path).expect("読めなかった").count(), 0);
}

fn main() {
    let tests: [(&str, fn()); 19] = [
        (
            "実行できるかふつうのファイルでない設定ファイルは開かずに_finder_で表示する",
            実行できるかふつうのファイルでない設定ファイルは開かずに_finder_で表示する,
        ),
        (
            "設定ファイルを開くを選ぶと無いときは見本を作って開く",
            設定ファイルを開くを選ぶと無いときは見本を作って開く,
        ),
        (
            "open_settings_file_のコマンドも同じく開く",
            open_settings_file_のコマンドも同じく開く,
        ),
        (
            "設定を選んでも設定ファイルは作らず開かない",
            設定を選んでも設定ファイルは作らず開かない,
        ),
        (
            "あるときは中身と更新日時を変えずに開く",
            あるときは中身と更新日時を変えずに開く,
        ),
        (
            "開けないと_finder_で表示し知らせる",
            開けないと_finder_で表示し知らせる,
        ),
        (
            "見本を作れないときは開かず知らせる",
            見本を作れないときは開かず知らせる,
        ),
        (
            "開けなかった知らせは設定ファイルの問題の知らせを消さず次の読み直しで戻る",
            開けなかった知らせは設定ファイルの問題の知らせを消さず次の読み直しで戻る,
        ),
        (
            "設定ファイルが無いと既定の表でメニューを作り知らせが無い",
            設定ファイルが無いと既定の表でメニューを作り知らせが無い,
        ),
        (
            "設定で替えたキーがメニューの項目のアクセラレータと_get_shortcuts_の両方に出る",
            設定で替えたキーがメニューの項目のアクセラレータと_get_shortcuts_の両方に出る,
        ),
        (
            "設定で替えたキーが設定のメニューの項目に付く",
            設定で替えたキーが設定のメニューの項目に付く,
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
