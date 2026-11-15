//! デスクトップ連携(設計書 §4 の `desktop/`、§5.4)。M2 はペーストボードへの文字列の書き込み(FR-064)と、
//! フォルダ選択のダイアログ(FR-001・FR-007)。
//! Finder で表示・開く(`NSWorkspace`)、ファイルの URL のコピー・ペースト(FR-027)は M3 以降で足す。

mod folder_panel;
mod pasteboard;

pub use folder_panel::{NotMainThread, choose_folder};
pub use pasteboard::MacPasteboard;
