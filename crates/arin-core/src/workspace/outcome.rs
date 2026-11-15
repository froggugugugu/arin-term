//! `Workspace` の入力(起動時に渡すもの)と、ストアをまたぐ操作の結果。

use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::persistence::codec::PersistedState;
use crate::project::{ProjectError, ProjectId, RootReference};
use crate::terminal::TerminalId;

/// 起動時に渡すもの。`src-tauri` が状態ファイルを読み(T33a)、各プロジェクトのブックマークを解決して作る。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Startup {
    /// 状態ファイルから読んだ状態(無い・退避したときは既定値)。
    pub state: PersistedState,
    /// 各プロジェクトのルートを解決した結果。ここに無いプロジェクトは見つからない扱いにする。
    pub roots: Vec<(ProjectId, RootResolution)>,
    /// 状態ファイルを退避したときの 1 行の知らせ(写しに載せる。保存しない)。
    pub notice: Option<String>,
}

/// 1 つのプロジェクトのルートを起動時に解決した結果(設計書 §7.1「起動」4)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RootResolution {
    /// 解決できた。いまのパスと、使うブックマーク(古くなっていたら作り直したもの)。
    Found(RootReference),
    /// 解決できなかった(削除された、ブックマークが壊れていて作り直せなかった)。
    Missing,
}

/// プロジェクトを削除しようとした結果(FR-003)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoveOutcome {
    /// 動作中のターミナルがあるので確認が要る。何も変えていない。
    NeedsConfirmation(Vec<RunningTerminal>),
    /// 削除した。`terminals` はそのプロジェクトにあったタブ(タブの順)。呼び出し側がセッションを終わらせる。
    Removed {
        /// 閉じたターミナル。
        terminals: Vec<TerminalId>,
    },
}

/// 動作中のターミナル 1 枚(削除と終了の確認に出す。FR-003・FR-063)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RunningTerminal {
    /// ターミナルの識別子。
    pub terminal_id: TerminalId,
    /// 持ち主のプロジェクト。検証画面では `null`。
    pub project_id: Option<ProjectId>,
    /// 持ち主のプロジェクトの表示名。検証画面では `null`。
    pub project_name: Option<String>,
    /// タブの表示名。
    pub label: String,
}

/// `Workspace` の操作を受け付けられない理由(ターミナルだけの操作は `TerminalError`)。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WorkspaceError {
    /// プロジェクトの規則で受け付けられない(ID が無いなど)。
    #[error(transparent)]
    Project(#[from] ProjectError),
    /// ルートが見つからないプロジェクトにはタブを開けない。
    #[error("プロジェクト {0} のフォルダが見つからないので、ターミナルを開けない")]
    RootMissing(ProjectId),
    /// プロジェクトを選んでいないので、どこにタブを開くか決められない。
    #[error("プロジェクトが選ばれていないので、ターミナルを開けない")]
    NoProjectSelected,
}

/// 接続のコマンド(`connect_terminal`)の応答。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ConnectOutcome {
    /// 接続で渡した寸法を PTY に反映したか。起動したときと、再接続で寸法を変えられたときは `true`。
    /// 再接続で PTY が書き込みで塞がっていて変えられなかったときは `false`(画面が寸法を 1 回送り直す)。
    pub size_applied: bool,
}
