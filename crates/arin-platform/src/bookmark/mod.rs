//! macOS のブックマーク(セキュリティスコープなし)の作成と解決(設計書 §4 の `bookmark/`、FR-007、§1.4 #12)。
//!
//! 登録したプロジェクトのフォルダを、移動・改名されても追えるように覚える。
//! - 改名・移動のあとに解決すると新しいパスと `stale = true` が返る。返ったパスから作り直すと古くなくなる
//! - フォルダが消えていると [`BookmarkError::NotFound`]
//! - 既知の制約: 消したあと同じパスに別のフォルダがあると、そこに解決される。
//!   改名後に元の名前で新しいフォルダを作ると、新しい方に解決される
//!
//! `state.json` には [`encode`] で文字列にして入れる。`arin-core` はこの文字列を中身の分からないものとして扱う。
//! どのスレッドからでも呼べる(中で autoreleasepool を張る)。

mod native;
mod port;

use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

pub use native::{create_bookmark, resolve_bookmark};
pub use port::MacBookmarks;

/// ブックマークの作成・解決・文字列からの復元の失敗。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BookmarkError {
    /// フォルダが見つからない(作成時に無い、または解決時に消えている)。
    #[error("フォルダが見つからない")]
    NotFound,
    /// ブックマークのデータが壊れている(空、形式が違う、文字列として不正)。
    #[error("ブックマークのデータが壊れている")]
    Corrupt,
    /// パスが UTF-8 でないため OS に渡せない。
    #[error("パスが UTF-8 ではない")]
    NonUtf8Path,
    /// 上のどれにも当たらない OS のエラー。
    #[error("OS のエラー(ドメイン {domain}、コード {code}): {message}")]
    Os {
        domain: String,
        code: isize,
        message: String,
    },
}

/// 解決の結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// フォルダの現在のパス(実パス)。
    pub path: PathBuf,
    /// ブックマークが古い(移動・改名などのあと)。`true` なら [`create_bookmark`] で作り直す。
    pub stale: bool,
}

/// ブックマークのバイト列を `state.json` に入れる文字列にする(base64 の標準の符号化、パディングあり)。
pub fn encode(data: &[u8]) -> String {
    STANDARD.encode(data)
}

/// [`encode`] の逆。base64 の標準の符号化として不正な文字列(空白・改行を含むものも)は [`BookmarkError::Corrupt`]。
pub fn decode(text: &str) -> Result<Vec<u8>, BookmarkError> {
    STANDARD.decode(text).map_err(|_| BookmarkError::Corrupt)
}

/// `NSCocoaErrorDomain` のエラーを [`BookmarkError`] に振り分ける。
fn classify(domain: String, code: isize, message: String) -> BookmarkError {
    // NSFileNoSuchFileError(4)、NSFileReadNoSuchFileError(260)、NSFileReadCorruptFileError(259)
    const NO_SUCH_FILE: isize = 4;
    const READ_NO_SUCH_FILE: isize = 260;
    const READ_CORRUPT_FILE: isize = 259;
    if domain == "NSCocoaErrorDomain" {
        match code {
            NO_SUCH_FILE | READ_NO_SUCH_FILE => return BookmarkError::NotFound,
            READ_CORRUPT_FILE => return BookmarkError::Corrupt,
            _ => {}
        }
    }
    BookmarkError::Os {
        domain,
        code,
        message,
    }
}

/// OS に渡すパスの文字列。UTF-8 でなければ [`BookmarkError::NonUtf8Path`]。
fn path_str(path: &Path) -> Result<&str, BookmarkError> {
    path.to_str().ok_or(BookmarkError::NonUtf8Path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_maps_known_cocoa_codes() {
        let c = |code| classify("NSCocoaErrorDomain".into(), code, "m".into());
        assert_eq!(c(4), BookmarkError::NotFound);
        assert_eq!(c(260), BookmarkError::NotFound);
        assert_eq!(c(259), BookmarkError::Corrupt);
        assert_eq!(
            c(256),
            BookmarkError::Os {
                domain: "NSCocoaErrorDomain".into(),
                code: 256,
                message: "m".into()
            }
        );
    }

    #[test]
    fn classify_keeps_other_domains_as_os() {
        assert_eq!(
            classify("NSPOSIXErrorDomain".into(), 4, "m".into()),
            BookmarkError::Os {
                domain: "NSPOSIXErrorDomain".into(),
                code: 4,
                message: "m".into()
            }
        );
    }

    #[test]
    fn non_utf8_path_is_rejected() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        let p = Path::new(OsStr::from_bytes(b"/tmp/\xff"));
        assert_eq!(path_str(p), Err(BookmarkError::NonUtf8Path));
        assert_eq!(
            create_bookmark(p),
            Err(BookmarkError::NonUtf8Path),
            "OS に渡す前に止める"
        );
    }

    #[test]
    fn encode_and_decode_roundtrip() {
        let bytes = [0u8, 1, 2, 250, 255];
        assert_eq!(decode(&encode(&bytes)), Ok(bytes.to_vec()));
        assert_eq!(decode(""), Ok(Vec::new()));
    }
}
