//! Finder で表示・既定のアプリで開く(`NSWorkspace`。objc2-app-kit)。FR-018、設計書 §5.4。
//!
//! OS に頼む前に、パスが絶対パスであることと、対象があることをこちらで確かめる(無いパスを OS に渡さない)。
//! 確かめる処理([`check_revealable`]・[`check_openable`])と、OS に頼む処理(`request_*`)を分け、
//! 自動テストは前者だけを通す(Finder やアプリを開かない)。
//!
//! スレッド: どのスレッドからでも呼べる。`NSWorkspace` は AppKit の中でメインスレッド専用の印が付いていない型で、
//! objc2-app-kit 0.3.2 も `MainThreadMarker` を求めない。中で autoreleasepool を張る。
//! `openURL:` は起動の受け付けまで少し待つことがあるので、呼び出し側(`src-tauri`)はメインスレッドの外から呼ぶのがよい【仮定】。

use std::path::Path;

use arin_core::ports::desktop::{DesktopError, DesktopPort};
use objc2::rc::{Retained, autoreleasepool};
use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSArray, NSString, NSURL};

/// `NSWorkspace` による [`DesktopPort`] の実装。状態を持たない。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MacDesktop;

impl MacDesktop {
    /// 作る。
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl DesktopPort for MacDesktop {
    fn reveal_in_finder(&self, path: &Path) -> Result<(), DesktopError> {
        check_revealable(path)?;
        request_reveal(path)
    }

    fn open_with_default_app(&self, path: &Path) -> Result<(), DesktopError> {
        check_openable(path)?;
        request_open(path)
    }
}

/// Finder で表示する前の検査。絶対パスで、項目そのものがあること(リンクはたどらない。
/// 先が無いシンボリックリンクも、リンク自体を選んで表示できる)。
///
/// # Errors
///
/// 絶対パスでないとき(`DesktopError::Failed`)、項目が無いとき(`DesktopError::NotFound`)。
pub fn check_revealable(path: &Path) -> Result<(), DesktopError> {
    require_absolute(path)?;
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(()),
        Err(e) => Err(missing_or_failed(path, &e)),
    }
}

/// 既定のアプリで開く前の検査。絶対パスで、リンクをたどった先があること。
///
/// # Errors
///
/// 絶対パスでないとき(`DesktopError::Failed`)、先が無いとき(`DesktopError::NotFound`)。
pub fn check_openable(path: &Path) -> Result<(), DesktopError> {
    require_absolute(path)?;
    match std::fs::metadata(path) {
        Ok(_) => Ok(()),
        Err(e) => Err(missing_or_failed(path, &e)),
    }
}

fn require_absolute(path: &Path) -> Result<(), DesktopError> {
    if path.is_absolute() {
        Ok(())
    } else {
        Err(DesktopError::Failed(format!(
            "絶対パスではない: {}",
            path.display()
        )))
    }
}

fn missing_or_failed(path: &Path, e: &std::io::Error) -> DesktopError {
    if e.kind() == std::io::ErrorKind::NotFound {
        DesktopError::NotFound(path.display().to_string())
    } else {
        DesktopError::Failed(format!("{}: {e}", path.display()))
    }
}

/// パスをファイルの URL にする。UTF-8 でないパス(APFS では作れない。計画書 M3 の【仮定】24)は扱わない。
fn file_url(path: &Path) -> Result<Retained<NSURL>, DesktopError> {
    let text = path
        .to_str()
        .ok_or_else(|| DesktopError::Failed(format!("UTF-8 でないパス: {}", path.display())))?;
    file_url_from_str(text, path.is_dir())
        .ok_or_else(|| DesktopError::Failed(format!("URL にできないパス: {}", path.display())))
}

/// 絶対パスの文字列から、文字をそのまま保ったファイルの URL を作る(ペーストボードの書き込みでも使う)。
///
/// `NSURL::fileURLWithPath` は名前を分解した形(NFD。`ダ` → `タ` + 濁点)に変えるので使わない。
/// 変えられた名前がペーストボードに載ると、貼り付け先で別の名前のファイルができてしまう(git では別のファイル)。
/// ここではバイト列を百分率符号化して `URLWithString` で作る。`dir` が真なら Finder と同じく末尾に `/` を付ける。
pub(super) fn file_url_from_str(path: &str, dir: bool) -> Option<Retained<NSURL>> {
    if !path.starts_with('/') {
        return None;
    }
    let mut url = String::with_capacity(path.len() * 3 + 8);
    url.push_str("file://");
    for b in path.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'.' | b'_' | b'~') {
            url.push(char::from(b));
        } else {
            url.push_str(&format!("%{b:02X}"));
        }
    }
    if dir && !url.ends_with('/') {
        url.push('/');
    }
    NSURL::URLWithString(&NSString::from_str(&url))
}

/// Finder に、項目を選んだ状態で表示させる(実際に Finder が前に出る)。
fn request_reveal(path: &Path) -> Result<(), DesktopError> {
    autoreleasepool(|_| {
        let url = file_url(path)?;
        let urls = NSArray::from_retained_slice(&[url]);
        // 戻り値が無く、失敗を知る手段が無い(Finder が受け付けなくても黙る)
        NSWorkspace::sharedWorkspace().activateFileViewerSelectingURLs(&urls);
        Ok(())
    })
}

/// 既定のアプリで開かせる(実際にアプリが起動する)。
fn request_open(path: &Path) -> Result<(), DesktopError> {
    autoreleasepool(|_| {
        let url = file_url(path)?;
        if NSWorkspace::sharedWorkspace().openURL(&url) {
            Ok(())
        } else {
            Err(DesktopError::Failed(format!(
                "既定のアプリで開けなかった: {}",
                path.display()
            )))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ファイルの_url_は空白と日本語を含むパスをそのまま表す() {
        let path = Path::new("/Users/someone/日本語 フォルダ/a b#c?.ts");
        autoreleasepool(|_| {
            let url = file_url(path).unwrap();
            assert!(url.isFileURL());
            assert_eq!(url.path().map(|p| p.to_string()).as_deref(), path.to_str());
        });
    }

    #[test]
    fn ファイルの_url_は名前を分解した形に変えない() {
        // `ダ` は 1 文字(NFC)のまま。fileURLWithPath は `タ` と濁点の 2 文字(NFD)に変える
        let path = "/tmp/フォルダ";
        autoreleasepool(|_| {
            let url = file_url_from_str(path, false).unwrap();
            assert_eq!(url.path().map(|p| p.to_string()).as_deref(), Some(path));
        });
    }

    #[test]
    fn フォルダの_url_は末尾にスラッシュを付けパスには付けない() {
        autoreleasepool(|_| {
            let url = file_url_from_str("/tmp/a b", true).unwrap();
            assert_eq!(
                url.absoluteString().map(|s| s.to_string()).as_deref(),
                Some("file:///tmp/a%20b/")
            );
            assert_eq!(
                url.path().map(|p| p.to_string()).as_deref(),
                Some("/tmp/a b")
            );
        });
    }

    #[test]
    fn 相対パスはファイルの_url_にしない() {
        assert!(file_url_from_str("src/a.ts", false).is_none());
    }

    #[test]
    fn utf8_でないパスは_url_にしない() {
        use std::os::unix::ffi::OsStrExt;
        let path = Path::new(std::ffi::OsStr::from_bytes(b"/tmp/\xff"));
        assert!(matches!(file_url(path), Err(DesktopError::Failed(_))));
    }
}
