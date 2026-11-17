//! 監視で届いた変更を「変わったフォルダ」にまとめる(設計書 §5.1 の `ChangeBatch`、§7.1「外部でのファイル変更」、
//! FR-014、PRD C7、計画書 M3 の【仮定】19・T38c)。
//!
//! 監視(`WatcherPort`)から届いた絶対パスの束を、ルート基準の「中身が変わったフォルダ」(項目の親のフォルダ)の
//! 集合にする。次のものは捨てる:
//!
//! - ルートの外のパス(要素ごとに比べる。`/x/app2` は `/x/app` の外)、相対パス
//! - `..`・`.` を含むパス、UTF-8 でない名前を含むパス(【仮定】24)
//! - 隠しファイル表示が OFF のときの、除外の中(どれかの要素が除外の名前)の変更
//!
//! ルート自体の変更はルートの読み直しにする。変わったフォルダが上限(`MAX_CHANGED_DIRS`)を超えたら、
//! 一覧ではなく「全部読み直し」の印だけにする。OS が伝えきれなかった印(`WatchBatch::rescan`)も全部読み直し。
//! 結果のフォルダは相対パスの文字列のバイトの順に並べ、重複を除く(決定的)。

use std::collections::BTreeSet;
use std::path::{Component, Path};

use crate::paths::RelPath;
use crate::ports::watch::WatchBatch;
use crate::project::ProjectId;
use crate::tree::exclusion::ExclusionRule;
use crate::tree::types::TreeChange;

/// 変わったフォルダの一覧で送る上限(【仮定】19)。これを超えたら全部読み直しにする。
pub const MAX_CHANGED_DIRS: usize = 1000;

/// まとめた変更。`full_reload` が真なら `dirs` は空。両方とも空なら送るものは無い。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChangeBatch {
    /// 中身が変わったフォルダ(ルート基準。ルートは空の相対パス)。バイトの順で重複なし。
    pub dirs: Vec<RelPath>,
    /// 全部読み直す。
    pub full_reload: bool,
}

impl ChangeBatch {
    /// 全部読み直しの印だけのもの。
    #[must_use]
    pub fn full_reload() -> Self {
        Self {
            dirs: Vec::new(),
            full_reload: true,
        }
    }

    /// 送るものが無いか(変わったフォルダが無く、全部読み直しでもない)。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.dirs.is_empty() && !self.full_reload
    }

    /// プロジェクト ID を付けて `tree-changed` イベントの中身にする(T42b)。
    #[must_use]
    pub fn into_tree_change(self, project_id: ProjectId) -> TreeChange {
        TreeChange {
            project_id,
            dirs: self.dirs,
            full_reload: self.full_reload,
        }
    }
}

/// 監視の束を、ルート基準の変わったフォルダにまとめる。
///
/// `root` は監視を張ったルートの絶対パス(監視が届けるパスと同じ形。実パスで張ったなら実パス)。
#[must_use]
pub fn summarize_changes(root: &Path, batch: &WatchBatch, rule: &ExclusionRule) -> ChangeBatch {
    if batch.rescan {
        return ChangeBatch::full_reload();
    }
    let mut dirs = BTreeSet::new();
    for path in &batch.paths {
        let Some(item) = relative_to_root(root, path) else {
            continue;
        };
        if rule.is_hidden_path(&item) {
            continue;
        }
        // 項目の親のフォルダが変わった。ルート自体の変更はルートの読み直しにする。
        dirs.insert(item.parent().unwrap_or_else(RelPath::root));
        if dirs.len() > MAX_CHANGED_DIRS {
            return ChangeBatch::full_reload();
        }
    }
    ChangeBatch {
        dirs: dirs.into_iter().collect(),
        full_reload: false,
    }
}

/// ルートの中の絶対パスをルート基準の相対パスにする。ルートの外・相対パス・`..`・`.`・UTF-8 でない名前は `None`。
fn relative_to_root(root: &Path, path: &Path) -> Option<RelPath> {
    if !root.is_absolute() || !path.is_absolute() {
        return None;
    }
    // `..` を含むパスは、字面ではルートの中に見えても外を指しうるので捨てる。
    // (`Path::components` は途中の `.` を読み飛ばすので、文字列の要素でも確かめる。)
    if path
        .as_os_str()
        .as_encoded_bytes()
        .split(|b| *b == b'/')
        .any(|c| c == b".." || c == b".")
    {
        return None;
    }
    let rest = path.strip_prefix(root).ok()?;
    let mut rel = RelPath::root();
    for component in rest.components() {
        let Component::Normal(name) = component else {
            return None;
        };
        rel = rel.join(name.to_str()?).ok()?;
    }
    Some(rel)
}
