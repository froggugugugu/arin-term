//! メニューバー(設計書 §4 の `menu.rs`、§9.1、FR-043・FR-044・FR-048・FR-052・FR-054、計画書 T33e・T42e・T47c・決定 (b)・前提 (e)・M4 の前提 (f))。
//!
//! 項目の ID とキーはショートカットの表(`arin_core::shortcuts`。T38d)から作り、ここには文言と並びだけを書く
//! (M4.5 で設定ファイルからキーを変えるため)。表はアプリの管理状態の [`Shortcuts`] が 1 つだけ持ち、メニューを作るときと
//! 画面へ渡すとき(`get_shortcuts`)の両方がそれを見る。起動時の表は設定ファイルの値を当てたもの(`settings.rs`。T65b)で、
//! 表を差し替えたら [`rebuild`] でメニューを丸ごと作り直す(ADR-012 の 5)。
//!
//! - 受け口が「メニューの項目」の行: 自前の項目にし、表のキーをアクセラレータにする(WKWebView より先に OS が受けるため。決定 (b))。
//!   項目が選ばれたら `on_menu_event` が、タブの操作と表示の操作は画面を経由せず `SessionRegistry` を呼び(変わった状態は写しで画面に届く)、
//!   「すべてを選択」は画面へ `menu-command` を送り、「終了」は終了の要求(`lifecycle::request_quit`。FR-063 の確認を挟む)にする
//! - 受け口が「編集の項目」の行(切り取り・コピー・ペースト): macOS 標準の項目(`cut:`・`copy:`・`paste:`)にする。WKWebView では
//!   画面の cut・copy・paste イベントになり、画面がフォーカス位置で振り分ける(`src/ui/terminals/clipboardRouting.ts`)。
//!   標準の項目のキーは muda が決める(⌘X・⌘C・⌘V)ので、表のキーは付けられない
//! - 受け口が「画面」の行: メニューに載せない(ツリーにフォーカスがあるときに画面の keydown で受ける。【仮定】21)
//! - 「設定ファイルを開く…」(T65d、ADR-012 の 6): アプリ名のメニューの先頭(区切り線の上)。選ばれたら `settings::open_from_menu`
//!
//! Tauri は、メニューを指定しないと macOS で既定のメニュー(`Menu::default`)を付ける。既定のものには ⌘W
//! (ウィンドウを閉じる)や ⌘M などが入っていてタブ操作とぶつかるので使わない。
//! 標準の「終了」(`terminate:`)と「すべてを選択」(`selectAll:`)は使わない。前者は終了の要求
//! (`RunEvent::ExitRequested`)を経由せずにアプリを終わらせ(Tauri 2.12.5 / tao 0.37.2 は
//! `applicationShouldTerminate:` を持たない)、後者は xterm.js ではなく文書全体を選ぶ。

use std::sync::{PoisonError, RwLock};

use arin_core::shortcuts::table::{ShortcutCommandId, ShortcutReceiver, ShortcutTable};
use arin_core::workspace::MenuCommand;
use tauri::menu::{
    Menu, MenuBuilder, MenuItem, MenuItemBuilder, PredefinedMenuItem, Submenu, SubmenuBuilder,
};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::events::MENU_COMMAND_EVENT;
use crate::lifecycle;
use crate::sessions::SessionRegistry;
use crate::settings;

/// ショートカットの表(アプリの管理状態。`Builder::manage` で登録する)。
///
/// メニューの組み立て(`build`)と画面へ渡すコマンド(`get_shortcuts`)はこれだけを見る。
/// 起動時は設定ファイルの値を当てた表で [`Shortcuts::new`] を呼ぶ(`lib.rs` の 1 か所。T65b)。
/// 動作中に設定が変わったら [`Shortcuts::replace`] で差し替え、[`rebuild`] でメニューを作り直す(T65c)。
#[derive(Debug)]
pub struct Shortcuts {
    table: RwLock<ShortcutTable>,
}

impl Shortcuts {
    /// 表を持たせる。
    #[must_use]
    pub const fn new(table: ShortcutTable) -> Self {
        Self {
            table: RwLock::new(table),
        }
    }

