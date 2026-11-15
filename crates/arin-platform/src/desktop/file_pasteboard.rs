//! `NSPasteboard`(objc2-app-kit)にファイルの URL を読み書きする。FR-027(Finder と互換のコピー・ペースト)、
//! 設計書 §9.1。
//!
//! 書くときは 1 つのファイルにつき 1 つの項目(`NSURL` をそのまま `writeObjects:`)にする。Finder のコピーと同じ形。
//! URL は名前の文字を保ったまま作る(`fileURLWithPath` は NFD に変えるので使わない。`workspace::file_url_from_str`)。
//! 読むときは項目ごとに `public.file-url`(`NSPasteboardTypeFileURL`)だけを見て、置かれた順に返す。
//! 文字列だけの項目(`file://` で始まる文字列を含む)はファイルとみなさない。
//! Finder はファイル参照の URL(`file:///.file/id=…`)を置くことがあるので、`filePathURL` でパスの URL に直してから読む。
//! 直せないもの(項目がもう無いなど)は飛ばす【仮定】。
//!
//! 書き込み先は作るときに決める(`MacPasteboard` と同じ)。アプリは [`MacFilePasteboard::general`]、
//! テストは [`MacFilePasteboard::named`](終わったら [`MacFilePasteboard::release`])。
//! `NSPasteboard` の参照はスレッドをまたいで持てないので、名前だけを持ち、呼ぶたびに引く。
//! どのスレッドからでも呼べる(中で autoreleasepool を張る)。

use std::path::{Path, PathBuf};

use arin_core::ports::pasteboard::{FilePasteboardPort, PasteboardError};
use objc2::msg_send;
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{NSPasteboard, NSPasteboardType, NSPasteboardTypeFileURL, NSPasteboardWriting};
use objc2_foundation::{NSArray, NSString, NSURL};

use super::workspace::file_url_from_str;

/// 読み書きの先。
#[derive(Debug, Clone, PartialEq, Eq)]
enum Target {
    /// 一般のペーストボード(⌘C・⌘V・Finder が使うもの)。
    General,
    /// 名前付きのペーストボード。
    Named(String),
}

/// `NSPasteboard` による [`FilePasteboardPort`] の実装。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacFilePasteboard {
    target: Target,
}

impl MacFilePasteboard {
    /// 一般のペーストボード(ユーザーのクリップボード)を読み書きする。
    #[must_use]
    pub fn general() -> Self {
        Self {
            target: Target::General,
        }
    }

    /// 名前付きのペーストボードを読み書きする(テスト用)。無ければ最初に使うときに OS が作る。
    /// 同じ名前の `MacPasteboard::named` と同じペーストボードを指す。
    #[must_use]
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            target: Target::Named(name.into()),
        }
    }

    /// 一般のペーストボードを読み書きするものか。
    #[must_use]
    pub fn is_general(&self) -> bool {
        self.target == Target::General
    }

    /// 名前付きのペーストボードを OS から解放する。一般のペーストボードでは何もしない。
    pub fn release(&self) {
        if self.is_general() {
            return;
        }
        autoreleasepool(|_| {
            let board = self.board();
            // objc2-app-kit 0.3.2 は `releaseGlobally` を包んでいないので、メッセージを直接送る
            // SAFETY: `releaseGlobally` は引数を取らず値を返さない `NSPasteboard` のインスタンスメソッドで、
            // 戻り値の型 `()` と一致する。`board` はこの間ずっと保持している。一般のペーストボードには送らない
            let () = unsafe { msg_send![&*board, releaseGlobally] };
        });
    }

    fn board(&self) -> Retained<NSPasteboard> {
        match &self.target {
            Target::General => NSPasteboard::generalPasteboard(),
            Target::Named(name) => NSPasteboard::pasteboardWithName(&NSString::from_str(name)),
        }
    }
}

impl FilePasteboardPort for MacFilePasteboard {
    fn write_file_urls(&self, paths: &[&Path]) -> Result<i64, PasteboardError> {
        // 中身を消す前に全部を確かめる(途中で失敗して、前の中身だけが消えることを避ける)
        let texts = paths
            .iter()
            .map(|p| Ok((absolute_utf8(p)?, p.is_dir())))
            .collect::<Result<Vec<_>, PasteboardError>>()?;
        autoreleasepool(|_| {
            let objects = texts
                .iter()
                .map(|&(t, dir)| {
                    file_url_from_str(t, dir)
                        .map(ProtocolObject::<dyn NSPasteboardWriting>::from_retained)
                        .ok_or_else(|| PasteboardError(format!("URL にできないパス: {t}")))
                })
                .collect::<Result<Vec<Retained<ProtocolObject<dyn NSPasteboardWriting>>>, _>>()?;
            let array = NSArray::from_retained_slice(&objects);
            let board = self.board();
            board.clearContents();
            if board.writeObjects(&array) {
                Ok(count(&board))
            } else {
                Err(PasteboardError(
                    "NSPasteboard がファイルの URL を受け付けなかった".into(),
                ))
            }
        })
    }

    fn read_file_urls(&self) -> Result<Vec<PathBuf>, PasteboardError> {
        autoreleasepool(|_| {
            let Some(items) = self.board().pasteboardItems() else {
                return Ok(Vec::new());
            };
            let paths = items
                .to_vec()
                .into_iter()
                .filter_map(|item| item.stringForType(file_url_type()))
                .filter_map(|s| NSURL::URLWithString(&s))
                .filter(|url| url.isFileURL())
                .filter_map(|url| url.filePathURL())
                .filter_map(|url| url.path())
                .map(|p| PathBuf::from(p.to_string()))
                .collect();
            Ok(paths)
        })
    }

    fn change_count(&self) -> i64 {
        autoreleasepool(|_| count(&self.board()))
    }
}

/// 絶対パスで UTF-8 のものだけを書く(ポートの約束。UTF-8 でない名前は扱わない=計画書 M3 の【仮定】24)。
fn absolute_utf8(path: &Path) -> Result<&str, PasteboardError> {
    if !path.is_absolute() {
        return Err(PasteboardError(format!(
            "絶対パスではない: {}",
            path.display()
        )));
    }
    path.to_str()
        .ok_or_else(|| PasteboardError(format!("UTF-8 でないパス: {}", path.display())))
}

fn count(board: &NSPasteboard) -> i64 {
    // NSInteger(isize)は arm64 では 64 ビットなので i64 にそのまま収まる
    i64::try_from(board.changeCount()).unwrap_or(i64::MAX)
}

/// ファイルの URL の種類(`public.file-url`)。
fn file_url_type() -> &'static NSPasteboardType {
    // SAFETY: AppKit が定義する変更されない定数の NSString で、プログラムの終わりまで有効
    unsafe { NSPasteboardTypeFileURL }
}
