//! タブの並びと表示中のタブ(設計書 §6.1 の `TerminalStore`、FR-039・FR-044、§15.2 #3)。
//!
//! プロジェクトごとにタブを順に持ち、0 枚も許す(C-21)。プロジェクトを持たないタブ(検証画面)も扱う。
//! ターミナルの ID は `terminal-<n>` で、起動中は使い回さない。表示名は「ターミナル N」(プロジェクト内の通し番号。
//! 計画書 M2 の【仮定】9)。タブを閉じたら右隣(無ければ左)が表示中になり、前後の切替は端で回り込む(【仮定】17)。
//! シェルが終わったタブは自動で閉じる。検証画面の設定では閉じない(【仮定】5)。
//!
//! 起動の仕様・状態(`TerminalStatus`)・cwd は持たない(`Workspace` が持つ。T32b)。保存もしない(§6.1)。

use super::{TerminalError, TerminalId};
use crate::project::ProjectId;

/// シェルが終わったタブの扱い。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExitPolicy {
    /// 自動で閉じる(既定。§15.2 #3 の A)。
    #[default]
    CloseTab,
    /// 閉じずに残す(9 枚の検証画面。M1 と同じく「終了しました」を出す)。
    KeepTab,
}

/// シェルが終わったときにストアがしたこと。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitAction {
    /// タブを閉じた。セッションの後片付けは呼び出し側が行う。
    Closed(TerminalTab),
    /// タブを残した。
    Kept,
}

/// タブ 1 枚。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalTab {
    /// ターミナルの識別子。
    pub id: TerminalId,
    /// 持ち主のプロジェクト。検証画面のタブは `None`。
    pub project: Option<ProjectId>,
    /// プロジェクト内の通し番号(1 から。閉じても使い回さない)。
    pub number: u32,
    /// 表示名(「ターミナル N」)。
    pub label: String,
}

/// 1 つの持ち主(プロジェクト、または持ち主なし)のタブの並び。
#[derive(Debug, Clone, PartialEq, Eq)]
struct TabGroup {
    project: Option<ProjectId>,
    tabs: Vec<TerminalTab>,
    active: Option<TerminalId>,
    last_number: u32,
}

/// プロジェクトごとのタブの並びと表示中のタブ。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TerminalStore {
    exit_policy: ExitPolicy,
    last_id: u64,
    groups: Vec<TabGroup>,
}

impl TerminalStore {
    /// 空のストアを作る。
    #[must_use]
    pub fn new(exit_policy: ExitPolicy) -> Self {
        Self {
            exit_policy,
            ..Self::default()
        }
    }

    /// シェルが終わったタブの扱い。
    #[must_use]
    pub fn exit_policy(&self) -> ExitPolicy {
        self.exit_policy
    }

    /// `project` の末尾にタブを足し、表示中にする。新しいターミナルの ID を返す。
    pub fn add(&mut self, project: Option<&ProjectId>) -> TerminalId {
        self.last_id += 1;
        let id = TerminalId::new(format!("terminal-{}", self.last_id));
        let index = match self.group_index(project) {
            Some(index) => index,
            None => {
                self.groups.push(TabGroup {
                    project: project.cloned(),
                    tabs: Vec::new(),
                    active: None,
                    last_number: 0,
                });
                self.groups.len() - 1
            }
        };
        let group = &mut self.groups[index];
        group.last_number += 1;
        group.tabs.push(TerminalTab {
            id: id.clone(),
            project: project.cloned(),
            number: group.last_number,
            label: format!("ターミナル {}", group.last_number),
        });
        group.active = Some(id.clone());
        id
    }

    /// タブを閉じ、閉じたタブを返す。表示中だったなら右隣(無ければ左)を表示中にする。最後の 1 枚なら 0 枚になる。
    ///
    /// # Errors
    ///
    /// その ID のタブが無いとき(`TerminalError::NotFound`)。
    pub fn close(&mut self, id: &TerminalId) -> Result<TerminalTab, TerminalError> {
        let (g, t) = self.locate(id)?;
        let group = &mut self.groups[g];
        let tab = group.tabs.remove(t);
        if group.active.as_ref() == Some(id) {
            let next = t.min(group.tabs.len().saturating_sub(1));
            group.active = group.tabs.get(next).map(|tab| tab.id.clone());
        }
        Ok(tab)
    }

