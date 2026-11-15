//! アプリ全体のコマンド(設計書 §5.2 の「アプリ」)。
//!
//! 画面側の呼び出しは `src/ipc/client.ts` の `CommandMap` と名前を合わせる。

use arin_core::launch_options::LaunchOptions;
use arin_core::workspace::AppSnapshot;
use tauri::{AppHandle, Runtime, State};

use crate::diagnostics::frontend_error_line;
use crate::lifecycle::{self, ReadyNotice};
use crate::sessions::SessionRegistry;

/// 状態の写しが変わったときに送るイベントの名前。中身は `AppSnapshot`(設計書 §5.2)。
pub const SNAPSHOT_EVENT: &str = "app-snapshot";

/// 状態の写しを返す。画面は起動時にこれを呼び、`appStore` を置き換える。
#[tauri::command]
pub fn get_app_snapshot(sessions: State<'_, SessionRegistry>) -> AppSnapshot {
    sessions.snapshot()
}

/// 起動時の指定(描画方式と全角幅の方式)を返す。画面は起動時に 1 回だけ呼び、ターミナルのエンジンを作る。
///
/// M1 の検証用(T21)。状態の写しとは分けて渡す(写しは保存する状態の正本で、開発用の指定を混ぜない)。T27 で外す。
#[tauri::command]
pub fn get_launch_options(options: State<'_, LaunchOptions>) -> LaunchOptions {
    *options
}

/// 画面の準備ができたことを受け取る。
///
/// いまは起動スモークの印を書く(スモークが有効なときだけ)。失敗は文字列にして画面へ返す(画面が表示する)。
#[tauri::command]
pub fn notify_app_ready(ready: State<'_, ReadyNotice>) -> Result<(), String> {
    ready.on_app_ready()
}

/// 画面で起きた失敗(CSP 違反、未処理の例外など)を受け取り、決まった頭の文字列を付けた 1 行で標準エラーに書く。
///
/// 診断の機能として常に有効(起動スモークはこの行で画面の失敗を検出する)。
#[tauri::command]
pub fn report_frontend_error(message: String) {
    eprintln!("{}", frontend_error_line(&message));
}

/// 終了の確認(`quit-requested`)で本人が「終了」を選んだ(FR-063)。確認なしで終わる:
/// ウィンドウを閉じ、状態を書いてから全セッションを終わらせる(`lifecycle::quit_now`)。取り消したときは何も呼ばない。
#[tauri::command]
pub fn confirm_quit<R: Runtime>(app: AppHandle<R>) {
    lifecycle::quit_now(&app);
}
