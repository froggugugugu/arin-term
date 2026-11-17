//! `Workspace` のエディタのタブ(ADR-015 の 3・4、計画書 T81b。【要確認】1・2 は推奨の A)。
//!
//! - エディタのタブは `EditorStore` が持ち、プロジェクトごとに表示中のエディタ(`shown`)を 0 か 1 つ持つ
//! - **タブ表示**で表示中のエディタがあれば、表示中のターミナルの代わりにそれを出す(`shown_editor`・`active_tab`)。
//!   タイル表示ではエディタを出さない(タイルはターミナルだけ。ADR-011 の規則を変えない)
//! - ターミナルのタブを選ぶ・作る・タイルにフォーカスすると、そのプロジェクトの表示中のエディタは隠れる(閉じない)
//! - タイル表示でエディタを開く・選ぶと、タブ表示に替える(【要確認】1 の A。表示モードの保存値も替わる)
//! - 前後のタブは、タブ表示ではターミナル → エディタの順で 1 周する。タイル表示ではターミナルだけ(今までどおり)
//! - 表示中がエディタのとき、そのプロジェクトのターミナルは「見ている」に数えない(`display.rs` の `attended_terminals`)
//! - 保存しない(再起動で開き直さない。【要確認】2 の A)
//!
//! 文書を読む・保存するのは呼び出し側(`src-tauri` が `DocumentPort` で行う)。ここは版と未保存の印だけを持つ。

use super::outcome::{CloseEditorOutcome, UnsavedEditor, WorkspaceError};
use super::state::Workspace;
use crate::editor::{
    EditorError, EditorId, EditorTab, FileStamp, ReadOnlyReason, is_markdown_path,
};
use crate::layout::tiles::DisplayMode;
use crate::paths::RelPath;
use crate::project::{Project, ProjectError, ProjectId};
use crate::terminal::TerminalId;

/// 選択中のプロジェクトでいま表示しているタブ(⌘W で閉じるもの・前後のタブの結果)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActiveTab {
    /// ターミナル。
    Terminal(TerminalId),
    /// エディタ(タブ表示のときだけ)。
    Editor(EditorId),
}

impl Workspace {
    /// `project` の `path` をエディタで開いて表示する(ADR-015 の 3)。新しく作ったら `(ID, true)`。
    /// 既に開いていれば作らずにそれを表示し `(ID, false)`(`stamp`・`read_only` は使わない)。
    /// そのプロジェクトを選び、タイル表示ならタブ表示に替える(拡大は解ける)。
    ///
    /// `stamp` と `read_only` は呼び出し側が `DocumentPort::read` で得たもの。
    ///
    /// # Errors
    ///
    /// プロジェクトが無いとき(`WorkspaceError::Project`)、ルートが見つからないとき(`WorkspaceError::RootMissing`)、
    /// 拡張子が `.md`・`.markdown` でないとき(`WorkspaceError::NotMarkdown`)。どの場合も何も変えない。
    pub fn open_editor(
        &mut self,
        project: &ProjectId,
        path: RelPath,
        stamp: FileStamp,
        read_only: Option<ReadOnlyReason>,
    ) -> Result<(EditorId, bool), WorkspaceError> {
        let found = self
            .projects
            .get(project)
            .ok_or_else(|| ProjectError::NotFound(project.clone()))?;
        if found.is_missing() {
            return Err(WorkspaceError::RootMissing(project.clone()));
        }
        if !is_markdown_path(&path) {
            return Err(WorkspaceError::NotMarkdown(path));
        }
        let opened = self.editors.open(project, path, stamp, read_only);
        self.show_editor_of(project);
        Ok(opened)
    }

    /// エディタのタブを選ぶ(タブバーのクリック)。持ち主のプロジェクトを選び、タイル表示ならタブ表示に替える。
    ///
    /// # Errors
    ///
    /// その ID のエディタが無いとき(`EditorError::NotFound`)。何も変えない。
    pub fn select_editor(&mut self, id: &EditorId) -> Result<(), EditorError> {
        self.editors.show(id)?;
        let project = self.editor_project(id)?;
        self.show_editor_of(&project);
        Ok(())
    }

    /// エディタを閉じる(ADR-015 の 4)。未保存の変更があり `discard` が偽なら、何も変えずに確認を求める。
    /// 表示中だったなら右隣(無ければ左隣)のエディタを、どちらも無ければ表示中のターミナルを出す。
    ///
    /// # Errors
    ///
    /// その ID のエディタが無いとき(`EditorError::NotFound`)。
    pub fn close_editor(
        &mut self,
        id: &EditorId,
        discard: bool,
    ) -> Result<CloseEditorOutcome, EditorError> {
        let tab = self
            .editor(id)
            .ok_or_else(|| EditorError::NotFound(id.clone()))?;
        if tab.dirty && !discard {
            return Ok(CloseEditorOutcome::NeedsConfirmation);
        }
        self.editors.close(id)?;
        self.sync_display();
        Ok(CloseEditorOutcome::Closed)
    }

    /// 未保存の印を替える(画面が変わり目だけ送る)。変わったら `Ok(true)`(このときだけ写しを送る)。
    ///
    /// # Errors
    ///
    /// その ID のエディタが無いとき(`EditorError::NotFound`)。
    pub fn set_editor_dirty(&mut self, id: &EditorId, dirty: bool) -> Result<bool, EditorError> {
        self.editors.set_dirty(id, dirty)
    }

    /// エディタの版(開いた・保存した・読み直したときのもの)。
    ///
    /// # Errors
    ///
    /// その ID のエディタが無いとき(`EditorError::NotFound`)。
    pub fn editor_stamp(&self, id: &EditorId) -> Result<FileStamp, EditorError> {
        self.editor(id)
            .map(|tab| tab.stamp)
            .ok_or_else(|| EditorError::NotFound(id.clone()))
    }

