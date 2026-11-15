//! メニューバー(設計書 §4 の `menu.rs`、§9.1、FR-043・FR-044、計画書 T33e・決定 (b))。
//!
//! macOS の WKWebView では、⌘C・⌘V はメニューの項目(`copy:`・`paste:`)を通って画面のコピー・ペーストの
//! イベントになる。画面はそのイベントを入口にしてフォーカス位置で振り分ける(`src/ui/terminals/clipboardRouting.ts`)。
//!
//! アプリのショートカット(⌘T・⌘W・⌘⇧[・⌘⇧]・⌘A・⌘Q)は、自前の項目のアクセラレータで受ける(WKWebView より先に
//! OS が受けるため。決定 (b))。項目が選ばれたら `on_menu_event` が、タブの操作は画面を経由せず `SessionRegistry` を
//! 呼び(変わった状態は写しで画面に届く)、「すべてを選択」は画面へ `menu-command` を送り、「終了」は終了の要求
//! (`lifecycle::request_quit`。FR-063 の確認を挟む)にする。
//!
//! Tauri は、メニューを指定しないと macOS で既定のメニュー(`Menu::default`)を付ける。既定のものには ⌘W
//! (ウィンドウを閉じる)や ⌘M などが入っていてタブ操作とぶつかるので使わない。
//! 標準の「終了」(`terminate:`)と「すべてを選択」(`selectAll:`)は使わない。前者は終了の要求
//! (`RunEvent::ExitRequested`)を経由せずにアプリを終わらせ(Tauri 2.12.5 / tao 0.37.2 は
//! `applicationShouldTerminate:` を持たない)、後者は xterm.js ではなく文書全体を選ぶ。

use arin_core::workspace::MenuCommand;
use tauri::menu::{Menu, MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::events::MENU_COMMAND_EVENT;
use crate::lifecycle;
use crate::sessions::SessionRegistry;

/// 自前の項目 1 つ(ID・文言・アクセラレータ)。
///
/// アクセラレータは Tauri(muda)の書式。読めない文字列は Tauri が黙って捨てるので、テストで文字列を確かめる。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuEntry {
    /// 項目の ID(`on_menu_event` に届く)。
    pub id: &'static str,
    /// 文言。
    pub text: &'static str,
    /// アクセラレータ。
    pub accelerator: &'static str,
}

/// 「ターミナル」の見出し。
pub const TERMINAL_MENU_TITLE: &str = "ターミナル";
/// 新規タブ(⌘T)。選択中のプロジェクトの末尾に開く。
pub const NEW_TAB: MenuEntry = MenuEntry {
    id: "terminal.new-tab",
    text: "新規タブ",
    accelerator: "Cmd+T",
};
/// タブを閉じる(⌘W)。表示中のタブを閉じる。
pub const CLOSE_TAB: MenuEntry = MenuEntry {
    id: "terminal.close-tab",
    text: "タブを閉じる",
    accelerator: "Cmd+W",
};
/// 前のタブ(⌘⇧[)。
pub const PREVIOUS_TAB: MenuEntry = MenuEntry {
    id: "terminal.previous-tab",
    text: "前のタブ",
    accelerator: "Cmd+Shift+[",
};
/// 次のタブ(⌘⇧])。
pub const NEXT_TAB: MenuEntry = MenuEntry {
    id: "terminal.next-tab",
    text: "次のタブ",
    accelerator: "Cmd+Shift+]",
};
/// 「ターミナル」に並べる項目(上から順。閉じるの下に区切り線)。
pub const TERMINAL_MENU_ITEMS: [MenuEntry; 4] = [NEW_TAB, CLOSE_TAB, PREVIOUS_TAB, NEXT_TAB];

/// 「編集」の見出し。
pub const EDIT_MENU_TITLE: &str = "編集";
/// 「編集」に並べる項目の文言(上から順)。コピーとペーストは標準の項目、すべてを選択は自前の項目(`SELECT_ALL`)。
pub const EDIT_MENU_ITEMS: [&str; 3] = ["コピー", "ペースト", "すべてを選択"];
/// すべてを選択(⌘A)。画面へ `menu-command` で送り、画面がフォーカス位置で振り分ける。
pub const SELECT_ALL: MenuEntry = MenuEntry {
    id: "edit.select-all",
    text: EDIT_MENU_ITEMS[2],
    accelerator: "Cmd+A",
};

