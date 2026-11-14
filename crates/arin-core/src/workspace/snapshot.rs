//! 画面へ渡す状態の写し。
//!
//! Rust の状態が変わるたびに、この写し全体を画面へ送る(設計書 §5.2)。画面はこれで丸ごと置き換える。
//! TypeScript の型は `ts-rs` で `src/ipc/generated/` に生成する(ADR-006)。
//! 生成物の書き出しはテスト(`tests/generated_types.rs`)が行い、ここでは入出力をしない。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::terminal::TerminalSnapshot;

/// 写しの形の版。写しの項目の意味を変えたり項目を消したりしたときに 1 つ上げる(項目の追加では上げない)。
pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;

/// 画面へ渡す状態の写し。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    /// 写しの形の版(`SNAPSHOT_SCHEMA_VERSION`)。
    pub schema_version: u32,
    /// アプリの版(例: `0.1.0`)。
    pub app_version: String,
    /// ターミナルの一覧(M1 は 1 枚)。
    pub terminals: Vec<TerminalSnapshot>,
}

impl AppSnapshot {
    /// いまの形の版で写しを作る。アプリの版は組み立て側(`src-tauri`)から受け取る。
    #[must_use]
    pub fn new(app_version: impl Into<String>, terminals: Vec<TerminalSnapshot>) -> Self {
        Self {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            app_version: app_version.into(),
            terminals,
        }
    }
}