    /// 持っている表の写し(差し替えと並んで読んでも、読んだ時点の表 1 つが丸ごと返る)。
    #[must_use]
    pub fn table(&self) -> ShortcutTable {
        self.table
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// 表を差し替える。前と違えば `true`(同じなら何もしない)。メニューは作り直さない([`rebuild`] を呼ぶ)。
    pub fn replace(&self, table: ShortcutTable) -> bool {
        let mut current = self.table.write().unwrap_or_else(PoisonError::into_inner);
        if *current == table {
            return false;
        }
        *current = table;
        true
    }
}

impl Default for Shortcuts {
    /// 既定の表(`ShortcutTable::default_table`)。
    fn default() -> Self {
        Self::new(ShortcutTable::default_table())
    }
}

/// 「ターミナル」の見出し。
pub const TERMINAL_MENU_TITLE: &str = "ターミナル";
/// 「編集」の見出し。
pub const EDIT_MENU_TITLE: &str = "編集";
/// 「表示」の見出し(「編集」の右。M4 の【仮定】10)。
pub const VIEW_MENU_TITLE: &str = "表示";
/// 「終了」の項目の ID(表のコマンドの ID と同じ)。文言はアプリ名を入れて作る(「Arin を終了」)。
pub const QUIT_ID: &str = ShortcutCommandId::AppQuit.as_str();

use ShortcutCommandId as C;

/// 「ターミナル」の並び。組の間に区切り線を引く(空の組は飛ばす)。
const TERMINAL_GROUPS: &[&[ShortcutCommandId]] = &[
    &[C::TerminalNewTab, C::TerminalCloseTab],
    &[C::TerminalPreviousTab, C::TerminalNextTab],
];
/// 「編集」の並び(macOS の順: 切り取り・コピー・ペースト、区切り線、すべてを選択)。
const EDIT_GROUPS: &[&[ShortcutCommandId]] = &[
    &[C::EditCut, C::EditCopy, C::EditPaste],
    &[C::EditSelectAll],
];
/// 「表示」の並び(表示モードの切替、区切り線、前後のページ、区切り線、拡大)。
const VIEW_GROUPS: &[&[ShortcutCommandId]] = &[
    &[C::ViewToggleDisplayMode],
    &[C::ViewPreviousPage, C::ViewNextPage],
    &[C::ViewToggleMaximize],
];

/// メニューに出す項目の文言。メニューに置き場所の無いコマンド(ツリーの操作)と、文言にアプリ名が入る「終了」は `None`。
#[must_use]
pub const fn label(command: ShortcutCommandId) -> Option<&'static str> {
    match command {
        C::TerminalNewTab => Some("新規タブ"),
        C::TerminalCloseTab => Some("タブを閉じる"),
        C::TerminalPreviousTab => Some("前のタブ"),
        C::TerminalNextTab => Some("次のタブ"),
        C::EditSelectAll => Some("すべてを選択"),
        C::EditCut => Some("切り取り"),
        C::EditCopy => Some("コピー"),
        C::EditPaste => Some("ペースト"),
        C::ViewToggleDisplayMode => Some("タイル表示とタブ表示を切り替える"),
        C::ViewPreviousPage => Some("前のページ"),
        C::ViewNextPage => Some("次のページ"),
        C::ViewToggleMaximize => Some("タイルを拡大/元に戻す"),
        C::AppOpenSettings => Some("設定ファイルを開く…"),
        _ => None,
    }
}

/// メニューの自前の項目に付けるアクセラレータ(Tauri・muda の書式)。受け口がメニューの項目でない行は `None`。
#[must_use]
pub fn accelerator(table: &ShortcutTable, command: ShortcutCommandId) -> Option<String> {
    let binding = table.get(command);
    (binding.receiver() == ShortcutReceiver::MenuItem).then(|| binding.accelerator().to_string())
}

/// メニューバーを組み立てる。`Builder::menu` に渡す。表は管理状態の [`Shortcuts`] から引く
/// (Tauri は `Builder::manage` の状態を入れてからメニューを作る)。
///
/// # Errors
///
/// メニューの部品を作れなかったとき(macOS ではメインスレッド以外で呼んだときなど)。
///
/// # Panics
///
/// [`Shortcuts`] を管理状態に登録していないとき(`lib.rs` の組み立てが必ず登録する)。
pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    build_with_table(app, &app.state::<Shortcuts>().table())
}

