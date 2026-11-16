//! Markdown の文書を読み、置き換え、新しく作る口(ADR-015 の 5、計画書 T81c)。実装は `arin-platform/src/fs/document.rs`。
//!
//! パスはどれも絶対パスで、ルートと相対パスをつなぐのと範囲の検査(`PathScopeGuard`)・拡張子の検査
//! (`editor::is_markdown_path`)・中身の検査(`editor::decode_document`)は呼び出し側が行う。
//! 実装は親を途中のリンクを許さずに開いて fd 相対で扱い、最後の要素のリンクは開かない。
//!
//! **置き換えは `replace` の 1 つだけ**(`docs/development-patterns.md` §1「削除と上書きの経路を作らない」の例外。
//! ADR-015 の 5 の条件): 同じフォルダの新しい一時ファイル(`.arin-save-…`)に書き終えてから改名で置き換え、
//! その場で切り詰めて書かない。ハードリンク・書けない・持ち主が違うファイルは置き換えない。消す操作は置かない
//! (一時ファイルを片付けられなかったら残して知らせる)。
//!
//! 失敗の文言は実装側のもので、**絶対パスを含む**。画面へはそのまま渡さない。

use std::path::Path;

use thiserror::Error;

use crate::editor::{FileStamp, ReadOnlyReason};

/// 読んだ文書。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadDocument {
    /// 中身(上限 `DOCUMENT_LIMIT_BYTES` 以下。文字としての検査はまだしていない)。
    pub bytes: Vec<u8>,
    /// 版。
    pub stamp: FileStamp,
    /// 読み取り専用で開く理由。書けるなら `None`。
    pub read_only: Option<ReadOnlyReason>,
}

/// 置き換えの結果(失敗ではないもの)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplaceOutcome {
    /// 置き換えた。置き換えたあとの版。
    Replaced(FileStamp),
    /// 渡した版と中身が違う(外で変わった)。何も書いていない。いまの版を返す。
    Changed(FileStamp),
    /// 対象が無い(外で消えた・名前が変わった)。何も書いていない(作るなら `create_new`)。
    Missing,
}

/// 文書を扱えなかった理由。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DocumentAccessError {
    /// 無い。
    #[error("見つからない: {0}")]
    NotFound(String),
    /// シンボリックリンク(リンク自体を開かない)。
    #[error("シンボリックリンクは開かない: {0}")]
    Link(String),
    /// フォルダ。
    #[error("フォルダは開かない: {0}")]
    Directory(String),
    /// ふつうのファイルでない(ソケット・デバイスなど)。
    #[error("ふつうのファイルではない: {0}")]
    NotAFile(String),
    /// 大きさが上限(`DOCUMENT_LIMIT_BYTES`)を超える。
    #[error("大きすぎる({size} バイト)")]
    TooLarge {
        /// バイト数。
        size: u64,
    },
    /// 作る先に同じ名前がすでにある。何も変えていない。
    #[error("同じ名前がすでにある: {0}")]
    AlreadyExists(String),
    /// 読み取り専用のものは置き換えない。何も変えていない。
    #[error("読み取り専用なので置き換えない: {0:?}")]
    ReadOnly(ReadOnlyReason),
    /// それ以外の入出力の失敗(途中のフォルダがリンク・権限が無い・一時ファイルを作れないなど)。
    /// 一時ファイルを残したときは、その場所を文言に含める(元のファイルは変わっていない)。
    #[error("文書を扱えなかった: {0}")]
    Io(String),
}

/// 文書を読む・置き換える・作る口。
pub trait DocumentPort: Send + Sync {
    /// 文書を読む(ふつうのファイルだけ。リンクはたどらない)。読み取り専用の理由も調べる
    /// (ハードリンクがある → 持ち主が自分でない → 自分が書けない → フォルダに書けない の順で最初のもの)。
    ///
    /// # Errors
    ///
    /// 無い(`NotFound`)、リンク(`Link`)、フォルダ(`Directory`)、ふつうのファイルでない(`NotAFile`)、
    /// 上限を超える(`TooLarge`。中身を読まない)、それ以外(`Io`)。
    fn read(&self, path: &Path) -> Result<ReadDocument, DocumentAccessError>;

    /// いまの版を調べる(外で変わったかの確認。要約のために中身を読むが返さない)。
    ///
    /// # Errors
    ///
    /// `read` と同じ。
    fn stat(&self, path: &Path) -> Result<FileStamp, DocumentAccessError>;

    /// 文書を `bytes` で置き換える。`expected` を渡したら、いまの中身がその版と同じとき
    /// (`editor::same_content`)だけ置き換え、違えば何も書かずに `Changed` を返す。`None` なら版を比べない
    /// (本人が「上書き保存」を選んだ)。権限・ACL・拡張属性は元のファイルから写す。
    ///
    /// # Errors
    ///
    /// 読み取り専用のとき(`ReadOnly`)、`bytes` が上限を超えるとき(`TooLarge`)、リンク・フォルダ・
    /// ふつうのファイルでないとき、それ以外(`Io`。一時ファイルを残したら場所を文言に含め、知らせ先にも渡す)。
    /// どの場合も元のファイルは変わっていない。
    fn replace(
        &self,
        path: &Path,
        bytes: &[u8],
        expected: Option<&FileStamp>,
    ) -> Result<ReplaceOutcome, DocumentAccessError>;

    /// 文書を新しく作る(消えていた文書を本人が「同じ名前で作る」を選んだ)。同じ名前があれば上書きしない。
    /// 作った版を返す。
    ///
    /// # Errors
    ///
    /// 同じ名前があるとき(`AlreadyExists`)、上限を超えるとき(`TooLarge`)、それ以外(`Io`)。
    fn create_new(&self, path: &Path, bytes: &[u8]) -> Result<FileStamp, DocumentAccessError>;
}
