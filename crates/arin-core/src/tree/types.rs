//! ツリーで画面とやり取りする型(ts-rs で生成する。計画書 T38a)。
//!
//! ツリーの中身は写しに含めず、画面がフォルダを展開したときに問い合わせる(設計書 §5.2)。
//! 絶対パスは載せない。フォルダは「プロジェクト ID + ルート基準の相対パス」で指す。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::paths::RelPath;
pub use crate::ports::fs::{DirEntry, EntryKind};
use crate::project::ProjectId;

/// フォルダの中身を問い合わせた結果(`list_directory` の応答。T42a)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DirListing {
    /// 読めた。除外と並び順を当てたもの。
    Listed {
        /// 中の項目(並び順のとおり)。
        entries: Vec<DirEntry>,
    },
    /// フォルダ(またはルート)が見つからない。
    NotFound,
    /// 指したものはフォルダではない。
    NotADirectory,
    /// 指したものはルートの外にある(シンボリックリンクで外へ出るなど)。何も読んでいない。
    OutsideRoot,
}

/// ツリーの変更(`tree-changed` イベントの中身。T42b)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TreeChange {
    /// 監視しているプロジェクト。
    pub project_id: ProjectId,
    /// 中身が変わったフォルダ(ルート基準。ルートは空の文字列)。`fullReload` が真なら空。
    pub dirs: Vec<RelPath>,
    /// 全部読み直す(変更が多すぎた、OS が個々の変更を伝えきれなかった)。
    pub full_reload: bool,
}
