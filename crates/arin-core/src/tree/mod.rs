//! ツリーの規則(設計書 §5.1 の `arin-core/tree`。M3)。
//!
//! - `types`: 画面とやり取りする型(T38a)
//! - `exclusion`: 除外の判定(T38c)
//! - `order`: 並び順(T38c)
//! - `changes`: 監視で届いた変更のまとめ(T38c)
//!
//! 後のタスクがこのファイルを触らずに足せるよう、子モジュールを公開して中の名前で使う。

pub mod changes;
pub mod exclusion;
pub mod order;
pub mod types;

pub use types::{DirEntry, DirListing, EntryKind, TreeChange};
