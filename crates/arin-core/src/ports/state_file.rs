//! 状態ファイルのポート(設計書 §6.2「書き込み方法」、計画書 M2 の決定 (d))。
//!
//! 状態フォルダの `state.json` を読む口、原子的に書く口、壊れたときに別の名前へ改名して退避する口を置く。
//! 実装は `arin-platform`(T31a。`arin_platform::storage::FsStateFile`)。
//!
//! 中身の解釈(JSON・版の判定)と退避の名前の決定は `arin-core` の `persistence`(T30a)が行う。
//! ここは中身をバイト列のまま運ぶだけで、入出力はしない。

use thiserror::Error;

/// 状態ファイルの名前。状態フォルダの直下に置く。
pub const STATE_FILE_NAME: &str = "state.json";

/// 状態ファイルを読んだ結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateFileRead {
    /// 状態ファイルが無い(初回の起動)。
    Missing,
    /// 状態ファイルの中身。UTF-8 や JSON として正しいかは確かめていない。
    Contents(Vec<u8>),
}

/// 状態ファイルの操作の失敗。原因は実装側の文言をそのまま持つ。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum StateFileError {
    /// 退避の名前が状態フォルダの直下の 1 つの名前になっていない(空、`.`・`..`、`/` や NUL を含む)。
    #[error("退避の名前が正しくない: {0:?}")]
    InvalidBackupName(String),
    /// 退避の名前のファイルがすでにある。上書きせずに止めた。
    #[error("退避の名前のファイルがすでにある: {0}")]
    BackupExists(String),
    /// それ以外の入出力の失敗(権限が無い、退避する状態ファイルが無いなど)。
    #[error("状態ファイルを扱えなかった: {0}")]
    Io(String),
}

/// 状態フォルダの `state.json` を読み書きする口。状態フォルダは実装を作るときに渡す。
pub trait StateFilePort: Send + Sync {
    /// 状態ファイルを読む。無ければ `StateFileRead::Missing` を返す。
    ///
    /// # Errors
    ///
    /// 無い以外の理由で読めないとき(`StateFileError::Io`)。
    fn read(&self) -> Result<StateFileRead, StateFileError>;

    /// 状態ファイルを原子的に置き換える。
    ///
    /// 同じフォルダの一時ファイルに書いて `fsync` してから名前を変える。途中で失敗しても元のファイルは残る。
    /// 状態フォルダが無ければ 0700 で作る(親のフォルダは作らない)。ファイルは 0600。
    ///
    /// # Errors
    ///
    /// フォルダを作れない・書き込めないなど(`StateFileError::Io`)。
    fn write(&self, contents: &[u8]) -> Result<(), StateFileError>;

    /// 状態ファイルを、状態フォルダの直下の `backup_name` に改名する。中身は変えない。
    ///
    /// # Errors
    ///
    /// 名前が正しくないとき(`StateFileError::InvalidBackupName`)、同じ名前がすでにあるとき
    /// (`StateFileError::BackupExists`。上書きしない)、それ以外で改名できないとき(`StateFileError::Io`)。
    fn move_aside(&self, backup_name: &str) -> Result<(), StateFileError>;
}
