//! メニューバー(設計書 §4 の `menu.rs`、§9.1、FR-043)。
//!
//! macOS の WKWebView では、⌘C・⌘V はメニューの項目(`copy:`・`paste:`)を通って画面のコピー・ペーストの
//! イベントになる。画面はそのイベントを入口にしてフォーカス位置で振り分ける(`src/ui/terminals/clipboardRouting.ts`)。
//!
//! Tauri は、メニューを指定しないと macOS で既定のメニュー(`Menu::default`。アプリ・ファイル・編集・表示・
//! ウィンドウ・ヘルプ)を付ける。既定のものには ⌘W(ウィンドウを閉じる)や ⌘M などが入っていて、
//! M2 のタブ操作(FR-044 の ⌘W)とぶつかるので使わない。ここでは要る項目だけを置く。
//! 項目はすべて macOS 標準の項目(`PredefinedMenuItem`)で、動作は AppKit が行う。

use tauri::menu::{Menu, MenuBuilder, SubmenuBuilder};
use tauri::{AppHandle, Runtime};

/// 「編集」の見出し。
pub const EDIT_MENU_TITLE: &str = "編集";
/// 「編集」に並べる項目の文言(上から順)。
pub const EDIT_MENU_ITEMS: [&str; 3] = ["コピー", "ペースト", "すべてを選択"];

/// メニューバーを組み立てる。`Builder::menu` に渡す。
///
/// - アプリ名のメニュー: 隠す・ほかを隠す・すべてを表示・終了(macOS のアプリとして最小限)
/// - 編集: コピー(⌘C)・ペースト(⌘V)・すべてを選択(⌘A)
///
/// # Errors
///
/// メニューの部品を作れなかったとき(macOS ではメインスレッド以外で呼んだときなど)。
pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let name = app.package_info().name.clone();
    let app_menu = SubmenuBuilder::new(app, &name)
        .hide_with_text(format!("{name} を隠す"))
        .hide_others_with_text("ほかを隠す")
        .show_all_with_text("すべてを表示")
        .separator()
        .quit_with_text(format!("{name} を終了"))
        .build()?;
    let [copy, paste, select_all] = EDIT_MENU_ITEMS;
    let edit_menu = SubmenuBuilder::new(app, EDIT_MENU_TITLE)
        .copy_with_text(copy)
        .paste_with_text(paste)
        .separator()
        .select_all_with_text(select_all)
        .build()?;
    MenuBuilder::new(app)
        .items(&[&app_menu, &edit_menu])
        .build()
}
