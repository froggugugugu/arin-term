//! ペーストボードのポート(FR-064。ルートの絶対パスをコピーする)。
//!
//! 実装は `arin-platform`(T31c。`NSPasteboard`)。書き込み先(一般のペーストボードか、テスト用の名前付きのものか)は
//! 実装を作るときに決め、このポートには出さない。

use thiserror::Error;

/// ペーストボードに書けなかった。原因は実装側の文言をそのまま持つ。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("ペーストボードに書けなかった: {0}")]
pub struct PasteboardError(pub String);

/// ペーストボードに文字列を書く口。
pub trait PasteboardPort: Send + Sync {
    /// 中身を消して、`text` をプレーンテキストとして 1 つだけ置く。
    ///
    /// # Errors
    ///
    /// OS が書き込みを受け付けなかったとき。
    fn write_text(&self, text: &str) -> Result<(), PasteboardError>;
}
