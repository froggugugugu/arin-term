//! ルート基準の相対パス(設計書 §5.2・§7.2「画面から届く相対パス」、計画書 T38a)。
//!
//! 画面はファイルを「プロジェクト ID + ルート基準の相対パス」で指す。画面から届く経路はこの型だけにし、
//! 作るときに絶対パス・`..`・`.`・空の要素・NUL を拒否する。シンボリックリンクでの脱出は、ここではなく
//! ファイル操作の範囲の検査(`fileops`)がファイルシステムのポートで実パスを調べて拒否する。

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;
use ts_rs::TS;

/// 区切りの文字。
const SEPARATOR: char = '/';

/// ルート基準の相対パス。区切りは `/`、空の文字列はルート自体。画面とのやり取りでは文字列。
///
/// 必ず検査を通った形(`/` で始まらない、`..`・`.`・空の要素が無い、末尾が `/` でない、NUL が無い)。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, TS)]
pub struct RelPath(String);

/// 相対パスにできない文字列。元の文字列を持つ。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RelPathError {
    /// `/` で始まる(絶対パス)。
    #[error("ルート基準の相対パスにできない(絶対パス): {0:?}")]
    Absolute(String),
    /// `..` か `.` の要素がある。
    #[error("ルート基準の相対パスにできない(`..`・`.` の要素): {0:?}")]
    DotComponent(String),
    /// 空の要素がある(`a//b`、末尾の `/`、空の名前を足そうとした)。
    #[error("ルート基準の相対パスにできない(空の要素): {0:?}")]
    EmptyComponent(String),
    /// NUL を含む。
    #[error("ルート基準の相対パスにできない(NUL を含む): {0:?}")]
    Nul(String),
    /// 足そうとした名前が `/` を含む(`join` は 1 つの名前だけを足す)。
    #[error("1 つの名前ではない(`/` を含む): {0:?}")]
    Separator(String),
}

impl RelPath {
    /// ルート自体(空の相対パス)。
    #[must_use]
    pub fn root() -> Self {
        Self(String::new())
    }

    /// 画面から届いた文字列から作る。空の文字列はルート。
    ///
    /// # Errors
    ///
    /// NUL を含むとき(`Nul`)、`/` で始まるとき(`Absolute`)、`..`・`.` の要素があるとき(`DotComponent`)、
    /// 空の要素か末尾の `/` があるとき(`EmptyComponent`)。複数に当たるときはこの順で先のものを返す。
    pub fn parse(text: &str) -> Result<Self, RelPathError> {
        if text.contains('\0') {
            return Err(RelPathError::Nul(text.to_owned()));
        }
        if text.starts_with(SEPARATOR) {
            return Err(RelPathError::Absolute(text.to_owned()));
        }
        if text.is_empty() {
            return Ok(Self::root());
        }
        let components = text.split(SEPARATOR);
        if components.clone().any(|c| c == "." || c == "..") {
            return Err(RelPathError::DotComponent(text.to_owned()));
        }
        if components.clone().any(str::is_empty) {
            return Err(RelPathError::EmptyComponent(text.to_owned()));
        }
        Ok(Self(text.to_owned()))
    }

    /// 文字列としての中身(ルートは空の文字列)。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// ルート自体か。
    #[must_use]
    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    /// 名前の並び(ルートでは何も返さない)。
    pub fn components(&self) -> impl Iterator<Item = &str> {
        self.0.split(SEPARATOR).filter(|c| !c.is_empty())
    }

    /// 親の相対パスと最後の名前に分ける。ルートは親を持たないので `None`。
    #[must_use]
    pub fn split_parent(&self) -> Option<(Self, &str)> {
        if self.is_root() {
            return None;
        }
        Some(match self.0.rsplit_once(SEPARATOR) {
            Some((parent, name)) => (Self(parent.to_owned()), name),
            None => (Self::root(), self.0.as_str()),
        })
    }

    /// 親の相対パス。ルートは `None`。
    #[must_use]
    pub fn parent(&self) -> Option<Self> {
        self.split_parent().map(|(parent, _)| parent)
    }

    /// 最後の名前。ルートは `None`。
    #[must_use]
    pub fn file_name(&self) -> Option<&str> {
        self.split_parent().map(|(_, name)| name)
    }

    /// 1 つの名前を子として足した相対パス。
    ///
    /// # Errors
    ///
    /// `name` が NUL を含むとき(`Nul`)、`/` を含むとき(`Separator`)、`.`・`..` のとき(`DotComponent`)、
    /// 空のとき(`EmptyComponent`)。
    pub fn join(&self, name: &str) -> Result<Self, RelPathError> {
        if name.contains('\0') {
            return Err(RelPathError::Nul(name.to_owned()));
        }
        if name.contains(SEPARATOR) {
            return Err(RelPathError::Separator(name.to_owned()));
        }
        if name == "." || name == ".." {
            return Err(RelPathError::DotComponent(name.to_owned()));
        }
        if name.is_empty() {
            return Err(RelPathError::EmptyComponent(String::new()));
        }
        if self.is_root() {
            Ok(Self(name.to_owned()))
        } else {
            Ok(Self(format!("{}{SEPARATOR}{name}", self.0)))
        }
    }
}

impl fmt::Display for RelPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for RelPath {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(serde::de::Error::custom)
    }
}

/// パスの種類(パスのコピーで相対か絶対かを選ぶ。FR-015)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum PathKind {
    /// ルート基準の相対パス。
    Relative,
    /// 絶対パス(ルートと相対パスをつないだもの)。
    Absolute,
}
