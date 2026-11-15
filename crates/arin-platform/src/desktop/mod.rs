//! デスクトップ連携(設計書 §4 の `desktop/`、§5.4)。M2 はペーストボードへの文字列の書き込みだけ(FR-064)。
//! Finder で表示・開く(`NSWorkspace`)、ファイルの URL のコピー・ペースト(FR-027)は M3 以降で足す。

mod pasteboard;

pub use pasteboard::MacPasteboard;
