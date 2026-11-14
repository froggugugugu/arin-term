//! メニューバーの組み立て(`menu.rs`、FR-043、設計書 §9.1)を、Tauri の模擬ランタイムで確かめる。
//!
//! アプリを組み立てたときに、Tauri の既定のメニューではなく本アプリのメニューが付き、
//! 「編集」にコピーとペースト(macOS 標準の項目)があることを見る。
//! 標準の項目がキーの組み合わせ(⌘C・⌘V)を持つことと、それが画面のイベントになることは AppKit の仕事なので、
//! 実機で確かめる(T25 の手順書)。
//!
//! macOS ではメニューの部品をメインスレッドでしか作れないため、`harness = false` で走らせる(`app_builder` と同じ)。

mod support;

use arin_lib::menu::{EDIT_MENU_ITEMS, EDIT_MENU_TITLE};
use tauri::App;
use tauri::menu::{Menu, MenuItemKind};
use tauri::test::MockRuntime;

fn build_app(state_dir: &support::TempDir) -> App<MockRuntime> {
    arin_lib::builder::<MockRuntime>(support::launch_in(state_dir, false))
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

/// 見出しの下の項目の文言(区切り線は `---`)。標準の項目でないものがあれば失敗させる。
fn predefined_texts(menu: &Menu<MockRuntime>, title: &str) -> Vec<String> {
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
                if text.is_empty() {
                    "---".to_owned()
                } else {
                    text
                }
            }
            other => panic!("{title} に標準の項目でないものがある: {:?}", other.id()),
        })
        .collect()
}

fn アプリのメニューは本アプリの見出しだけで既定のメニューではない() {
    let state_dir = support::TempDir::new("menu-titles");
    let app = build_app(&state_dir);
    let menu = app.menu().expect("アプリにメニューが付いていない");

    let name = app.package_info().name.clone();
    // 既定のメニューにある「File」「View」「Window」「Help」は置かない(⌘W などを M2 で使うため)。
    assert_eq!(titles(&menu), vec![name, EDIT_MENU_TITLE.to_owned()]);
}

fn 編集にはコピーとペーストとすべてを選択がある() {
    let state_dir = support::TempDir::new("menu-edit");
    let app = build_app(&state_dir);
    let menu = app.menu().expect("アプリにメニューが付いていない");

    assert_eq!(EDIT_MENU_ITEMS, ["コピー", "ペースト", "すべてを選択"]);
    assert_eq!(
        predefined_texts(&menu, EDIT_MENU_TITLE),
        vec!["コピー", "ペースト", "---", "すべてを選択"]
    );
}

fn アプリ名のメニューには終了がある() {
    let state_dir = support::TempDir::new("menu-app");
    let app = build_app(&state_dir);
    let menu = app.menu().expect("アプリにメニューが付いていない");
    let name = app.package_info().name.clone();

    let texts = predefined_texts(&menu, &name);
    assert_eq!(texts.last(), Some(&format!("{name} を終了")));
}

fn main() {
    let tests: [(&str, fn()); 3] = [
        (
            "アプリのメニューは本アプリの見出しだけで既定のメニューではない",
            アプリのメニューは本アプリの見出しだけで既定のメニューではない,
        ),
        ("編集にはコピーとペーストとすべてを選択がある", 編集にはコピーとペーストとすべてを選択がある),
        ("アプリ名のメニューには終了がある", アプリ名のメニューには終了がある),
    ];
    for (name, test) in tests {
        test();
        println!("test {name} ... ok");
    }
}
