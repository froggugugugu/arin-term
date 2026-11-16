//! アプリ全体のコマンド(設計書 §5.2 の「アプリ」)。
//!
//! 画面側の呼び出しは `src/ipc/client.ts` の `CommandMap` と名前を合わせる。

use arin_core::shortcuts::table::ShortcutTable;
use arin_core::workspace::AppSnapshot;
use tauri::{AppHandle, Runtime, State};

use crate::diagnostics::frontend_error_line;
use crate::lifecycle::{self, ReadyNotice};
use crate::menu::Shortcuts;
use crate::sessions::SessionRegistry;
use crate::settings::{self, AppSettings};

/// 状態の写しが変わったときに送るイベントの名前。中身は `AppSnapshot`(設計書 §5.2)。
pub const SNAPSHOT_EVENT: &str = "app-snapshot";

/// 状態の写しを返す。画面は起動時にこれを呼び、`appStore` を置き換える。
#[tauri::command]
pub fn get_app_snapshot(sessions: State<'_, SessionRegistry>) -> AppSnapshot {
    sessions.snapshot()
}

/// ショートカットの表を返す(T42e)。画面は起動時に 1 回だけ呼び、キーの振り分けとメニューのキーの表示に使う(T43a)。
///
/// メニューバー(`menu.rs`)と同じ管理状態の表(`Shortcuts`)を返す。設定ファイルの値を当てた表(T65b)。
/// 設定が変わったら `settings-changed` が届くので、画面は呼び直す(T65c・T65e)。
#[tauri::command]
pub fn get_shortcuts(shortcuts: State<'_, Shortcuts>) -> ShortcutTable {
    shortcuts.table()
}

/// 設定ファイルの知らせの 1 行を返す(問題が無ければ null。T65b、前提 (g))。
///
/// ファイル全体の問題(構文エラーの行と列・読めない理由)と、ショートカットの行の問題(コマンドの ID・キー・理由)を
/// まとめた 1 行。状態ファイルの知らせ(写しの `notice`)とは別。設定の行の全文と絶対パスは含まない(AC-K09)。
#[tauri::command]
pub fn get_settings_notice(settings: State<'_, AppSettings>) -> Option<String> {
    settings.notice()
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

/// 設定画面の「設定ファイルを開く…」(ADR-013 の 6、AC-F10)。メニューの「設定ファイルを開く…」と同じ処理で、
/// 無いときだけ見本を作り、既定のアプリで開く(開けなければ Finder で表示して知らせる)。引数なし(パスは画面を通らない)。
/// 待ってよいスレッドに任せて待たずに戻る。結果の知らせは `settings-changed` と `get_settings_notice` で届く。
#[tauri::command]
pub fn open_settings_file<R: Runtime>(app: AppHandle<R>) {
    settings::open_settings_file(&app);
}

/// 終了の確認(`quit-requested`)で本人が「終了」を選んだ(FR-063)。確認なしで終わる:
/// ウィンドウを閉じ、状態を書いてから全セッションを終わらせる(`lifecycle::quit_now`)。取り消したときは何も呼ばない。
#[tauri::command]
pub fn confirm_quit<R: Runtime>(app: AppHandle<R>) {
    lifecycle::quit_now(&app);
}
