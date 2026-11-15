//! 設定ファイルのポート(設計書 §5.1 `SettingsFilePort`、§1.3 #10、ADR-012 の 1・3・6、計画書 T64b)。
//!
//! 状態フォルダの `settings.toml` を読む口と、**無いときだけ**見本を作る口を置く。設定ファイルは人が書くもので、
//! アプリは読むだけ(上書き・改名・退避・削除の口は作らない。ADR-012 の 3)。実装は `arin-platform`(T65a)。
//!
//! 中身の解釈(UTF-8・TOML)はここではしない。UTF-8 と大きさの確かめは `settings::decode_contents`、
//! TOML の解析は `arin-platform` が行う。失敗の理由は種類だけを持ち、パスや OS の文言は持たない(知らせに出さないため)。

use thiserror::Error;

/// 設定ファイルの名前。状態フォルダの直下に置く(`AppPaths::settings_file`)。
pub const SETTINGS_FILE_NAME: &str = "settings.toml";

/// 設定ファイルの大きさの上限(1 MiB。【仮定】ADR-012 の 3)。これを超えるファイルは読まない。
pub const MAX_SETTINGS_FILE_BYTES: u64 = 1024 * 1024;

/// 設定ファイルを読んだ結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsFileRead {
    /// 設定ファイルが無い。
    Missing,
    /// 設定ファイルの中身。UTF-8 や TOML として正しいかは確かめていない。
    Contents(Vec<u8>),
}

/// 見本を作る口の結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleCreation {
    /// 無かったので見本を作った。
    Created,
    /// もうあった。中身は変えていない。
    AlreadyExists,
}

/// 設定ファイルを扱えなかった理由の種類。パスや OS の文言は持たない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SettingsFileError {
    /// [`MAX_SETTINGS_FILE_BYTES`] を超える。
    #[error("設定ファイルが大きすぎる")]
    TooLarge,
    /// フォルダなど、ふつうのファイルでない。
    #[error("設定ファイルがふつうのファイルではない")]
    NotAFile,
    /// 権限が無い。
    #[error("設定ファイルを扱う権限が無い")]
    PermissionDenied,
    /// それ以外の入出力の失敗。
    #[error("設定ファイルを扱えなかった")]
    Io,
}

/// 状態フォルダの `settings.toml` を読む口。状態フォルダは実装を作るときに渡す。
pub trait SettingsFilePort: Send + Sync {
    /// 設定ファイルを読む。無ければ [`SettingsFileRead::Missing`] を返す。
    ///
    /// シンボリックリンクならリンクの先を読む(ADR-012 の 1)。[`MAX_SETTINGS_FILE_BYTES`] を超える分は読まずに
    /// [`SettingsFileError::TooLarge`] を返す。
    ///
    /// # Errors
    ///
    /// 大きすぎる・ふつうのファイルでない・権限が無い・それ以外の入出力の失敗。
    fn read(&self) -> Result<SettingsFileRead, SettingsFileError>;

    /// 設定ファイルが**無いときだけ**、`contents` で新しく作る(ADR-012 の 6)。同じ名前があれば何もしない
    /// (中身を変えない。ファイルでなくても上書きしない)。
    ///
    /// 状態フォルダが無ければ 0700 で作る(親のフォルダは作らない)。ファイルは 0600。
    ///
    /// # Errors
    ///
    /// 権限が無い・それ以外の入出力の失敗。
    fn create_sample_if_missing(
        &self,
        contents: &[u8],
    ) -> Result<SampleCreation, SettingsFileError>;
}
