//! アプリ全体のコマンド(設計書 §5.2 の「アプリ」)。
//!
//! 画面側の呼び出しは `src/ipc/client.ts` の `CommandMap` と名前を合わせる。

use arin_core::workspace::AppSnapshot;
use tauri::State;

use crate::lifecycle::ReadyNotice;
use crate::sessions::SessionRegistry;

/// 状態の写しが変わったときに送るイベントの名前。中身は `AppSnapshot`(設計書 §5.2)。
pub const SNAPSHOT_EVENT: &str = "app-snapshot";

/// 状態の写しを返す。画面は起動時にこれを呼び、`appStore` を置き換える。
#[tauri::command]
pub fn get_app_snapshot(sessions: State<'_, SessionRegistry>) -> AppSnapshot {
    sessions.snapshot()
}

/// 画面の準備ができたことを受け取る。
///
/// いまは起動スモークの印を書く(スモークが有効なときだけ)。失敗は文字列にして画面へ返す(画面が表示する)。
#[tauri::command]
pub fn notify_app_ready(ready: State<'_, ReadyNotice>) -> Result<(), String> {
    ready.on_app_ready()
}
