//! 文書の規則(ADR-015 の 5): 対象の拡張子、大きさの上限、文字の検査と BOM・改行、版、読み取り専用の理由。
//!
//! 版(`FileStamp`)は `stat` の 4 つ(装置・inode・大きさ・更新時刻)と中身の 64 ビットの要約。要約は
//! `std::hash::DefaultHasher`(`new()` は固定の鍵なので、同じ中身なら同じ値)で取る。改ざんの検出ではなく、
//! 外での変更の検出に使う(暗号学的なハッシュの依存を足さない。ADR-015 の却下した案)。

use std::hash::{DefaultHasher, Hasher};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::paths::RelPath;

/// 開ける文書の大きさの上限(1 MiB。BOM を含むバイト数)。
pub const DOCUMENT_LIMIT_BYTES: u64 = 1024 * 1024;

/// 対象の拡張子(大文字・小文字を問わない)。
pub const MARKDOWN_EXTENSIONS: [&str; 2] = ["md", "markdown"];

/// UTF-8 の BOM。
const BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// 拡張子が `.md`・`.markdown`(大文字・小文字を問わない)のパスか。拡張子だけの名前(`.md`)は対象にしない。
#[must_use]
pub fn is_markdown_path(path: &RelPath) -> bool {
    path.file_name()
        .and_then(|name| name.rsplit_once('.'))
        .is_some_and(|(stem, ext)| {
            !stem.is_empty()
                && MARKDOWN_EXTENSIONS
                    .iter()
                    .any(|e| ext.eq_ignore_ascii_case(e))
        })
}

/// 文書の改行(画面は CodeMirror の `lineSeparator` をこれに合わせる)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum LineEnding {
    /// `\n`。
    Lf,
    /// `\r\n`。文書に 1 つでもあればこれ(混在は保存で 1 種類にそろう)。
    Crlf,
}

/// 読んだバイト列を文字にした結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    /// 本文(先頭の BOM は外してある)。
    pub text: String,
    /// 先頭に BOM があった(保存で付け直す)。
    pub bom: bool,
    /// 改行。
    pub line_ending: LineEnding,
}

/// 文書として扱えない理由(中身の規則。開けない理由のうち、ファイルの種類によるものはポートの失敗)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum DocumentError {
    /// 上限(`DOCUMENT_LIMIT_BYTES`)を超える。
    #[error("文書が大きすぎる({size} バイト。上限は 1 MiB)")]
    TooLarge {
        /// バイト数。
        size: u64,
    },
    /// UTF-8 として不正な並びか NUL を含む。
    #[error("文書が UTF-8 の文字ではない")]
    NotText,
}

/// 読んだバイト列を本文にする。大きさ → BOM → NUL と UTF-8 の順に調べる。
///
/// # Errors
///
/// 上限を超えるとき(`TooLarge`)、NUL を含むか UTF-8 として不正なとき(`NotText`)。
pub fn decode_document(bytes: &[u8]) -> Result<Decoded, DocumentError> {
    let size = bytes.len() as u64;
    if size > DOCUMENT_LIMIT_BYTES {
        return Err(DocumentError::TooLarge { size });
    }
    let (bom, body) = match bytes.strip_prefix(&BOM) {
        Some(rest) => (true, rest),
        None => (false, bytes),
    };
    if body.contains(&0) {
        return Err(DocumentError::NotText);
    }
    let text = std::str::from_utf8(body)
        .map_err(|_| DocumentError::NotText)?
        .to_owned();
    let line_ending = if text.contains("\r\n") {
        LineEnding::Crlf
    } else {
        LineEnding::Lf
    };
    Ok(Decoded {
        text,
        bom,
        line_ending,
    })
}

/// 本文を保存するバイト列にする(`bom` なら先頭に BOM を付け直す)。改行は本文のまま(画面がそろえて送る)。
#[must_use]
pub fn encode_document(text: &str, bom: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() + if bom { BOM.len() } else { 0 });
    if bom {
        out.extend_from_slice(&BOM);
    }
    out.extend_from_slice(text.as_bytes());
    out
}

/// 中身の 64 ビットの要約(決定的。変更の検出だけに使う)。
#[must_use]
pub fn digest(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    hasher.write(bytes);
    hasher.finish()
}

/// ファイルの `stat` のうち、版に使う 4 つ。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileStat {
    /// 装置番号。
    pub dev: u64,
    /// inode。
    pub ino: u64,
    /// 大きさ(バイト)。
    pub size: u64,
    /// 更新時刻(UNIX 時刻のナノ秒)。
    pub mtime_ns: i64,
}

/// 文書の版(開いた・保存したときのもの)。外で変わったかを比べるのに使う。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileStamp {
    /// `stat` の 4 つ。
    pub stat: FileStat,
    /// 中身の要約(`digest`)。
    pub digest: u64,
}

impl FileStamp {
    /// `stat` と読んだ中身から作る。
    #[must_use]
    pub fn new(stat: FileStat, bytes: &[u8]) -> Self {
        Self {
            stat,
            digest: digest(bytes),
        }
    }

    /// `stat` の 4 つが同じか(同じなら中身を読まずに「変わっていない」とみなす)。
    #[must_use]
    pub fn same_stat(&self, stat: &FileStat) -> bool {
        self.stat == *stat
    }
}

/// 2 つの版が同じ中身か。`stat` の 4 つが同じなら同じ。違っても大きさと要約が同じなら同じ
/// (`touch`・同じ中身での置き換え。呼び出し側は版を新しいものに持ち直す)。
#[must_use]
pub fn same_content(a: &FileStamp, b: &FileStamp) -> bool {
    a.same_stat(&b.stat) || (a.stat.size == b.stat.size && a.digest == b.digest)
}

/// 読み取り専用で開く理由(ADR-015 の 5。保存は改名で置き換えるので、置き換えると困るもの・置き換えられないもの)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum ReadOnlyReason {
    /// ほかの名前(ハードリンク)がある。置き換えるとリンクが切れる。
    HardLink,
    /// 持ち主が自分でない(置き換えると持ち主を写せない)。
    NotOwner,
    /// 自分が書けない。
    NotWritable,
    /// フォルダに書けない(一時ファイルを作れない)。
    FolderNotWritable,
}
