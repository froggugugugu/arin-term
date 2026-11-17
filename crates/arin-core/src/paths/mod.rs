//! パスに関わる規則(設計書 §4・§11)。
//!
//! 状態の置き場所の決定(`AppPaths`)と、ルート基準の相対パス(`RelPath`。M3)を置く。
//! 相対・絶対パスの整形(`format`)とシェルエスケープ(`shell`)は M3 の T38b が子モジュールに置く
//! (このファイルを触らずに足せるよう、モジュールを公開して中の名前で使う)。

mod app_paths;
pub mod format;
mod relative;
pub mod shell;

pub use app_paths::{
    AppPaths, AppPathsError, SMOKE_READY_MARKER_FILE_NAME, SMOKE_TERMINAL_MARKER_FILE_NAME,
};
pub use relative::{PathKind, RelPath, RelPathError};