/// 管理状態の [`Shortcuts`] の表でメニューバーを作り直し、アプリのメニューを丸ごと差し替える(ADR-012 の 5、前提 (f))。
///
/// 項目ごとの `set_accelerator` は使わない(読めない文字列を黙って捨て、macOS では `None` でキーを消さない)。
/// 項目の ID は表のコマンドの ID のままなので、`on_menu_event` は変わらない。
///
/// **メインスレッドで呼ぶ**(macOS ではメニューの部品をメインスレッドでしか作れない)。別のスレッド
/// (設定ファイルの監視など)からは `AppHandle::run_on_main_thread` で包んで呼ぶ。
///
/// # Errors
///
/// メニューの部品を作れなかった・差し替えられなかったとき。そのときは前のメニューのまま。
///
/// # Panics
///
/// [`Shortcuts`] を管理状態に登録していないとき(`lib.rs` の組み立てが必ず登録する)。
pub fn rebuild<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let menu = build(app)?;
    app.set_menu(menu)?;
    Ok(())
}

/// 表を渡してメニューバーを組み立てる。
///
/// - アプリ名のメニュー: 設定ファイルを開く…(自前。T65d)・隠す・ほかを隠す・すべてを表示(標準)・終了(自前)
/// - ターミナル: 新規タブ・タブを閉じる・前のタブ・次のタブ(自前)
/// - 編集: 切り取り・コピー・ペースト(標準)・すべてを選択(自前)
/// - 表示: タイル表示とタブ表示の切替・前のページ・次のページ・タイルの拡大(自前。T47c)
///
/// 受け口が合わない行(自前の項目なら「メニューの項目」以外、標準の項目なら「編集の項目」以外)は載せない。
///
/// # Errors
///
/// メニューの部品を作れなかったとき。
pub fn build_with_table<R: Runtime>(
    app: &AppHandle<R>,
    table: &ShortcutTable,
) -> tauri::Result<Menu<R>> {
    let name = app.package_info().name.clone();
    let mut app_menu = SubmenuBuilder::new(app, &name);
    if let Some(Entry::Own(open_settings)) = entry(app, table, C::AppOpenSettings)? {
        app_menu = app_menu.item(&open_settings).separator();
    }
    let mut app_menu = app_menu
        .hide_with_text(format!("{name} を隠す"))
        .hide_others_with_text("ほかを隠す")
        .show_all_with_text("すべてを表示");
    if let Some(key) = accelerator(table, C::AppQuit) {
        let quit = MenuItemBuilder::with_id(QUIT_ID, format!("{name} を終了"))
            .accelerator(key)
            .build(app)?;
        app_menu = app_menu.separator().item(&quit);
    }
    let app_menu = app_menu.build()?;
    let terminal_menu = submenu(app, table, TERMINAL_MENU_TITLE, TERMINAL_GROUPS)?;
    let edit_menu = submenu(app, table, EDIT_MENU_TITLE, EDIT_GROUPS)?;
    let view_menu = submenu(app, table, VIEW_MENU_TITLE, VIEW_GROUPS)?;

    MenuBuilder::new(app)
        .items(&[&app_menu, &terminal_menu, &edit_menu, &view_menu])
        .build()
}

/// 見出し 1 つを組み立てる。組の間に区切り線を引き、載せる項目が無い組は飛ばす。
fn submenu<R: Runtime>(
    app: &AppHandle<R>,
    table: &ShortcutTable,
    title: &str,
    groups: &[&[ShortcutCommandId]],
) -> tauri::Result<Submenu<R>> {
    let mut builder = SubmenuBuilder::new(app, title);
    let mut first = true;
    for group in groups {
        let mut items = Vec::new();
        for &command in *group {
            if let Some(item) = entry(app, table, command)? {
                items.push(item);
            }
        }
        if items.is_empty() {
            continue;
        }
        if !first {
            builder = builder.separator();
        }
        first = false;
        for item in items {
            builder = match item {
                Entry::Own(item) => builder.item(&item),
                Entry::Standard(item) => builder.item(&item),
            };
        }
    }
    builder.build()
}

/// 載せる項目 1 つ。
enum Entry<R: Runtime> {
    /// 自前の項目(表のキーをアクセラレータにする)。
    Own(MenuItem<R>),
    /// macOS 標準の項目(キーは muda が決める)。
    Standard(PredefinedMenuItem<R>),
}