    /// シェルが終わった。設定に従ってタブを閉じるか残す。
    ///
    /// # Errors
    ///
    /// その ID のタブが無いとき(`TerminalError::NotFound`)。
    pub fn on_shell_exited(&mut self, id: &TerminalId) -> Result<ExitAction, TerminalError> {
        match self.exit_policy {
            ExitPolicy::CloseTab => self.close(id).map(ExitAction::Closed),
            ExitPolicy::KeepTab => self.locate(id).map(|_| ExitAction::Kept),
        }
    }

    /// プロジェクトのタブをすべて外し、外した ID をタブの順で返す(プロジェクトの削除)。番号も忘れる。
    pub fn remove_project(&mut self, project: &ProjectId) -> Vec<TerminalId> {
        match self.group_index(Some(project)) {
            Some(index) => self
                .groups
                .remove(index)
                .tabs
                .into_iter()
                .map(|tab| tab.id)
                .collect(),
            None => Vec::new(),
        }
    }

    /// そのタブを表示中にする。
    ///
    /// # Errors
    ///
    /// その ID のタブが無いとき(`TerminalError::NotFound`)。
    pub fn activate(&mut self, id: &TerminalId) -> Result<(), TerminalError> {
        let (g, _) = self.locate(id)?;
        self.groups[g].active = Some(id.clone());
        Ok(())
    }

    /// 次のタブを表示中にし、表示中の ID を返す。末尾の次は先頭。0 枚なら `None`。
    pub fn select_next(&mut self, project: Option<&ProjectId>) -> Option<TerminalId> {
        self.step(project, 1)
    }

    /// 前のタブを表示中にし、表示中の ID を返す。先頭の前は末尾。0 枚なら `None`。
    pub fn select_previous(&mut self, project: Option<&ProjectId>) -> Option<TerminalId> {
        self.step(project, -1)
    }

    /// `project` のタブ(順のとおり)。タブを持ったことが無ければ空。
    #[must_use]
    pub fn tabs(&self, project: Option<&ProjectId>) -> &[TerminalTab] {
        self.group(project).map_or(&[], |g| g.tabs.as_slice())
    }

    /// `project` の表示中のタブ。0 枚なら `None`。
    #[must_use]
    pub fn active(&self, project: Option<&ProjectId>) -> Option<&TerminalId> {
        self.group(project).and_then(|g| g.active.as_ref())
    }

    /// その ID のタブ。
    #[must_use]
    pub fn tab(&self, id: &TerminalId) -> Option<&TerminalTab> {
        self.locate(id).ok().map(|(g, t)| &self.groups[g].tabs[t])
    }

    /// すべてのタブ(持ち主が初めてタブを持った順、その中はタブの順)。
    pub fn iter(&self) -> impl Iterator<Item = &TerminalTab> {
        self.groups.iter().flat_map(|g| g.tabs.iter())
    }

    fn step(&mut self, project: Option<&ProjectId>, delta: isize) -> Option<TerminalId> {
        let index = self.group_index(project)?;
        let group = &mut self.groups[index];
        let active = group.active.as_ref()?;
        let len = group.tabs.len();
        let current = group.tabs.iter().position(|t| &t.id == active)?;
        let next = (current + len).wrapping_add_signed(delta) % len;
        group.active = Some(group.tabs[next].id.clone());
        group.active.clone()
    }

    fn group_index(&self, project: Option<&ProjectId>) -> Option<usize> {
        self.groups
            .iter()
            .position(|g| g.project.as_ref() == project)
    }

    fn group(&self, project: Option<&ProjectId>) -> Option<&TabGroup> {
        self.group_index(project).map(|index| &self.groups[index])
    }

    fn locate(&self, id: &TerminalId) -> Result<(usize, usize), TerminalError> {
        self.groups
            .iter()
            .enumerate()
            .find_map(|(g, group)| {
                group
                    .tabs
                    .iter()
                    .position(|tab| &tab.id == id)
                    .map(|t| (g, t))
            })
            .ok_or_else(|| TerminalError::NotFound(id.clone()))
    }
}
