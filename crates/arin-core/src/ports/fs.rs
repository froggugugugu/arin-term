//! ファイルシステムのポート(設計書 §5.1、§5.4「上書きしない移動・コピー」「ゴミ箱と Finder の「戻す」」、計画書 T38a)。
//!
//! 実装は `arin-platform/src/fs/`(T40a・T40c)。パスはどれも絶対パスで、ルートと相対パスをつなぐのは呼び出し側
//! (`fileops`・`src-tauri`)が行う。範囲の検査(ルートの外に出ないか)もこのポートの外で行う。
//! 書き込む操作(作成・移動・コピー・ゴミ箱)に渡すパスは**実パス**(`real_dir` で解決した親 + 名前)にする。
//! 実装は途中の要素にシンボリックリンクがあれば失敗する(最後の要素のリンクはリンク自体を対象にする)。
//!
//! **消す操作と上書きする操作は置かない**(FR-028・FR-030)。作成・移動・コピーは「既にあれば失敗する」だけで、
//! 削除はゴミ箱だけ。「置き換える」は呼び出し側が既存をゴミ箱に入れてから置く(設計書 §1.3 #11)。
//!
//! 項目の種類と項目(`EntryKind`・`DirEntry`)は画面にもそのまま渡すので、ts-rs で生成する。
//! 機能モジュール(`tree`・`fileops`)はここの型を共有する(設計書 §3.2)。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

/// 項目の種類。シンボリックリンクはリンク自体を 1 つの項目とし、指す先でファイルかフォルダかを分ける。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum EntryKind {
    /// ふつうのファイル。
    File,
    /// フォルダ。
    Directory,
    /// フォルダを指すシンボリックリンク。展開しない(ルートの外を操作させないため。設計書 §15.2 #8)。
    DirectoryLink,
    /// ファイルを指すシンボリックリンク。
    FileLink,
    /// それ以外(壊れたシンボリックリンク、ソケット、デバイスなど)。
    Other,
}

/// フォルダの中の項目 1 つ。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DirEntry {
    /// 名前(`/` を含まない 1 つの名前)。
    pub name: String,
    /// 種類。
    pub kind: EntryKind,
}

/// ファイルシステムの操作の失敗。原因は実装側の文言をそのまま持つ。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FsError {
    /// 対象(移動・コピーでは元)が無い。
    #[error("見つからない: {0}")]
    NotFound(String),
    /// 作る先・移す先・写す先に同じ名前がすでにある。何も変えていない。
    #[error("同じ名前がすでにある: {0}")]
    AlreadyExists(String),
    /// フォルダであるべきところがフォルダではない。
    #[error("フォルダではない: {0}")]
    NotADirectory(String),
    /// 別のボリュームへの移動はできない(`EXDEV`。コピーしてから消すことはしない。計画書 M3 の【仮定】14)。
    #[error("別のディスクへは移動できない: {0}")]
    CrossDevice(String),
    /// それ以外の入出力の失敗(権限が無いなど)。
    #[error("ファイルを扱えなかった: {0}")]
    Io(String),
}

/// ファイルとフォルダを読み、上書きせずに作る・移す・写す口と、ゴミ箱に入れる口。
pub trait FileSystemPort: Send + Sync {
    /// フォルダの中身(名前と種類)を返す。順は決めない(並べ替えは `tree` が行う)。
    /// 名前が UTF-8 でない項目は返さない(計画書 M3 の【仮定】24)。
    ///
    /// # Errors
    ///
    /// 無いとき(`FsError::NotFound`)、フォルダでないとき(`FsError::NotADirectory`)、読めないとき(`FsError::Io`)。
    fn read_dir(&self, dir: &Path) -> Result<Vec<DirEntry>, FsError>;

    /// 項目の種類を返す。リンクはたどらず、リンク自体を 1 つの項目として扱う(種類だけは指す先で分ける)。
    ///
    /// # Errors
    ///
    /// 無いとき(`FsError::NotFound`)、調べられないとき(`FsError::Io`)。
    fn entry_kind(&self, path: &Path) -> Result<EntryKind, FsError>;

    /// フォルダの実パス(シンボリックリンクをすべて解決した絶対パス)を返す。範囲の検査に使う。
    ///
    /// # Errors
    ///
    /// 無いとき(`FsError::NotFound`)、フォルダでないとき(`FsError::NotADirectory`)、調べられないとき(`FsError::Io`)。
    fn real_dir(&self, dir: &Path) -> Result<PathBuf, FsError>;

    /// 空のファイルを新しく作る。同じ名前がすでにあれば何も変えずに失敗する。
    ///
    /// # Errors
    ///
    /// 同じ名前があるとき(`FsError::AlreadyExists`)、親が無いとき(`FsError::NotFound`)、それ以外(`FsError::Io`)。
    fn create_file(&self, path: &Path) -> Result<(), FsError>;

    /// フォルダを新しく作る(親は作らない)。同じ名前がすでにあれば何も変えずに失敗する。
    ///
    /// # Errors
    ///
    /// 同じ名前があるとき(`FsError::AlreadyExists`)、親が無いとき(`FsError::NotFound`)、それ以外(`FsError::Io`)。
    fn create_dir(&self, path: &Path) -> Result<(), FsError>;

    /// `from` を `to` へ移す(名称変更を含む。フォルダは中身ごと)。`to` がすでにあれば何も変えずに失敗する。
    /// ただし `to` が `from` と同じ項目(大文字・小文字だけの違い)なら名前を変える(計画書 M3 の【仮定】15)。
    ///
    /// # Errors
    ///
    /// `from` が無いとき(`FsError::NotFound`)、`to` があるとき(`FsError::AlreadyExists`)、
    /// 別のボリュームのとき(`FsError::CrossDevice`)、それ以外(`FsError::Io`)。
    fn move_entry(&self, from: &Path, to: &Path) -> Result<(), FsError>;

    /// `from` を `to` へ写す(フォルダは中身ごと)。`to` がすでにあれば何も変えずに失敗する。`from` は変えない。
    ///
    /// # Errors
    ///
    /// `from` が無いとき(`FsError::NotFound`)、`to` があるとき(`FsError::AlreadyExists`)、それ以外(`FsError::Io`)。
    fn copy_entry(&self, from: &Path, to: &Path) -> Result<(), FsError>;

    /// `path` の項目(リンクはたどらない)と、フォルダ `dir` が同じボリュームにあるか(移動が `CrossDevice` に
    /// ならないか)。「置き換える」で、置けないと分かっている移動の前に既存をゴミ箱に入れないために使う。
    /// 読むだけで、ファイルシステムを変えない。
    ///
    /// # Errors
    ///
    /// どちらかが無いとき(`FsError::NotFound`)、調べられないとき(`FsError::Io`)。
    fn same_volume(&self, path: &Path, dir: &Path) -> Result<bool, FsError>;

    /// 項目をゴミ箱に入れる(Finder の「戻す」で戻せる方式。ADR-007 の (a))。
    /// ゴミ箱の中の場所が分かる方式ならそれを、分からなければ `None` を返す。
    /// ゴミ箱に入れられなかったときに、別の方式や削除に切り替えない。
    ///
    /// # Errors
    ///
    /// 無いとき(`FsError::NotFound`)、ゴミ箱に入れられないとき(`FsError::Io`)。
    fn trash(&self, path: &Path) -> Result<Option<PathBuf>, FsError>;
}
