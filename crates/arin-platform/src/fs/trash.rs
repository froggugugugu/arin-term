//! ゴミ箱(`FileSystemPort::trash` の中身。計画書 T40c、ADR-007 の (a)、設計書 §5.4「ゴミ箱と Finder の「戻す」」)。
//!
//! 方式: `NSFileManager` の `trashItemAtURL:resultingItemURL:error:`(ADR-007 の (a) の方式 1。【仮定】)。
//! OS に頼むのは [`request_trash`] の 1 か所だけにし、方式を差し替えるときはそこだけを変える。
//!
//! - OS に頼む前に、絶対パスであること、`..` を含まないこと、名前があること、UTF-8 であること、
//!   項目そのものがあること(リンクはたどらない)を確かめる
//! - パスは要素から組み直す(末尾の `/` と `.` を落とす)。末尾に `/` が残ると、リンクの先のフォルダを指してしまう
//! - シンボリックリンクはリンク自体を入れる(URL の末尾に `/` を付けない)
//! - URL はパスのバイト列から作る(`fileURLWithPath` は名前を NFD に変える。`desktop::file_url_from_str`)
//! - ゴミ箱に入れられなかったときは、その失敗をそのまま返す。別の方式や削除に切り替えない
//!   (このモジュールに削除の呼び出しは無い)
//!
//! スレッド: どのスレッドからでも呼べる(`NSFileManager::defaultManager` はスレッドをまたいで使える。
//! objc2-foundation 0.3.2 も `MainThreadMarker` を求めない)。その場で終わる同期の処理で、
//! 中で autoreleasepool を張る。メインスレッドの外(コマンドの非同期タスク)から呼ぶ(設計書 §7.1)。

use std::path::{Component, Path, PathBuf};

use arin_core::ports::fs::FsError;
use objc2::rc::{Retained, autoreleasepool};
use objc2_foundation::{NSError, NSFileManager, NSURL};

use crate::desktop::file_url_from_str;

/// `NSFileNoSuchFileError`(`NSCocoaErrorDomain`)。調べたあとに外から消されたとき。
const NO_SUCH_FILE: isize = 4;
/// `NSCocoaErrorDomain`。
const COCOA_ERROR_DOMAIN: &str = "NSCocoaErrorDomain";

/// OS がゴミ箱に入れられなかった理由。
#[derive(Debug)]
struct OsTrashError {
    domain: String,
    code: isize,
    message: String,
}

/// 項目をゴミ箱に入れ、ゴミ箱の中の場所を返す(分からなければ `None`)。
pub(super) fn trash(path: &Path) -> Result<Option<PathBuf>, FsError> {
    let (path, is_dir) = check_trashable(path)?;
    autoreleasepool(|_| {
        let url = item_url(&path, is_dir)?;
        request_trash(&url).map_err(|e| trash_error(&path, &e))
    })
}

/// OS に頼む前の検査。組み直したパスと、項目そのものがフォルダか(リンクは偽)を返す。
fn check_trashable(path: &Path) -> Result<(PathBuf, bool), FsError> {
    if !path.is_absolute() {
        return Err(FsError::Io(format!("絶対パスではない: {}", path.display())));
    }
    if path.components().any(|c| c == Component::ParentDir) {
        return Err(FsError::Io(format!("`..` を含む: {}", path.display())));
    }
    // 要素から組み直すと、末尾の `/`・途中の `.`・重なった `/` が落ちる
    let normalized: PathBuf = path.components().collect();
    if normalized.file_name().is_none() {
        return Err(FsError::Io(format!("名前が無い: {}", path.display())));
    }
    match std::fs::symlink_metadata(&normalized) {
        Ok(meta) => Ok((normalized, meta.file_type().is_dir())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Err(FsError::NotFound(path.display().to_string()))
        }
        Err(e) => Err(FsError::Io(format!("{}: {e}", path.display()))),
    }
}

