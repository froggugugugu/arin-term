//! ルートのパスの字面の規則(FR-010 の二重登録の判定、§15.2 #7 のゴミ箱、FR-064 の表示)。
//!
//! ファイルシステムには触れない。シンボリックリンクの解決と大文字・小文字の表記の統一は
//! `RootLocatorPort::canonical_dir`(`arin-platform`)が済ませてから渡す前提で、ここでは字面だけを整える。

use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

use super::ProjectError;

/// ゴミ箱のフォルダの名前(ホームの `.Trash`、外付けのボリュームの `.Trashes`)。
const TRASH_NAMES: [&str; 2] = [".Trash", ".Trashes"];

/// 絶対パスの字面を整える。重なった `/`、途中の `.`、末尾の `/` を取り除く。
///
/// # Errors
///
/// UTF-8 でないとき、相対パスのとき、`..` を含むとき(字面だけでは解決できないため)。
pub(crate) fn normalize_root(path: &Path) -> Result<PathBuf, ProjectError> {
    let text = path.to_str().ok_or(ProjectError::NotUtf8Path)?;
    if !path.has_root() {
        return Err(ProjectError::RelativePath(text.to_owned()));
    }
    // `components` は重なった `/`・途中の `.`・末尾の `/` を返さない(`.` を返すのは相対パスの先頭だけで、ここには来ない)。
    let mut normalized = PathBuf::new();
    for component in path.components() {
        if component == Component::ParentDir {
            return Err(ProjectError::ParentComponent(text.to_owned()));
        }
        normalized.push(component.as_os_str());
    }
    Ok(normalized)
}

/// パスがゴミ箱の中を指しているか。
pub(crate) fn is_in_trash(path: &Path) -> bool {
    path.components().any(|component| match component {
        Component::Normal(name) => TRASH_NAMES.iter().any(|trash| name == OsStr::new(trash)),
        _ => false,
    })
}

/// フォルダ名(表示名の既定)。ルート(`/`)そのものは `/`。
pub(crate) fn folder_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.to_string_lossy().into_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// 表示用のパス。ホームの中は `~` で始める。ホームが分からない・絶対パスでないときは絶対パスのまま。
pub(crate) fn display_path(path: &Path, home: Option<&Path>) -> String {
    let home = home.and_then(|home| normalize_root(home).ok());
    match home
        .as_deref()
        .and_then(|home| path.strip_prefix(home).ok())
    {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Some(rest) => format!("~/{}", rest.to_string_lossy()),
        None => path.to_string_lossy().into_owned(),
    }
}
