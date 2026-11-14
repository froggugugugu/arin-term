//! ストアをまたぐ操作の入口と、画面へ渡す状態の写しの生成(設計書 §5.1・§5.2)。
//!
//! いまは状態の写し `AppSnapshot` の型だけを置く。プロジェクトやターミナルの項目は M1・M2 で足す。

mod snapshot;

pub use snapshot::{AppSnapshot, SNAPSHOT_SCHEMA_VERSION};
