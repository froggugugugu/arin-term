//! ターミナルの ID・状態・失敗と、寸法の検証(設計書 §5.1、§6.1 の `TerminalStore`)。
//!
//! タブの並びと表示中のタブは `store.rs`(`TerminalStore`)。アテンションなどは M6 で足す。

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use super::ShellLaunchError;
use crate::ports::PtySize;

/// ターミナルの識別子。画面とのやり取りでは文字列。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
pub struct TerminalId(String);

impl TerminalId {
    /// 文字列から作る。中身の検査はしない(存在しない ID は操作のときに `NotFound` になる)。
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// 文字列としての中身。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TerminalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// ターミナルの状態。
///
/// `NotStarted` → (最初の接続で起動)→ `Running` → (シェルが終わる)→ `Exited`。
/// 起動に失敗したら `Failed` になり、次の接続でもう一度起動を試す。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TerminalStatus {
    /// まだ起動していない(画面が接続すると起動する)。
    NotStarted,
    /// シェルが動いている。
    Running,
    /// シェルが終わった。
    Exited {
        /// 終了コード。分からないときは `null`。
        code: Option<u32>,
        /// シグナルで終わったときのシグナルの名前。
        signal: Option<String>,
    },
    /// 起動に失敗した。
    Failed {
        /// 理由(画面に出す)。
        message: String,
    },
}

/// 状態の写しに載せるターミナル 1 枚。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSnapshot {
    /// 識別子。接続・入力などのコマンドで指す。
    pub id: TerminalId,
    /// 状態。
    pub status: TerminalStatus,
}

/// ターミナルの操作を受け付けられない理由。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TerminalError {
    /// その ID のターミナルが無い。
    #[error("ターミナル {0} が見つからない")]
    NotFound(TerminalId),
    /// 寸法が 0 を含む、または上限(`MAX_ROWS`・`MAX_COLS`)を超える。
    #[error("ターミナルの寸法 {rows} 行 × {cols} 桁は使えない(行は 1〜1000、桁は 1〜2000 にする)")]
    InvalidSize {
        /// 受け取った行数。
        rows: u16,
        /// 受け取った桁数。
        cols: u16,
    },
    /// まだシェルが起動していない(接続の前、または起動に失敗した)。
    #[error("ターミナル {0} はまだ起動していない")]
    NotStarted(TerminalId),
    /// シェルが終わっている。
    #[error("ターミナル {0} のシェルは終了している")]
    Exited(TerminalId),
    /// 起動の仕様を組み立てられなかった。
    #[error(transparent)]
    Launch(#[from] ShellLaunchError),
}

/// 行数の上限(計画書 M2 の【仮定】11)。
pub const MAX_ROWS: u16 = 1_000;
/// 桁数の上限(計画書 M2 の【仮定】11)。
pub const MAX_COLS: u16 = 2_000;

/// 画面から受け取った寸法を検証し、PTY の寸法にする。行は 1〜`MAX_ROWS`、桁は 1〜`MAX_COLS`。
///
/// # Errors
///
/// 行か桁が 0 のとき、上限を超えるとき(`TerminalError::InvalidSize`)。
pub fn validate_size(rows: u16, cols: u16) -> Result<PtySize, TerminalError> {
    if !(1..=MAX_ROWS).contains(&rows) || !(1..=MAX_COLS).contains(&cols) {
        return Err(TerminalError::InvalidSize { rows, cols });
    }
    Ok(PtySize { rows, cols })
}
