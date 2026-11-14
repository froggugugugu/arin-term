//! 状態フォルダへの読み書き(設計書 §4 の `storage/`)。
//!
//! いまは起動スモークの印だけを置く。`state.json` と設定ファイルの読み書きは M1 以降で足す。

mod ready_marker;

pub use ready_marker::write_ready_marker;
