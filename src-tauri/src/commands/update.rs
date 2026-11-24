//! 新しい版の確認のコマンド(ADR-018)。確かめる処理と知らせは `crate::update`。

use arin_core::update::UpdateNotice;
use tauri::{AppHandle, Runtime, State};

use crate::sessions::SessionRegistry;
use crate::update::{self, UpdateChecker};

/// いまの知らせ(起動時に画面が 1 回呼ぶ。そのあとは `update-changed` で届く)。無ければ `None`。
#[tauri::command]
pub fn get_update_notice(checker: State<'_, UpdateChecker>) -> Option<UpdateNotice> {
    checker.notice()
}

/// 知らせを閉じる。
#[tauri::command]
pub fn dismiss_update_notice(checker: State<'_, UpdateChecker>) {
    checker.dismiss();
}

/// Releases のページを既定のブラウザで開く(URL は Rust が持つ版から作る。画面から URL を受け取らない)。
///
/// # Errors
///
/// ブラウザに渡せなかったとき(短い文)。
#[tauri::command]
pub fn open_release_page(checker: State<'_, UpdateChecker>) -> Result<(), String> {
    checker
        .open_release_page()
        .map_err(|_| "Releases のページを開けませんでした".to_owned())
}

/// 起動時に新しい版を確かめるかを切り替える(設定画面)。オンにしたら、その場で 1 回確かめる(新しい版があるときだけ知らせる)。
/// オフにしたら、出している知らせを消す。
#[tauri::command]
pub fn set_check_updates<R: Runtime>(
    app: AppHandle<R>,
    sessions: State<'_, SessionRegistry>,
    checker: State<'_, UpdateChecker>,
    enabled: bool,
) {
    if !sessions.set_check_updates(enabled) {
        return;
    }
    if enabled {
        update::check(&app, false);
    } else {
        checker.clear(&app);
    }
}
