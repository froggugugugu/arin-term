//! Markdown のエディタの規則と型(ADR-015 の 3・5、計画書 T81a)。
//!
//! - `document`: 対象の拡張子・大きさの上限・文字の検査と BOM・改行・版(`FileStamp`)・読み取り専用の理由
//! - `store`: エディタのタブ(`EditorTab`)と、プロジェクトごとの並びと表示中(`EditorStore`)
//! - `outcome`: 文書のコマンドの応答とイベントの中身(T81d)
//!
//! 入出力はしない。ファイルを読み書きするのは `arin-platform` の `DocumentPort` の実装(`ports::document`)。

mod document;
mod outcome;
mod store;

pub use document::{
    DOCUMENT_LIMIT_BYTES, Decoded, DocumentError, FileStamp, FileStat, LineEnding,
    MARKDOWN_EXTENSIONS, ReadOnlyReason, decode_document, digest, encode_document,
    is_markdown_path, same_content,
};
pub use outcome::{
    CheckDocumentOutcome, DocumentNotAllowedReason, EditorCloseRequest, OpenDocumentOutcome,
    SaveDocumentOutcome,
};
pub use store::{EditorError, EditorId, EditorStore, EditorTab};
