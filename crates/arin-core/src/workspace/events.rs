//! Rust から画面へ送るイベントの中身のうち、写し(`AppSnapshot`)以外のもの(計画書 T33e)。
//!
//! イベントの名前と送るところは `src-tauri/src/events.rs`。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::RunningTerminal;

/// メニューバーの項目から画面へ送る操作(`menu-command` イベントの中身。設計書 §9.1)。
///
/// 画面がフォーカス位置で振り分ける(`src/logic/keys/menuCommand.ts` の `MenuCommand` と同じ文字列)。
/// タブの操作(新規・閉じる・前後)は画面を経由せず Rust が行うので、ここには無い。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum MenuCommand {
    /// 「すべてを選択」(⌘A)。ターミナルにフォーカスがあれば、そのターミナルを全選択する。
    SelectAll,
}

/// 終了の確認を求める(`quit-requested` イベントの中身。FR-063)。
///
/// 終了の要求(⌘Q、ウィンドウの閉じるボタン)のときに動作中のターミナルがあれば、終了を止めてこれを送る。
/// 画面が確認を出し、確認したら `confirm_quit` を呼ぶ。取り消したら何も呼ばない(何も変わっていない)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct QuitRequest {
    /// 動作中のターミナル(プロジェクトの並び順・タブの順)。空にはならない。
    pub running: Vec<RunningTerminal>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::TerminalId;

    #[test]
    fn メニューの操作は画面と同じ文字列になる() {
        assert_eq!(
            serde_json::to_value(MenuCommand::SelectAll).unwrap(),
            serde_json::json!("select-all")
        );
    }

    #[test]
    fn 終了の確認の中身は動作中の一覧を持つ() {
        let request = QuitRequest {
            running: vec![RunningTerminal {
                terminal_id: TerminalId::new("terminal-3"),
                project_id: None,
                project_name: Some("app".to_owned()),
                label: "ターミナル 1".to_owned(),
            }],
        };
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            serde_json::json!({
                "running": [{
                    "terminalId": "terminal-3",
                    "projectId": null,
                    "projectName": "app",
                    "label": "ターミナル 1"
                }]
            })
        );
    }
}
