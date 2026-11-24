//! タブの並びと表示中のタブ(設計書 §6.1 の `TerminalStore`、FR-039・FR-044、§15.2 #3)。
//!
//! プロジェクトごとにタブを順に持ち、0 枚も許す(C-21)。タブは必ずどれかのプロジェクトのもの
//! (M1 の 9 枚の検証画面の「持ち主の無いタブ」は M4 の T49d で外した)。
//! ターミナルの ID は `terminal-<n>` で、起動中は使い回さない。表示名は「ターミナル N」(プロジェクト内の通し番号。
//! 計画書 M2 の【仮定】9)。タブを閉じたら右隣(無ければ左)が表示中になり、前後の切替は端で回り込む(【仮定】17)。
//! シェルが終わったタブは自動で閉じる(§15.2 #3 の A。閉じるのは `Workspace::mark_exited` が `close` で行う)。
//!
//! アテンションの印(FR-055。設計書 §7.1「アテンション」の 3)をタブごとに持つ。印はタブと一緒に消える
//! (閉じる・プロジェクトの削除)。立てるか・消すかの判定は `Workspace` が行う(計画書 T48b)。
//!
//! 起動の仕様・状態(`TerminalStatus`)・cwd は持たない(`Workspace` が持つ。T32b)。保存もしない(§6.1)。

use super::{TerminalError, TerminalId};
use crate::project::ProjectId;
use crate::reorder::move_item;

/// タブ 1 枚。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalTab {
    /// ターミナルの識別子。
    pub id: TerminalId,
    /// 持ち主のプロジェクト。
    pub project: ProjectId,
    /// プロジェクト内の通し番号(1 から。閉じても使い回さない)。
    pub number: u32,
    /// 表示名(「ターミナル N」)。
    pub label: String,
    /// アテンションの印(表示していないあいだに BEL・OSC 9・OSC 777 があった)。新しいタブでは偽。
    /// 変えるのは `set_attention`・`clear_attention` だけ。
    pub attention: bool,
}

/// 1 つのプロジェクトのタブの並び。
#[derive(Debug, Clone, PartialEq, Eq)]
struct TabGroup {
    project: ProjectId,
    tabs: Vec<TerminalTab>,
    active: Option<TerminalId>,
    last_number: u32,
}

/// プロジェクトごとのタブの並びと表示中のタブ。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TerminalStore {
    last_id: u64,
    groups: Vec<TabGroup>,
}

impl TerminalStore {
    /// `project` の末尾にタブを足し、表示中にする。新しいターミナルの ID を返す。
    pub fn add(&mut self, project: &ProjectId) -> TerminalId {
        self.last_id += 1;
        let id = TerminalId::new(format!("terminal-{}", self.last_id));
        let index = match self.group_index(project) {
            Some(index) => index,
            None => {
                self.groups.push(TabGroup {
                    project: project.clone(),
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
            project: project.clone(),
            number: group.last_number,
            label: format!("ターミナル {}", group.last_number),
            attention: false,
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

    /// プロジェクトのタブをすべて外し、外した ID をタブの順で返す(プロジェクトの削除)。番号も忘れる。
    pub fn remove_project(&mut self, project: &ProjectId) -> Vec<TerminalId> {
        match self.group_index(project) {
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

    /// タブを同じプロジェクトの中で動かす(タブのドラッグ。ADR-019)。`to_index` は動かしたあとの位置(0 から。末尾より
    /// 大きければ末尾)。表示中のタブ・番号・表示名は変えない。位置が変わったら `true`。
    ///
    /// # Errors
    ///
    /// その ID のタブが無いとき(`TerminalError::NotFound`。何も変えない)。
    pub fn move_to(&mut self, id: &TerminalId, to_index: usize) -> Result<bool, TerminalError> {
        let (g, t) = self.locate(id)?;
        Ok(move_item(&mut self.groups[g].tabs, t, to_index))
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
    pub fn select_next(&mut self, project: &ProjectId) -> Option<TerminalId> {
        self.step(project, 1)
    }

    /// 前のタブを表示中にし、表示中の ID を返す。先頭の前は末尾。0 枚なら `None`。
    pub fn select_previous(&mut self, project: &ProjectId) -> Option<TerminalId> {
        self.step(project, -1)
    }

    /// そのタブにアテンションの印を立てる。立ったら `true`。既に立っている・その ID のタブが無いときは `false`。
    pub fn set_attention(&mut self, id: &TerminalId) -> bool {
        self.set_attention_to(id, true)
    }

    /// そのタブのアテンションの印を消す。消えたら `true`。立っていない・その ID のタブが無いときは `false`。
    pub fn clear_attention(&mut self, id: &TerminalId) -> bool {
        self.set_attention_to(id, false)
    }

    fn set_attention_to(&mut self, id: &TerminalId, value: bool) -> bool {
        let Ok((g, t)) = self.locate(id) else {
            return false;
        };
        let tab = &mut self.groups[g].tabs[t];
        let changed = tab.attention != value;
        tab.attention = value;
        changed
    }

    /// `project` のタブ(順のとおり)。タブを持ったことが無ければ空。
    #[must_use]
    pub fn tabs(&self, project: &ProjectId) -> &[TerminalTab] {
        self.group(project).map_or(&[], |g| g.tabs.as_slice())
    }

    /// `project` の表示中のタブ。0 枚なら `None`。
    #[must_use]
    pub fn active(&self, project: &ProjectId) -> Option<&TerminalId> {
        self.group(project).and_then(|g| g.active.as_ref())
    }

    /// その ID のタブ。
    #[must_use]
    pub fn tab(&self, id: &TerminalId) -> Option<&TerminalTab> {
        self.locate(id).ok().map(|(g, t)| &self.groups[g].tabs[t])
    }

    /// すべてのタブ(プロジェクトが初めてタブを持った順、その中はタブの順)。
    pub fn iter(&self) -> impl Iterator<Item = &TerminalTab> {
        self.groups.iter().flat_map(|g| g.tabs.iter())
    }

    fn step(&mut self, project: &ProjectId, delta: isize) -> Option<TerminalId> {
        let index = self.group_index(project)?;
        let group = &mut self.groups[index];
        let active = group.active.as_ref()?;
        let len = group.tabs.len();
        let current = group.tabs.iter().position(|t| &t.id == active)?;
        let next = (current + len).wrapping_add_signed(delta) % len;
        group.active = Some(group.tabs[next].id.clone());
        group.active.clone()
    }

    fn group_index(&self, project: &ProjectId) -> Option<usize> {
        self.groups.iter().position(|g| &g.project == project)
    }

    fn group(&self, project: &ProjectId) -> Option<&TabGroup> {
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
