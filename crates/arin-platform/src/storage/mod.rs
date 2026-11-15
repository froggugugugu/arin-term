//! 状態フォルダへの読み書き(設計書 §4 の `storage/`)。
//!
//! 起動スモークの印と、状態ファイル(`state.json`)の読み書き・退避を置く。設定ファイルの読み書きは M5 で足す。

mod ready_marker;
mod state_file;

pub use ready_marker::write_ready_marker;
pub use state_file::FsStateFile;
