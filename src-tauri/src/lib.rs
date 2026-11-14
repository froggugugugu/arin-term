//! Arin の組み立て(設計書 §11)。
//!
//! ここでは部品を生成して渡すことだけを行い、規則を書かない。
//! いまは `Workspace` が無いので、置き場所と起動の通知先を管理状態に登録し、コマンドを登録して
//! tauri.conf.json のウィンドウを 1 枚出すだけの構成にする。

mod commands;
pub mod launch;
pub mod lifecycle;

use tauri::{Builder, Runtime};

use crate::launch::LaunchConfig;
use crate::lifecycle::ReadyNotice;

/// アプリの組み立て。ランタイムを型引数で受け取り、テストでは模擬ランタイムに差し替える。
///
/// 起動時の設定(`LaunchConfig`)は引数で受け取る。環境変数を読むのは `run` の側だけ。
pub fn builder<R: Runtime>(launch: LaunchConfig) -> Builder<R> {
    let ready = ReadyNotice::new(launch.ready_marker().map(ToOwned::to_owned));
    Builder::<R>::new()
        .manage(launch.paths().clone())
        .manage(ready)
        .invoke_handler(tauri::generate_handler![
            commands::app::get_app_snapshot,
            commands::app::notify_app_ready,
        ])
}

/// アプリを起動する。
pub fn run() {
    let launch = match LaunchConfig::from_env() {
        Ok(launch) => launch,
        Err(e) => {
            // 置き場所を決められないまま起動すると、意図しない場所を読み書きしかねないので起動しない。
            eprintln!("Arin を起動できない: {e}");
            std::process::exit(2);
        }
    };
    builder::<tauri::Wry>(launch)
        .run(tauri::generate_context!())
        .expect("Tauri のアプリを起動できなかった");
}
