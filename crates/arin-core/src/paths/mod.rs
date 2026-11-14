//! パスに関わる規則(設計書 §4・§11)。
//!
//! いまは状態の置き場所の決定(`AppPaths`)だけを置く。相対・絶対パスの整形とシェルエスケープは M3 で足す。

mod app_paths;

pub use app_paths::{AppPaths, AppPathsError, SMOKE_READY_MARKER_FILE_NAME};