/// 「終了」の項目の ID。文言はアプリ名を入れて作る(「Arin を終了」)。
pub const QUIT_ID: &str = "app.quit";
/// 「終了」のアクセラレータ(⌘Q)。
pub const QUIT_ACCELERATOR: &str = "Cmd+Q";

/// メニューバーを組み立てる。`Builder::menu` に渡す。
///
/// - アプリ名のメニュー: 隠す・ほかを隠す・すべてを表示(標準)・終了(自前。⌘Q)
/// - ターミナル: 新規タブ(⌘T)・タブを閉じる(⌘W)・前のタブ(⌘⇧[)・次のタブ(⌘⇧])
/// - 編集: コピー(⌘C)・ペースト(⌘V)(標準)・すべてを選択(自前。⌘A)
///
/// # Errors
///
/// メニューの部品を作れなかったとき(macOS ではメインスレッド以外で呼んだときなど)。
pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let name = app.package_info().name.clone();
    let quit = MenuItemBuilder::with_id(QUIT_ID, format!("{name} を終了"))
        .accelerator(QUIT_ACCELERATOR)
        .build(app)?;
    let app_menu = SubmenuBuilder::new(app, &name)
        .hide_with_text(format!("{name} を隠す"))
        .hide_others_with_text("ほかを隠す")
        .show_all_with_text("すべてを表示")
        .separator()
        .item(&quit)
        .build()?;

    let [new_tab, close_tab, previous_tab, next_tab] =
        TERMINAL_MENU_ITEMS.map(|entry| item(app, entry));
    let terminal_menu = SubmenuBuilder::new(app, TERMINAL_MENU_TITLE)
        .item(&new_tab?)
        .item(&close_tab?)
        .separator()
        .item(&previous_tab?)
        .item(&next_tab?)
        .build()?;

    let [copy, paste, _] = EDIT_MENU_ITEMS;
    let edit_menu = SubmenuBuilder::new(app, EDIT_MENU_TITLE)
        .copy_with_text(copy)
        .paste_with_text(paste)
        .separator()
        .item(&item(app, SELECT_ALL)?)
        .build()?;

    MenuBuilder::new(app)
        .items(&[&app_menu, &terminal_menu, &edit_menu])
        .build()
}

fn item<R: Runtime>(
    app: &AppHandle<R>,
    entry: MenuEntry,
) -> tauri::Result<tauri::menu::MenuItem<R>> {
    MenuItemBuilder::with_id(entry.id, entry.text)
        .accelerator(entry.accelerator)
        .build(app)
}

/// メニューの項目が選ばれたとき(`Builder::on_menu_event` から。メインスレッドで呼ばれる)。
///
/// - 新規タブ・前後のタブ: `SessionRegistry` を呼ぶ。プロジェクトを選んでいない・見つからない・タブが 0 枚のときは何もしない
/// - タブを閉じる: シェルを終わらせるのに待つことがあるので、待ってよいスレッドで閉じる。0 枚なら何もしない
/// - すべてを選択: 画面へ `menu-command`(`"select-all"`)を送る
/// - 終了: 終了の要求(動作中のターミナルがあれば画面に確認を求める。FR-063)
/// - それ以外の ID(標準の項目など): 何もしない
pub fn on_menu_event<R: Runtime>(app: &AppHandle<R>, id: &str) {
    match id {
        QUIT_ID => lifecycle::request_quit(app),
        id if id == SELECT_ALL.id => {
            // 画面がまだ無いときなどは送れない。送れなくても失うものは無い(押し直せばよい)。
            let _ = app.emit(MENU_COMMAND_EVENT, MenuCommand::SelectAll);
        }
        id if id == CLOSE_TAB.id => {
            if let Some(sessions) = app.try_state::<SessionRegistry>() {
                let sessions = sessions.inner().clone();
                tauri::async_runtime::spawn_blocking(move || sessions.close_active_terminal());
            }
        }
        id => {
            let Some(sessions) = app.try_state::<SessionRegistry>() else {
                return;
            };
            if id == NEW_TAB.id {
                // 開けないとき(プロジェクトを選んでいない・見つからない)は何もしない。
                let _ = sessions.new_terminal();
            } else if id == PREVIOUS_TAB.id {
                sessions.select_previous_terminal();
            } else if id == NEXT_TAB.id {
                sessions.select_next_terminal();
            }
        }
    }
}
