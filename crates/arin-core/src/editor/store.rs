//! エディタのタブ(ADR-015 の 3)。プロジェクトごとの並び(開いた順)と、表示中のエディタ。
//!
//! エディタの ID は `editor-<n>` で、起動中は使い回さない。同じプロジェクトの同じ相対パスは 1 つのタブだけ。
//! 表示中のエディタ(`shown`)はプロジェクトごとに 0 か 1 つ。`Some` のとき、タブ表示ではそのエディタを
//! 表示中のターミナルの代わりに出す(どちらを出すかは `Workspace` が決める)。
//! 表示中のエディタを閉じたら右隣のエディタ(無ければ左隣。どちらも無ければ表示中のエディタは無し=ターミナル)を出す。
//! 保存しない(再起動で開き直さない。ADR-015 の【要確認】2 の A)。

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use super::{FileStamp, ReadOnlyReason};
use crate::paths::RelPath;
use crate::project::ProjectId;

/// エディタの識別子。画面とのやり取りでは文字列。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
pub struct EditorId(String);

impl EditorId {
    /// 文字列から作る。中身の検査はしない(存在しない ID は操作のときに `NotFound` になる)。
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// 文字列としての中身。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EditorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// エディタの操作を受け付けられない理由。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EditorError {
    /// その ID のエディタが無い。
    #[error("エディタ {0} が見つからない")]
    NotFound(EditorId),
}

/// エディタのタブ 1 つ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorTab {
    /// 識別子。
    pub id: EditorId,
    /// 持ち主のプロジェクト。
    pub project: ProjectId,
    /// ルート基準の相対パス(拡張子は `.md`・`.markdown`)。
    pub path: RelPath,
    /// 未保存の変更がある(画面が変わり目だけ送る)。
    pub dirty: bool,
    /// 読み取り専用の理由。書けるなら `None`。
    pub read_only: Option<ReadOnlyReason>,
    /// 開いた・最後に保存した・読み直したときの版。
    pub stamp: FileStamp,
}

impl EditorTab {
    /// タブの表示名(ファイル名)。
    #[must_use]
    pub fn label(&self) -> &str {
        self.path.file_name().unwrap_or_default()
    }
}

/// プロジェクトごとのエディタの並びと表示中のエディタ。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EditorStore {
    last_id: u64,
    /// すべてのタブ(開いた順)。プロジェクトの並びはここから絞って作る。
    tabs: Vec<EditorTab>,
    /// プロジェクトごとの表示中のエディタ。
    shown: Vec<(ProjectId, EditorId)>,
}

impl EditorStore {
    /// `project` の `path` を開いて表示中にする。新しく作ったら `(ID, true)`。
    /// 既に開いていれば作らずにそれを表示中にし `(ID, false)`(`stamp`・`read_only` は使わない)。
    pub fn open(
        &mut self,
        project: &ProjectId,
        path: RelPath,
        stamp: FileStamp,
        read_only: Option<ReadOnlyReason>,
    ) -> (EditorId, bool) {
        if let Some(id) = self.find(project, &path).cloned() {
            self.set_shown(project, id.clone());
            return (id, false);
        }
        self.last_id += 1;
        let id = EditorId::new(format!("editor-{}", self.last_id));
        self.tabs.push(EditorTab {
            id: id.clone(),
            project: project.clone(),
            path,
            dirty: false,
            read_only,
            stamp,
        });
        self.set_shown(project, id.clone());
        (id, true)
    }

    /// `project` の `path` のエディタ。開いていなければ `None`。
    #[must_use]
    pub fn find(&self, project: &ProjectId, path: &RelPath) -> Option<&EditorId> {
        self.tabs
            .iter()
            .find(|t| &t.project == project && &t.path == path)
            .map(|t| &t.id)
    }

