//! デスクトップ連携(設計書 §4 の `desktop/`、§5.4)。M2 はペーストボードへの文字列の書き込み(FR-064)と、
//! フォルダ選択のダイアログ(FR-001・FR-007)。M3 で Finder で表示・既定のアプリで開く(`NSWorkspace`。FR-018)と、
//! ファイルの URL のコピー・ペースト(FR-027)を足した。
//!
//! 呼べるスレッド: フォルダ選択のダイアログはメインスレッドだけ(メインスレッドの外では開かずに失敗する)。
//! ペーストボード(文字列・ファイルの URL)と Finder・アプリへの受け渡しは、どのスレッドからでも呼べる。

mod file_pasteboard;
mod folder_panel;
mod pasteboard;
mod workspace;

pub use file_pasteboard::MacFilePasteboard;
pub use folder_panel::{NotMainThread, choose_folder};
pub use pasteboard::MacPasteboard;
pub(crate) use workspace::file_url_from_str;
pub use workspace::{MacDesktop, check_openable, check_revealable};