    /// エディタの版を替える(保存した・読み直した・`touch` だけだった)。
    ///
    /// # Errors
    ///
    /// その ID のエディタが無いとき(`EditorError::NotFound`)。
    pub fn set_editor_stamp(&mut self, id: &EditorId, stamp: FileStamp) -> Result<(), EditorError> {
        self.editors.set_stamp(id, stamp)
    }

    /// その ID のエディタ(持ち主のプロジェクト・相対パス・読み取り専用・版。読み書きの前に呼び出し側が引く)。
    #[must_use]
    pub fn editor(&self, id: &EditorId) -> Option<&EditorTab> {
        self.editors.tab(id)
    }

    /// 選択中のプロジェクトでいま出しているエディタ。タブ表示で、そのプロジェクトの表示中のエディタがあるときだけ
    /// `Some`(タイル表示・ターミナルを表示中のときは `None`)。
    #[must_use]
    pub fn shown_editor(&self) -> Option<EditorId> {
        if self.display.mode != DisplayMode::Tabs {
            return None;
        }
        let project = self.projects.selected()?;
        self.editors.shown(project).cloned()
    }

    /// 選択中のプロジェクトでいま表示しているタブ(⌘W の対象)。出しているエディタがあればそれ、
    /// 無ければ表示中のターミナル(タイル表示ではフォーカス中のタイル)。どちらも無ければ `None`。
    #[must_use]
    pub fn active_tab(&self) -> Option<ActiveTab> {
        self.shown_editor()
            .map(ActiveTab::Editor)
            .or_else(|| self.active_terminal().map(ActiveTab::Terminal))
    }

    /// 選択中のプロジェクトの次のタブを表示し、表示したタブを返す。タブ表示ではターミナル → エディタの順で、
    /// 末尾の次は先頭。タイル表示ではターミナルだけを回る。選んでいない・タブが無ければ `None`。
    pub fn select_next_tab(&mut self) -> Option<ActiveTab> {
        self.step_tab(1)
    }

    /// 選択中のプロジェクトの前のタブを表示し、表示したタブを返す(`select_next_tab` の逆順)。
    pub fn select_previous_tab(&mut self) -> Option<ActiveTab> {
        self.step_tab(-1)
    }

    /// 未保存のエディタ(終了の確認の材料。ADR-015 の 4)。プロジェクトの並び順、その中は開いた順。
    #[must_use]
    pub fn unsaved_editors(&self) -> Vec<UnsavedEditor> {
        self.unsaved_editors_in(None)
    }

    /// 未保存のエディタ。`project` を渡せばそのプロジェクトのものだけ。
    pub(super) fn unsaved_editors_in(&self, project: Option<&ProjectId>) -> Vec<UnsavedEditor> {
        self.ordered_editors()
            .filter(|tab| tab.dirty && project.is_none_or(|p| &tab.project == p))
            .map(|tab| UnsavedEditor {
                editor_id: tab.id.clone(),
                project_name: self
                    .projects
                    .get(&tab.project)
                    .map(Project::name)
                    .unwrap_or_default(),
                path: tab.path.clone(),
            })
            .collect()
    }

    /// すべてのエディタ。プロジェクトの並び順、その中は開いた順。
    pub(super) fn ordered_editors(&self) -> impl Iterator<Item = &EditorTab> {
        self.projects
            .projects()
            .iter()
            .flat_map(|p| self.editors.tabs(p.id()))
    }

    /// `project` のターミナルのタブを選んだ・作った・タイルにフォーカスした。表示中のエディタを隠す(閉じない)。
    pub(super) fn hide_editor_of(&mut self, project: &ProjectId) {
        self.editors.hide(project);
    }

    /// `project` の表示中のエディタを出す: そのプロジェクトを選び、タブ表示にして、表示を合わせ直す。
    fn show_editor_of(&mut self, project: &ProjectId) {
        // 呼ぶ側がプロジェクトがあることを確かめている。
        let _ = self.projects.select(project);
        // タブ表示にすると拡大は解ける。既にタブ表示なら表示を合わせ直すだけ。
        if !self.set_display_mode(DisplayMode::Tabs) {
            self.sync_display();
        }
    }

    fn editor_project(&self, id: &EditorId) -> Result<ProjectId, EditorError> {
        self.editor(id)
            .map(|tab| tab.project.clone())
            .ok_or_else(|| EditorError::NotFound(id.clone()))
    }

    fn step_tab(&mut self, delta: isize) -> Option<ActiveTab> {
        let project = self.projects.selected().cloned()?;
        let mut order: Vec<ActiveTab> = self
            .tabs
            .tabs(&project)
            .iter()
            .map(|tab| ActiveTab::Terminal(tab.id.clone()))
            .collect();
        if self.display.mode == DisplayMode::Tabs {
            order.extend(
                self.editors
                    .tabs(&project)
                    .map(|tab| ActiveTab::Editor(tab.id.clone())),
            );
        }
        if order.is_empty() {
            return None;
        }
        let len = order.len();
        let next = match self
            .active_tab()
            .and_then(|current| order.iter().position(|t| *t == current))
        {
            Some(current) => (current + len).wrapping_add_signed(delta) % len,
            None if delta < 0 => len - 1,
            None => 0,
        };
        let target = order.swap_remove(next);
        match &target {
            ActiveTab::Terminal(id) => {
                // 並びから取った ID なので必ずある。
                let _ = self.tabs.activate(id);
                self.hide_editor_of(&project);
            }
            ActiveTab::Editor(id) => {
                // 並びから取った ID なので必ずある。
                let _ = self.editors.show(id);
            }
        }
        self.sync_display();
        Some(target)
    }
}
