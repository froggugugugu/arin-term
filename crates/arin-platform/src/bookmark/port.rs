//! [`BookmarkPort`] の実装。自由関数([`create_bookmark`]・[`resolve_bookmark`]・[`encode`]・[`decode`])を包む。

use std::path::Path;

use arin_core::ports::{BookmarkPort, ResolvedBookmark, RootError};

use super::{BookmarkError, create_bookmark, decode, encode, resolve_bookmark};

/// macOS のブックマーク(セキュリティスコープなし)。文字列は base64 の標準の符号化。状態を持たない。
#[derive(Debug, Clone, Copy, Default)]
pub struct MacBookmarks;

impl MacBookmarks {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl BookmarkPort for MacBookmarks {
    fn create(&self, path: &Path) -> Result<String, RootError> {
        create_bookmark(path)
            .map(|data| encode(&data))
            .map_err(|e| to_root_error(Some(path), e))
    }

    fn resolve(&self, bookmark: &str) -> Result<ResolvedBookmark, RootError> {
        let data = decode(bookmark).map_err(|e| to_root_error(None, e))?;
        let resolved = resolve_bookmark(&data).map_err(|e| to_root_error(None, e))?;
        Ok(ResolvedBookmark {
            path: resolved.path,
            stale: resolved.stale,
        })
    }
}

/// [`BookmarkError`] を [`RootError`] に振り分ける。見つからないものだけ `NotFound`、残りは `Bookmark`。
fn to_root_error(path: Option<&Path>, e: BookmarkError) -> RootError {
    let message = match path {
        Some(path) => format!("{}: {e}", path.display()),
        None => e.to_string(),
    };
    match e {
        BookmarkError::NotFound => RootError::NotFound(message),
        BookmarkError::Corrupt | BookmarkError::NonUtf8Path | BookmarkError::Os { .. } => {
            RootError::Bookmark(message)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 見つからないものだけ_not_found_になる() {
        let p = Some(Path::new("/x"));
        assert!(matches!(
            to_root_error(p, BookmarkError::NotFound),
            RootError::NotFound(m) if m.starts_with("/x: ")
        ));
        assert!(matches!(
            to_root_error(None, BookmarkError::Corrupt),
            RootError::Bookmark(m) if m == BookmarkError::Corrupt.to_string()
        ));
        assert!(matches!(
            to_root_error(p, BookmarkError::NonUtf8Path),
            RootError::Bookmark(_)
        ));
        let os = BookmarkError::Os {
            domain: "D".into(),
            code: 1,
            message: "m".into(),
        };
        assert!(matches!(to_root_error(p, os), RootError::Bookmark(_)));
    }
}
