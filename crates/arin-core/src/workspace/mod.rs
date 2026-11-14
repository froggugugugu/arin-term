//! ストアをまたぐ操作の入口と、画面へ渡す状態の写しの生成(設計書 §5.1・§5.2)。
//!
//! M1 はターミナル 1 枚だけを持つ。プロジェクトの項目は M2 で足す。

mod snapshot;
mod state;

pub use snapshot::{AppSnapshot, SNAPSHOT_SCHEMA_VERSION};
pub use state::{ConnectPlan, Workspace};
