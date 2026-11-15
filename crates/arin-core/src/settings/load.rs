//! 設定ファイルを読んだ結果の型(計画書 T64b。ADR-012 の 3「ファイル全体の問題」・4)。
//!
//! TOML の解析は `arin-platform`(T65a)が行い、ここの素朴な形([`SettingsDocument`])にして渡す。
//! `arin-core` は TOML を知らない。値は「文字列か、文字列でないかの印」だけを持つ。

use std::fmt;

use crate::ports::settings_file::{MAX_SETTINGS_FILE_BYTES, SettingsFileError, SettingsFileRead};

/// 設定ファイルの中の位置(どちらも 1 始まり。列は文字の数で数える)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextPosition {
    /// 行。
    pub line: u32,
    /// 列。
    pub column: u32,
}

impl TextPosition {
    /// 行と列から作る。
    #[must_use]
    pub const fn new(line: u32, column: u32) -> Self {
        Self { line, column }
    }

    /// 文字列の中のバイトの位置から、行と列を数える(TOML のクレートの返す位置を、文言を使わずに知らせるため)。
    ///
    /// 改行は `\n` で数える(`\r\n` の `\r` は前の行の末尾)。文字の途中を指されたらその文字の位置、
    /// 末尾より後ろは末尾にする。
    #[must_use]
    pub fn from_byte_offset(text: &str, offset: usize) -> Self {
        let mut end = offset.min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let before = &text[..end];
        let line_start = before.rfind('\n').map_or(0, |i| i + 1);
        let line = before.matches('\n').count() + 1;
        let column = before[line_start..].chars().count() + 1;
        Self {
            line: u32::try_from(line).unwrap_or(u32::MAX),
            column: u32::try_from(column).unwrap_or(u32::MAX),
        }
    }
}

/// 設定の値。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingValue {
    /// 文字列。
    Text(String),
    /// 文字列でない値(数・真偽・配列・日時、またはキーとしてつなげない深さの表)。中身は持たない。
    NotText,
}

/// 節の中の 1 行(「キー = 値」)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingEntry {
    /// キー(書いたまま。入れ子の表を 1 段つないだキーは解析する側が `.` でつなぐ。ADR-012 の 2)。
    pub key: String,
    /// 値。
    pub value: SettingValue,
}

/// 節の中身。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SectionBody {
    /// 表。行を書いた順に並べる。
    Table(Vec<SettingEntry>),
    /// 表でない(`shortcuts = 1` のような最上位の値)。中身は持たない。
    NotTable,
}

/// 設定ファイルの最上位の 1 つの節。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsSection {
    /// 節の名前(書いたまま)。
    pub name: String,
    /// 中身。
    pub body: SectionBody,
}

/// TOML として読めた設定ファイル(節を書いた順に並べる)。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SettingsDocument {
    /// 節。
    pub sections: Vec<SettingsSection>,
}

/// ファイルとして読めなかった理由の種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsUnreadable {
    /// UTF-8 でない。
    NotUtf8,
    /// 大きすぎる([`MAX_SETTINGS_FILE_BYTES`] を超える)。
    TooLarge,
    /// フォルダなど、ふつうのファイルでない。
    NotAFile,
    /// 読む権限が無い。
    PermissionDenied,
    /// それ以外の入出力の失敗。
    Io,
}

impl From<SettingsFileError> for SettingsUnreadable {
    fn from(error: SettingsFileError) -> Self {
        match error {
            SettingsFileError::TooLarge => Self::TooLarge,
            SettingsFileError::NotAFile => Self::NotAFile,
            SettingsFileError::PermissionDenied => Self::PermissionDenied,
            SettingsFileError::Io => Self::Io,
        }
    }
}

impl fmt::Display for SettingsUnreadable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotUtf8 => "UTF-8 の文字ではない",
            Self::TooLarge => "大きすぎる(1 MiB まで)",
            Self::NotAFile => "ファイルではない",
            Self::PermissionDenied => "読む権限が無い",
            Self::Io => "読み取りに失敗した",
        })
    }
}

/// 設定ファイルを読んだ結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsLoad {
    /// ファイルが無い。
    Missing,
    /// TOML として読めた。
    Loaded(SettingsDocument),
    /// ファイルとして読めなかった。
    Unreadable(SettingsUnreadable),
    /// TOML の構文エラー。位置だけを持つ(クレートの文言は該当行の抜粋を含みうるので持たない)。
    Syntax(TextPosition),
}

impl SettingsLoad {
    /// ポートの読み取りの結果から作る。中身があれば [`decode_contents`] で確かめてから `parse`(TOML の解析)に渡す。
    pub fn from_file(
        read: Result<SettingsFileRead, SettingsFileError>,
        parse: impl FnOnce(&str) -> Result<SettingsDocument, TextPosition>,
    ) -> Self {
        match read {
            Ok(SettingsFileRead::Missing) => Self::Missing,
            Ok(SettingsFileRead::Contents(bytes)) => match decode_contents(&bytes) {
                Ok(text) => match parse(text) {
                    Ok(document) => Self::Loaded(document),
                    Err(position) => Self::Syntax(position),
                },
                Err(reason) => Self::Unreadable(reason),
            },
            Err(error) => Self::Unreadable(error.into()),
        }
    }
}

/// 中身のバイト列を、大きさの上限以内の UTF-8 の文字列として取り出す。
///
/// # Errors
///
/// [`MAX_SETTINGS_FILE_BYTES`] を超えるとき([`SettingsUnreadable::TooLarge`])、UTF-8 でないとき
/// ([`SettingsUnreadable::NotUtf8`])。
pub fn decode_contents(bytes: &[u8]) -> Result<&str, SettingsUnreadable> {
    if u64::try_from(bytes.len()).map_or(true, |len| len > MAX_SETTINGS_FILE_BYTES) {
        return Err(SettingsUnreadable::TooLarge);
    }
    std::str::from_utf8(bytes).map_err(|_| SettingsUnreadable::NotUtf8)
}
