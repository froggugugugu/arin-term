//! デスクトップのポート(FR-018。設計書 §5.4「Finder で表示、開く」、計画書 T38a)。
//!
//! 実装は `arin-platform/src/desktop/`(T40d。`NSWorkspace`)。パスは絶対パス。

use std::path::Path;

use thiserror::Error;

/// Finder で表示・開くの失敗。原因は実装側の文言をそのまま持つ。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DesktopError {
    /// 対象が無い。
    #[error("見つからない: {0}")]
    NotFound(String),
    /// OS が受け付けなかった。
    #[error("Finder やアプリに渡せなかった: {0}")]
    Failed(String),
}

/// Finder とアプリに項目を渡す口。
pub trait DesktopPort: Send + Sync {
    /// Finder で項目を選んだ状態で表示する。
    ///
    /// # Errors
    ///
    /// 無いとき(`DesktopError::NotFound`)、OS が受け付けなかったとき(`DesktopError::Failed`)。
    fn reveal_in_finder(&self, path: &Path) -> Result<(), DesktopError>;

    /// 既定のアプリで開く。
    ///
    /// # Errors
    ///
    /// 無いとき(`DesktopError::NotFound`)、OS が受け付けなかったとき(`DesktopError::Failed`)。
    fn open_with_default_app(&self, path: &Path) -> Result<(), DesktopError>;

    /// 既定のブラウザで Releases のページを開く(ADR-018)。開いてよいのは
    /// `crate::update::is_release_page` が通す URL だけ(実装が確かめる)。
    ///
    /// # Errors
    ///
    /// 許していない URL のとき・OS が受け付けなかったとき(`DesktopError::Failed`)。
    fn open_release_page(&self, url: &str) -> Result<(), DesktopError>;
}