/// 項目のファイルの URL(名前の文字を保つ)。UTF-8 でないパスは扱わない(計画書 M3 の【仮定】24)。
fn item_url(path: &Path, is_dir: bool) -> Result<Retained<NSURL>, FsError> {
    let text = path
        .to_str()
        .ok_or_else(|| FsError::Io(format!("UTF-8 でないパス: {}", path.display())))?;
    file_url_from_str(text, is_dir)
        .ok_or_else(|| FsError::Io(format!("URL にできないパス: {}", path.display())))
}

/// OS にゴミ箱へ入れてもらう(方式 1)。方式を差し替えるときはこの関数だけを変える。
fn request_trash(url: &NSURL) -> Result<Option<PathBuf>, OsTrashError> {
    let manager = NSFileManager::defaultManager();
    let mut resulting: Option<Retained<NSURL>> = None;
    match manager.trashItemAtURL_resultingItemURL_error(url, Some(&mut resulting)) {
        Ok(()) => Ok(resulting
            .and_then(|u| u.path())
            .map(|p| PathBuf::from(p.to_string()))),
        Err(error) => Err(os_error(&error)),
    }
}

fn os_error(error: &NSError) -> OsTrashError {
    OsTrashError {
        domain: error.domain().to_string(),
        code: error.code(),
        message: error.localizedDescription().to_string(),
    }
}

/// OS の失敗をポートの失敗にする。調べたあとに消されていたら「見つからない」、それ以外は入出力の失敗。
fn trash_error(path: &Path, error: &OsTrashError) -> FsError {
    if error.domain == COCOA_ERROR_DOMAIN && error.code == NO_SUCH_FILE {
        FsError::NotFound(path.display().to_string())
    } else {
        FsError::Io(format!(
            "{}: ゴミ箱に入れられなかった: {}({} {})",
            path.display(),
            error.message,
            error.domain,
            error.code
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    /// このテストだけが使う一時フォルダ(ゴミ箱には何も入れない)。
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let base = std::fs::canonicalize(std::env::temp_dir()).unwrap();
            let dir = base.join(format!(
                "arin-platform-trash-unit-{name}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn フォルダへのリンクは末尾のスラッシュを落としリンク自体として扱う() {
        let s = Scratch::new("link");
        let target = s.0.join("target");
        std::fs::create_dir(&target).unwrap();
        let link = s.0.join("link");
        symlink(&target, &link).unwrap();
        let (path, is_dir) = check_trashable(Path::new(&format!("{}/./", link.display()))).unwrap();
        assert_eq!(path, link);
        assert!(!is_dir, "リンクの先をたどった");
        autoreleasepool(|_| {
            let url = item_url(&path, is_dir).unwrap();
            let text = url.absoluteString().unwrap().to_string();
            assert!(
                !text.ends_with('/'),
                "リンクの URL の末尾に / が付いた: {text}"
            );
        });
    }

    #[test]
    fn フォルダの_url_は名前を分解した形に変えない() {
        let s = Scratch::new("nfc");
        let dir = s.0.join("ダミー");
        std::fs::create_dir(&dir).unwrap();
        let (path, is_dir) = check_trashable(&dir).unwrap();
        assert!(is_dir);
        autoreleasepool(|_| {
            let url = item_url(&path, is_dir).unwrap();
            assert_eq!(url.path().map(|p| p.to_string()).as_deref(), dir.to_str());
        });
    }

    #[test]
    fn 名前の無いパスは拒否する() {
        assert!(matches!(
            check_trashable(Path::new("/")),
            Err(FsError::Io(_))
        ));
    }

    #[test]
    fn 調べたあとに消されていたら見つからないにする() {
        let gone = OsTrashError {
            domain: COCOA_ERROR_DOMAIN.to_owned(),
            code: NO_SUCH_FILE,
            message: String::new(),
        };
        let denied = OsTrashError {
            domain: COCOA_ERROR_DOMAIN.to_owned(),
            code: 513,
            message: "権限が無い".to_owned(),
        };
        let p = Path::new("/tmp/a");
        assert!(matches!(trash_error(p, &gone), FsError::NotFound(_)));
        assert!(matches!(trash_error(p, &denied), FsError::Io(m) if m.contains("権限が無い")));
    }
}