    /// エディタを閉じ、閉じたタブを返す。表示中だったなら右隣(無ければ左隣)のエディタを表示中にし、
    /// どちらも無ければ表示中のエディタは無しにする。
    ///
    /// # Errors
    ///
    /// その ID のエディタが無いとき(`EditorError::NotFound`)。
    pub fn close(&mut self, id: &EditorId) -> Result<EditorTab, EditorError> {
        let index = self.index(id)?;
        let project = self.tabs[index].project.clone();
        let siblings: Vec<EditorId> = self.tabs(&project).map(|t| t.id.clone()).collect();
        let position = siblings.iter().position(|s| s == id).unwrap_or(0);
        let tab = self.tabs.remove(index);
        if self.shown(&project) == Some(id) {
            let next = siblings
                .get(position + 1)
                .or_else(|| position.checked_sub(1).and_then(|p| siblings.get(p)))
                .cloned();
            match next {
                Some(next) => self.set_shown(&project, next),
                None => self.hide(&project),
            }
        }
        Ok(tab)
    }

    /// プロジェクトのエディタをすべて外し、外したタブを開いた順で返す(プロジェクトの削除)。
    pub fn remove_project(&mut self, project: &ProjectId) -> Vec<EditorTab> {
        self.hide(project);
        let (removed, kept) = std::mem::take(&mut self.tabs)
            .into_iter()
            .partition(|t| &t.project == project);
        self.tabs = kept;
        removed
    }

    /// そのエディタを、持ち主のプロジェクトの表示中にする。
    ///
    /// # Errors
    ///
    /// その ID のエディタが無いとき(`EditorError::NotFound`)。
    pub fn show(&mut self, id: &EditorId) -> Result<(), EditorError> {
        let project = self.tabs[self.index(id)?].project.clone();
        self.set_shown(&project, id.clone());
        Ok(())
    }

    /// `project` の表示中のエディタを無しにする(ターミナルのタブを選んだ・作った)。エディタは閉じない。
    pub fn hide(&mut self, project: &ProjectId) {
        self.shown.retain(|(p, _)| p != project);
    }

    /// `project` の表示中のエディタ。無ければ `None`。
    #[must_use]
    pub fn shown(&self, project: &ProjectId) -> Option<&EditorId> {
        self.shown
            .iter()
            .find(|(p, _)| p == project)
            .map(|(_, id)| id)
    }

    /// 未保存の印を替える。変わったら `Ok(true)`。
    ///
    /// # Errors
    ///
    /// その ID のエディタが無いとき(`EditorError::NotFound`)。
    pub fn set_dirty(&mut self, id: &EditorId, dirty: bool) -> Result<bool, EditorError> {
        let tab = self.tab_mut(id)?;
        let changed = tab.dirty != dirty;
        tab.dirty = dirty;
        Ok(changed)
    }

    /// 版を替える(保存した・読み直した・`touch` だけだった)。
    ///
    /// # Errors
    ///
    /// その ID のエディタが無いとき(`EditorError::NotFound`)。
    pub fn set_stamp(&mut self, id: &EditorId, stamp: FileStamp) -> Result<(), EditorError> {
        self.tab_mut(id)?.stamp = stamp;
        Ok(())
    }

    /// `project` のエディタ(開いた順)。
    pub fn tabs<'a>(&'a self, project: &'a ProjectId) -> impl Iterator<Item = &'a EditorTab> + 'a {
        self.tabs.iter().filter(move |t| &t.project == project)
    }

    /// その ID のエディタ。
    #[must_use]
    pub fn tab(&self, id: &EditorId) -> Option<&EditorTab> {
        self.tabs.iter().find(|t| &t.id == id)
    }

    /// すべてのエディタ(開いた順)。
    pub fn iter(&self) -> impl Iterator<Item = &EditorTab> {
        self.tabs.iter()
    }

    fn tab_mut(&mut self, id: &EditorId) -> Result<&mut EditorTab, EditorError> {
        let index = self.index(id)?;
        Ok(&mut self.tabs[index])
    }

    fn index(&self, id: &EditorId) -> Result<usize, EditorError> {
        self.tabs
            .iter()
            .position(|t| &t.id == id)
            .ok_or_else(|| EditorError::NotFound(id.clone()))
    }

    fn set_shown(&mut self, project: &ProjectId, id: EditorId) {
        match self.shown.iter_mut().find(|(p, _)| p == project) {
            Some(entry) => entry.1 = id,
            None => self.shown.push((project.clone(), id)),
        }
    }
}
