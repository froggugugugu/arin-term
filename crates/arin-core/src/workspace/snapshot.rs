//! 画面へ渡す状態の写し。
//!
//! Rust の状態が変わるたびに、この写し全体を画面へ送る(設計書 §5.2)。画面はこれで丸ごと置き換える。
//! TypeScript の型は `ts-rs` で `src/ipc/generated/` に生成する(ADR-006)。
//! 生成物の書き出しはテスト(`tests/generated_types.rs`)が行い、ここでは入出力をしない。
//!
//! 絶対パスとブックマークは載せない(設計書 §12。パスのコピーは Rust が行う)。載せるのは表示用のパス(`~` 省略)だけ。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::layout::panes::PaneSizes;
use crate::project::{ProjectColor, ProjectId};
use crate::terminal::{TerminalId, TerminalStatus};

/// 写しの形の版。写しの項目の意味を変えたり項目を消したりしたときに 1 つ上げる(項目の追加では上げない)。
pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;

/// 画面へ渡す状態の写し。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    /// 写しの形の版(`SNAPSHOT_SCHEMA_VERSION`)。
    pub schema_version: u32,
    /// アプリの版(例: `0.1.0`)。
    pub app_version: String,
    /// プロジェクトの一覧(並び順のとおり)。9 枚の検証画面では空。
    pub projects: Vec<ProjectCardSnapshot>,
    /// 選択中のプロジェクト。
    pub selected_project_id: Option<ProjectId>,
    /// ターミナルの一覧(プロジェクトの並び順、その中はタブの順。持ち主の無いもの=検証画面は最後)。
    pub terminals: Vec<TerminalTabSnapshot>,
    /// ペインの寸法(保存値。表示値への調整は画面が行う)。
    pub layout: PaneSizes,
    /// 状態ファイルを退避したときの 1 行の知らせ。無ければ `null`。
    pub notice: Option<String>,
}

/// 写しに載せるプロジェクト 1 件(カード 1 枚)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCardSnapshot {
    /// 識別子。
    pub id: ProjectId,
    /// 表示名。
    pub name: String,
    /// 色の識別子。
    pub color: ProjectColor,
    /// 表示用のパス(ホームの中は `~` で始める。ツールチップ用。FR-007)。
    pub display_path: String,
    /// ルートが見つからない。
    pub missing: bool,
    /// このプロジェクトのタブ(タブの順)。
    pub terminal_ids: Vec<TerminalId>,
    /// 表示中のタブ。0 枚なら `null`。
    pub active_terminal_id: Option<TerminalId>,
}

/// 写しに載せるターミナル(タブ)1 枚。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TerminalTabSnapshot {
    /// 識別子。接続・入力などのコマンドで指す。
    pub id: TerminalId,
    /// 持ち主のプロジェクト。9 枚の検証画面では `null`。
    pub project_id: Option<ProjectId>,
    /// タブの表示名(「ターミナル N」)。
    pub label: String,
    /// 状態。
    pub status: TerminalStatus,
}