/// コマンドの行の受け口に合わせて項目を作る。載せないときは `None`。
fn entry<R: Runtime>(
    app: &AppHandle<R>,
    table: &ShortcutTable,
    command: ShortcutCommandId,
) -> tauri::Result<Option<Entry<R>>> {
    let Some(text) = label(command) else {
        return Ok(None);
    };
    let standard = matches!(command, C::EditCut | C::EditCopy | C::EditPaste);
    let receiver = table.get(command).receiver();
    match (standard, receiver) {
        (true, ShortcutReceiver::EditItem) => {
            let item = match command {
                C::EditCut => PredefinedMenuItem::cut(app, Some(text))?,
                C::EditCopy => PredefinedMenuItem::copy(app, Some(text))?,
                _ => PredefinedMenuItem::paste(app, Some(text))?,
            };
            Ok(Some(Entry::Standard(item)))
        }
        (false, ShortcutReceiver::MenuItem) => {
            let mut builder = MenuItemBuilder::with_id(command.as_str(), text);
            if let Some(key) = accelerator(table, command) {
                builder = builder.accelerator(key);
            }
            Ok(Some(Entry::Own(builder.build(app)?)))
        }
        _ => Ok(None),
    }
}

/// メニューの項目が選ばれたとき(`Builder::on_menu_event` から。メインスレッドで呼ばれる)。
/// 項目の ID は表のコマンドの ID(M2 から変えていない)。
///
/// - 新規タブ・前後のタブ: `SessionRegistry` を呼ぶ。プロジェクトを選んでいない・見つからない・タブが 0 枚のときは何もしない
/// - タブを閉じる: シェルを終わらせるのに待つことがあるので、待ってよいスレッドで閉じる。0 枚なら何もしない
/// - すべてを選択: 画面へ `menu-command`(`"select-all"`)を送る
/// - 表示の 4 項目: `SessionRegistry` の表示の口を呼ぶ(変わったときだけ写しが届く)。タブ表示でのページ送り・拡大、
///   端のページでのページ送りは何もしない。項目はいつも有効のまま並べる(M4 の【仮定】9)
/// - 終了: 終了の要求(動作中のターミナルがあれば画面に確認を求める。FR-063)
/// - 設定ファイルを開く…: 待ってよいスレッドで、無ければ見本を作ってから既定のアプリで開く(`settings::open_from_menu`。T65d)
/// - それ以外の ID(標準の項目・画面で受けるコマンドなど): 何もしない
pub fn on_menu_event<R: Runtime>(app: &AppHandle<R>, id: &str) {
    let Some(command) = ShortcutCommandId::from_id(id) else {
        return;
    };
    match command {
        C::AppQuit => lifecycle::request_quit(app),
        C::AppOpenSettings => settings::open_from_menu(app),
        C::EditSelectAll => {
            // 画面がまだ無いときなどは送れない。送れなくても失うものは無い(押し直せばよい)。
            let _ = app.emit(MENU_COMMAND_EVENT, MenuCommand::SelectAll);
        }
        C::TerminalCloseTab => {
            if let Some(sessions) = app.try_state::<SessionRegistry>() {
                let sessions = sessions.inner().clone();
                tauri::async_runtime::spawn_blocking(move || sessions.close_active_terminal());
            }
        }
        C::TerminalNewTab | C::TerminalPreviousTab | C::TerminalNextTab => {
            let Some(sessions) = app.try_state::<SessionRegistry>() else {
                return;
            };
            // 開けない・移れないとき(プロジェクトを選んでいない・見つからない・タブが 0 枚)は何もしない。
            match command {
                C::TerminalNewTab => {
                    let _ = sessions.new_terminal();
                }
                C::TerminalPreviousTab => {
                    let _ = sessions.select_previous_terminal();
                }
                _ => {
                    let _ = sessions.select_next_terminal();
                }
            }
        }
        C::ViewToggleDisplayMode
        | C::ViewPreviousPage
        | C::ViewNextPage
        | C::ViewToggleMaximize => {
            let Some(sessions) = app.try_state::<SessionRegistry>() else {
                return;
            };
            // 変わらないとき(タブ表示でのページ送り・拡大、端のページ、0 枚)は何もしない。
            let _ = match command {
                C::ViewToggleDisplayMode => sessions.toggle_display_mode(),
                C::ViewPreviousPage => sessions.previous_tile_page(),
                C::ViewNextPage => sessions.next_tile_page(),
                _ => sessions.toggle_tile_maximize(),
            };
        }
        _ => {}
    }
}
