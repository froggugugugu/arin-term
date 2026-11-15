//! 相対パス・絶対パス・挿入の文字列の整形(設計書 §5.1 の `PathFormatter`、§7.1「パスをターミナルへ」、
//! §15.2 #6、計画書 T38b、【仮定】16・17)。
//!
//! - 相対パス: ルート基準で `/` 区切り。先頭に `./` を付けず、フォルダの末尾に `/` を付けない。ルートは空の文字列
//! - 絶対パス: ルート(絶対パス。末尾の `/` と重なった `/` はそろえる)に相対パスの名前を足したもの
//! - 挿入の文字列: 同じプロジェクトのターミナルなら相対、違うなら絶対(§15.2 #6 の A)。各パスを
//!   `shell::escape_shell_word` でエスケープして空白でつなぎ、末尾に半角スペースを 1 つ付ける。Enter は含めない

use std::path::{Component, Path, PathBuf};

use thiserror::Error;

use super::relative::{PathKind, RelPath};
use super::shell::escape_shell_word;
use crate::project::ProjectId;

/// パスを整形できなかった。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PathFormatError {
    /// ルートが絶対パスでない。
    #[error("ルートが絶対パスでない: {0:?}")]
    RootNotAbsolute(PathBuf),
    /// ルートに `..` か `.` の要素がある。
    #[error("ルートに `..`・`.` の要素がある: {0:?}")]
    RootNotNormal(PathBuf),
    /// 絶対パスが UTF-8 でなく、文字列にできない。
    #[error("UTF-8 でないパスは文字列にできない: {0:?}")]
    NotUtf8(PathBuf),
    /// 挿入するパスが 1 つも無い。
    #[error("挿入するパスが無い")]
    NoPaths,
}

/// ルート基準の相対パスの文字列(パスのコピー用。エスケープしない。ルートは空の文字列)。
#[must_use]
pub fn relative_text(path: &RelPath) -> String {
    path.as_str().to_owned()
}

/// ルートと相対パスをつないだ絶対パス。ルート自体はルートのまま(末尾の `/` は付けない)。
///
/// # Errors
///
/// ルートが絶対パスでないとき(`RootNotAbsolute`)、ルートに `..`・`.` の要素があるとき(`RootNotNormal`)。
pub fn absolute_path(root: &Path, path: &RelPath) -> Result<PathBuf, PathFormatError> {
    if !root.is_absolute() {
        return Err(PathFormatError::RootNotAbsolute(root.to_path_buf()));
    }
    let mut out = PathBuf::new();
    for component in root.components() {
        match component {
            Component::RootDir | Component::Normal(_) => out.push(component),
            Component::Prefix(_) | Component::CurDir | Component::ParentDir => {
                return Err(PathFormatError::RootNotNormal(root.to_path_buf()));
            }
        }
    }
    for name in path.components() {
        out.push(name);
    }
    Ok(out)
}

/// 絶対パスの文字列(パスのコピー用。エスケープしない)。
///
/// # Errors
///
/// `absolute_path` の失敗に加えて、UTF-8 でないとき(`NotUtf8`。【仮定】24)。
pub fn absolute_text(root: &Path, path: &RelPath) -> Result<String, PathFormatError> {
    absolute_path(root, path)?
        .into_os_string()
        .into_string()
        .map_err(|os| PathFormatError::NotUtf8(PathBuf::from(os)))
}

/// パスのコピーの文字列(FR-015。エスケープせず、末尾に空白を付けない)。
///
/// # Errors
///
/// 絶対パスのとき、`absolute_text` と同じ。相対パスは失敗しない。
pub fn path_text(root: &Path, path: &RelPath, kind: PathKind) -> Result<String, PathFormatError> {
    match kind {
        PathKind::Relative => Ok(relative_text(path)),
        PathKind::Absolute => absolute_text(root, path),
    }
}

/// 挿入する文字列を相対にするか絶対にするか(§15.2 #6 の A)。項目のプロジェクトと、挿入先のターミナルの
/// プロジェクトが同じなら相対、違えば絶対。
#[must_use]
pub fn insertion_path_kind(source: &ProjectId, destination: &ProjectId) -> PathKind {
    if source == destination {
        PathKind::Relative
    } else {
        PathKind::Absolute
    }
}

/// ターミナルへ挿入する文字列(FR-016・FR-024)。各パスをエスケープして空白でつなぎ、末尾に半角スペースを
/// 1 つ付ける。改行などの制御文字のバイトと Enter を含まない。
///
/// 相対パスでは、ルート自体は `.`、`-` で始まるものは先頭に `./` を付ける(コマンドのオプションとして
/// 読まれないように。【仮定】)。
///
/// # Errors
///
/// `paths` が空のとき(`NoPaths`)。絶対パスのとき、`absolute_text` と同じ。
pub fn insertion_text(
    root: &Path,
    paths: &[RelPath],
    kind: PathKind,
) -> Result<String, PathFormatError> {
    if paths.is_empty() {
        return Err(PathFormatError::NoPaths);
    }
    let mut out = String::new();
    for path in paths {
        let text = match kind {
            PathKind::Relative if path.is_root() => ".".to_owned(),
            PathKind::Relative if path.as_str().starts_with('-') => format!("./{}", path.as_str()),
            PathKind::Relative => relative_text(path),
            PathKind::Absolute => absolute_text(root, path)?,
        };
        out.push_str(&escape_shell_word(&text));
        out.push(' ');
    }
    Ok(out)
}
