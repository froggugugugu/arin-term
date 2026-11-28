//! `NSPasteboard`(objc2-app-kit)に文字列を書く。
//!
//! 書き込み先は作るときに決める。アプリは [`MacPasteboard::general`](ユーザーのクリップボード)、
//! テストは [`MacPasteboard::named`](名前付きのペーストボード。終わったら [`MacPasteboard::release`])を使う。
//! `NSPasteboard` の参照はスレッドをまたいで持てないので、名前だけを持ち、呼ぶたびに引く。
//! どのスレッドからでも呼べる(中で autoreleasepool を張る)。

use arin_core::ports::pasteboard::{PasteboardError, PasteboardPort};
use objc2::msg_send;
use objc2::rc::{Retained, autoreleasepool};
use objc2_app_kit::{NSPasteboard, NSPasteboardType, NSPasteboardTypeString};
use objc2_foundation::NSString;

/// 書き込み先。
#[derive(Debug, Clone, PartialEq, Eq)]
enum Target {
    /// 一般のペーストボード(⌘C・⌘V が使うもの)。
    General,
    /// 名前付きのペーストボード。
    Named(String),
}

/// `NSPasteboard` による [`PasteboardPort`] の実装。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacPasteboard {
    target: Target,
}

impl MacPasteboard {
    /// 一般のペーストボード(ユーザーのクリップボード)に書く。
    #[must_use]
    pub fn general() -> Self {
        Self {
            target: Target::General,
        }
    }

    /// 名前付きのペーストボードに書く(テスト用)。無ければ最初に使うときに OS が作る。
    #[must_use]
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            target: Target::Named(name.into()),
        }
    }

    /// 一般のペーストボードに書くものか。
    #[must_use]
    pub fn is_general(&self) -> bool {
        self.target == Target::General
    }

    /// プレーンテキストの中身を読む。無ければ `None`(アプリは [`PasteboardPort::read_text`] を通して呼ぶ)。
    #[must_use]
    pub fn read_text(&self) -> Option<String> {
        autoreleasepool(|_| {
            self.board()
                .stringForType(string_type())
                .map(|s| s.to_string())
        })
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

impl PasteboardPort for MacPasteboard {
    fn write_text(&self, text: &str) -> Result<(), PasteboardError> {
        autoreleasepool(|_| {
            let board = self.board();
            board.clearContents();
            if board.setString_forType(&NSString::from_str(text), string_type()) {
                Ok(())
            } else {
                Err(PasteboardError(
                    "NSPasteboard が文字列を受け付けなかった".into(),
                ))
            }
        })
    }

    fn read_text(&self) -> Result<Option<String>, PasteboardError> {
        // 同じ名前の固有のメソッド(`Option` を返す)を呼ぶ。固有のメソッドが trait のものより先に選ばれる。
        Ok(MacPasteboard::read_text(self))
    }
}

/// プレーンテキストの種類(`public.utf8-plain-text`)。
fn string_type() -> &'static NSPasteboardType {
    // SAFETY: AppKit が定義する変更されない定数の NSString で、プログラムの終わりまで有効
    unsafe { NSPasteboardTypeString }
}
