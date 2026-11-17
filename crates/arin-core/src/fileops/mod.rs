//! ファイル操作の規則と手順(設計書 §5.1 の `arin-core/fileops`。M3)。
//!
//! - `types`: 画面とやり取りする型(T38a)
//! - `scope`: 範囲の検査(`PathScopeGuard`。T39a)
//! - `name`: 名前の検査(`NameValidator`。T39a)
//! - `unique`: 両方残すの名前(`UniqueNameGenerator`。T39a)
//! - `service`: 操作の手順(`FileOperationService`。T39b)
//! - `clipboard`: 切り取りの状態(`FileClipboard`。T39b)
//!
//! 後のタスクがこのファイルを触らずに足せるよう、子モジュールを公開して中の名前で使う。

pub mod clipboard;
pub mod name;
pub mod scope;
pub mod service;
pub mod types;
pub mod unique;

pub use types::{
    ConflictResolution, FileOpOutcome, InvalidNameReason, NewEntryKind, NotAllowedReason,
};
