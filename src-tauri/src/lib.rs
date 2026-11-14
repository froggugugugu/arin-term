//! Arin の組み立て(設計書 §11)。
//!
//! ここでは部品を生成して渡すことだけを行い、規則を書かない。
//! いまは `Workspace` が無いので、コマンドを登録して tauri.conf.json のウィンドウを 1 枚出すだけの構成にする。

mod commands;

use tauri::{Builder, Runtime};

/// アプリの組み立て。ランタイムを型引数で受け取り、テストでは模擬ランタイムに差し替える。
pub fn builder<R: Runtime>() -> Builder<R> {
    Builder::<R>::new().invoke_handler(tauri::generate_handler![
        commands::app::get_app_snapshot,
        commands::app::notify_app_ready,
    ])
}

/// アプリを起動する。
pub fn run() {
    builder::<tauri::Wry>()
        .run(tauri::generate_context!())
        .expect("Tauri のアプリを起動できなかった");
}
