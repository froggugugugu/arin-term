//! ターミナルの右クリックのメニューの「コピー」「ペースト」(ADR-020)。
//!
//! 画面からは `copy`・`paste` のイベントを起こせず、WebKit の `navigator.clipboard` は確認を出すので、
//! 一般のペーストボード(`PasteboardPort`)をここで読み書きする。プレーンテキストだけを扱う。

use std::sync::Arc;

use arin_core::ports::pasteboard::PasteboardPort;
use tauri::State;

/// ターミナルのコピー・ペーストが使うペーストボード。Tauri の管理状態に登録する。
pub struct Clipboard {
    pasteboard: Arc<dyn PasteboardPort>,
}

impl Clipboard {
    #[must_use]
    pub fn new(pasteboard: Arc<dyn PasteboardPort>) -> Self {
        Self { pasteboard }
    }
}

/// 文字列をクリップボードに書く(右クリックの「コピー」)。空の文字列は書かない(中身を消さない)。
///
/// # Errors
///
/// 書けなかったとき(短い文)。
#[tauri::command]
pub fn write_clipboard_text(clipboard: State<'_, Clipboard>, text: String) -> Result<(), String> {
    if text.is_empty() {
        return Ok(());
    }
    clipboard
        .pasteboard
        .write_text(&text)
        .map_err(|_| "クリップボードに書けませんでした".to_owned())
}

/// クリップボードのプレーンテキストを返す(右クリックの「ペースト」)。文字列が無ければ空の文字列。
///
/// # Errors
///
/// 読めなかったとき(短い文)。
#[tauri::command]
pub fn read_clipboard_text(clipboard: State<'_, Clipboard>) -> Result<String, String> {
    clipboard
        .pasteboard
        .read_text()
        .map(Option::unwrap_or_default)
        .map_err(|_| "クリップボードを読めませんでした".to_owned())
}
