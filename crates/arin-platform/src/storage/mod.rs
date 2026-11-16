//! 状態フォルダへの読み書き(設計書 §4 の `storage/`)。
//!
//! 起動スモークの印、状態ファイル(`state.json`)の読み書き・退避、設定ファイル(`settings.toml`)の
//! 読み取り・見本の作成・TOML の解析を置く。

mod ready_marker;
mod settings_file;
mod state_file;

pub use ready_marker::write_ready_marker;
pub use settings_file::{FsSettingsFile, load_settings_file, parse_settings_toml};
pub use state_file::FsStateFile;
