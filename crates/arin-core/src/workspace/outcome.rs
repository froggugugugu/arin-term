//! `Workspace` の入力(起動時に渡すもの)と、ストアをまたぐ操作の結果。

use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::editor::EditorId;
use crate::paths::RelPath;
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

/// 見回りの対象のプロジェクト 1 件(`Workspace::project_roots`。計画書 T33d)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRoot {
    /// 識別子。
    pub id: ProjectId,
    /// いまのルートの参照(最後に分かっていたパスとブックマーク)。
    pub root: RootReference,
    /// ルートが見つからない。
    pub missing: bool,
}

/// プロジェクトを削除しようとした結果(FR-003)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoveOutcome {
    /// 動作中のターミナルか未保存のエディタがあるので確認が要る。何も変えていない(ADR-015 の 4)。
    NeedsConfirmation {
        /// そのプロジェクトの動作中のターミナル(タブの順)。
        running: Vec<RunningTerminal>,
        /// そのプロジェクトの未保存のエディタ(開いた順)。
        unsaved: Vec<UnsavedEditor>,
    },
    /// 削除した。`terminals` はそのプロジェクトにあったタブ(タブの順)。呼び出し側がセッションを終わらせる。
    /// 開いていたエディタも外した(未保存の変更は捨てた)。
    Removed {
        /// 閉じたターミナル。
        terminals: Vec<TerminalId>,
    },
}

/// 削除のコマンド(`remove_project`)の応答(FR-003)。閉じたタブの ID は画面に返さない(写しで届く)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RemoveProjectOutcome {
    /// 動作中のターミナルか未保存のエディタがあるので確認が要る。何も変えていない。画面が確認を出し、確認したら
    /// `confirmed` を真にして呼び直す。
    NeedsConfirmation {
        /// そのプロジェクトの動作中のターミナル(タブの順)。
        running: Vec<RunningTerminal>,
        /// そのプロジェクトの未保存のエディタ(開いた順。ADR-015 の 4)。
        unsaved: Vec<UnsavedEditor>,
    },
    /// 一覧から外した(フォルダには触れていない)。
    Removed,
}

impl From<&RemoveOutcome> for RemoveProjectOutcome {
    fn from(outcome: &RemoveOutcome) -> Self {
        match outcome {
            RemoveOutcome::NeedsConfirmation { running, unsaved } => Self::NeedsConfirmation {
                running: running.clone(),
                unsaved: unsaved.clone(),
            },
            RemoveOutcome::Removed { .. } => Self::Removed,
        }
    }
}

/// フォルダを登録するコマンド(`add_project_by_dialog`)の応答(FR-001・FR-010)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AddProjectOutcome {
    /// 登録して選び、タブを 1 枚開いた。
    Added {
        /// 登録したプロジェクト。
        project_id: ProjectId,
    },
    /// 同じフォルダが登録済みだったので、増やさずにそれを選んだ(失敗ではない)。
    AlreadyRegistered {
        /// 選んだ既存のプロジェクト。
        project_id: ProjectId,
    },
    /// ダイアログが取り消された。何も変えていない。
    Cancelled,
}

/// 動作中のターミナル 1 枚(削除と終了の確認に出す。FR-003・FR-063)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RunningTerminal {
    /// ターミナルの識別子。
    pub terminal_id: TerminalId,
    /// 持ち主のプロジェクト。いまは必ず値がある(M1 の 9 枚の検証画面の `null` は M4 の T49d で無くなった。
    /// 形を変えないため、型は `null` を取りうるままにしてある)。
    pub project_id: Option<ProjectId>,
    /// 持ち主のプロジェクトの表示名。いまは必ず値がある(型は `project_id` と同じ理由で `null` を取りうるまま)。
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
    /// 拡張子が `.md`・`.markdown` でないので、エディタで開かない(ADR-015 の 5)。
    #[error("Markdown のファイルではないので、エディタで開けない: {0}")]
    NotMarkdown(RelPath),
}

/// 未保存のエディタ 1 つ(終了・プロジェクトの削除・閉じる確認に出す。ADR-015 の 4)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UnsavedEditor {
    /// エディタの識別子。
    pub editor_id: EditorId,
    /// 持ち主のプロジェクトの表示名。
    pub project_name: String,
    /// ルート基準の相対パス。
    pub path: RelPath,
}

/// エディタを閉じようとした結果(`close_editor` の応答。ADR-015 の 4)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum CloseEditorOutcome {
    /// 閉じた。
    Closed,
    /// 未保存の変更があり `discard` が偽なので閉じていない(何も変えていない)。画面が確認を出す。
    NeedsConfirmation,
}

/// 接続のコマンド(`connect_terminal`)の応答。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ConnectOutcome {
    /// 接続で渡した寸法を PTY に反映したか。起動したときと、再接続で寸法を変えられたときは `true`。
    /// 再接続で PTY が書き込みで塞がっていて変えられなかったときは `false`(画面が寸法を 1 回送り直す)。
    pub size_applied: bool,
}
