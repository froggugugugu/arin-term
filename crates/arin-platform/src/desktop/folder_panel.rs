//! フォルダ選択のダイアログ(`NSOpenPanel`。objc2-app-kit)。FR-001(登録)と FR-007(再リンク)。
//!
//! `tauri-plugin-dialog` は `tauri-plugin-fs` を外せない依存に持つので使わない(ADR-009、設計書 §12)。
//! 選んだパスは画面を経由させず、呼び出し側(`src-tauri`)がそのまま登録に使う。
//! AppKit の決まりでメインスレッドでしか開けない。メインスレッドでなければ開かずに失敗を返す。
//! 自動テストでは開かない(GUI を出すため)。

use std::path::PathBuf;

use objc2::MainThreadMarker;
use objc2::rc::autoreleasepool;
use objc2_app_kit::{NSModalResponseOK, NSOpenPanel};
use objc2_foundation::NSString;

/// メインスレッドの外から開こうとした。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("フォルダ選択のダイアログはメインスレッドでしか開けない")]
pub struct NotMainThread;

/// フォルダを 1 つだけ選ばせるダイアログを、閉じられるまで出す(アプリ全体のモーダル)。
/// `prompt` は決定のボタンの文言。選ばれたフォルダのパス(正規化はしない)を返し、取り消されたら `None`。
///
/// # Errors
///
/// メインスレッドから呼ばなかったとき(開かない)。
pub fn choose_folder(prompt: &str) -> Result<Option<PathBuf>, NotMainThread> {
    let mtm = MainThreadMarker::new().ok_or(NotMainThread)?;
    Ok(run_panel(mtm, prompt))
}

fn run_panel(mtm: MainThreadMarker, prompt: &str) -> Option<PathBuf> {
    autoreleasepool(|_| {
        let panel = NSOpenPanel::openPanel(mtm);
        panel.setCanChooseDirectories(true);
        panel.setCanChooseFiles(false);
        panel.setAllowsMultipleSelection(false);
        panel.setResolvesAliases(true);
        panel.setCanCreateDirectories(true);
        panel.setPrompt(Some(&NSString::from_str(prompt)));
        if panel.runModal() != NSModalResponseOK {
            return None;
        }
        let url = panel.URL()?;
        let path = url.path()?;
        Some(PathBuf::from(path.to_string()))
    })
}
