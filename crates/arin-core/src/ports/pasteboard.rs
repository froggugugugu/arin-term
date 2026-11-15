//! ペーストボードのポート。
//!
//! - `PasteboardPort`: 文字列を書く口(FR-064。ルートの絶対パスをコピーする。M2)
//! - `FilePasteboardPort`: ファイルの URL を読み書きする口と変更番号(FR-027。Finder と互換のコピー・ペースト。M3)
//!
//! 2 つを別の trait にしたのは、文字列だけを使う既存の偽物(`src-tauri/tests/` など)を書き換えずに済ませるため。
//! 実装は `arin-platform`(T31c・T40d。`NSPasteboard`)。書き込み先(一般のペーストボードか、テスト用の名前付きのものか)は
//! 実装を作るときに決め、このポートには出さない。

use std::path::{Path, PathBuf};

use thiserror::Error;

/// ペーストボードを読み書きできなかった。原因は実装側の文言をそのまま持つ。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("ペーストボードを読み書きできなかった: {0}")]
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

/// ペーストボードのファイルの URL を読み書きする口。切り取りは、書いたときの変更番号と、ペーストのときの
/// 変更番号が同じかで判定する(設計書 §9.1、計画書 M3 の【仮定】13)。
pub trait FilePasteboardPort: Send + Sync {
    /// 中身を消して、`paths`(絶対パス)をファイルの URL として順のまま置く。書いたあとの変更番号を返す。
    ///
    /// # Errors
    ///
    /// OS が書き込みを受け付けなかったとき。
    fn write_file_urls(&self, paths: &[&Path]) -> Result<i64, PasteboardError>;

    /// いまあるファイルの URL を、置かれた順のまま絶対パスで返す。ファイルの URL が無ければ空。
    ///
    /// # Errors
    ///
    /// 読めなかったとき。
    fn read_file_urls(&self) -> Result<Vec<PathBuf>, PasteboardError>;

    /// いまの変更番号(中身が置き換わるたびに変わる。`NSPasteboard` の `changeCount`)。
    fn change_count(&self) -> i64;
}
