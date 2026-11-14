//! アプリ全体のコマンド(設計書 §5.2 の「アプリ」)。
//!
//! 画面側の呼び出しは `src/ipc/client.ts` の `CommandMap` と名前を合わせる。

use arin_core::workspace::AppSnapshot;
use tauri::{AppHandle, Runtime};

/// 状態の写しを返す。画面は起動時にこれを呼び、`appStore` を置き換える。
#[tauri::command]
pub fn get_app_snapshot<R: Runtime>(app: AppHandle<R>) -> AppSnapshot {
    AppSnapshot::new(app.package_info().version.to_string())
}

/// 画面の準備ができたことを受け取る。
///
/// いまは受け取るだけで何もしない。写しにあるターミナルの接続や、起動スモークでの確認は
/// T10・T11 以降で `Workspace` を通してここにつなぐ。
#[tauri::command]
pub fn notify_app_ready() {}
