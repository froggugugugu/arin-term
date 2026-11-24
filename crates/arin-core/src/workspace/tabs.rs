//! `Workspace` の並べ替えとタブをまとめて閉じる口(ADR-019)。
//!
//! - プロジェクトの並び(カードのドラッグ)。保存する並びと写しの並びがそのまま変わる。タイルの並びはプロジェクトの順に
//!   付いていくので、ページはフォーカス中のタイルのページに合わせ直す(`sync_display`)
//! - タブの並び(タブ表示のタブバーのドラッグ)。同じプロジェクトの同じ種類の中だけで動かす(ターミナルはターミナルの中、
//!   エディタはエディタの中。「同じ種類の中だけ」)。表示中のタブは変えない
//! - タブをまとめて閉じる(タブの右クリックのメニュー「タブを閉じる」「ほかのタブを閉じる」「これより右を閉じる」)。
//!   閉じるタブに動作中のターミナルか未保存のエディタがあり、確認していなければ何も変えずに確認を求める(「あれば確認を出す」)

use super::outcome::{CloseTabsRequest, CloseTabsResult, RunningTerminal, UnsavedEditor};
use super::state::Workspace;
use crate::editor::{EditorError, EditorId};
use crate::project::{Project, ProjectError, ProjectId};
use crate::terminal::{TerminalError, TerminalId};

impl Workspace {
    /// プロジェクトの並び順を変える。`to_index` は動かしたあとの位置(0 から。末尾より大きければ末尾)。
    /// 選択・タブは変えない。位置が変わったら `true`(このときだけ写しを送る。保存が予約される)。
    ///
    /// # Errors
    ///
    /// その ID のプロジェクトが無いとき(`ProjectError::NotFound`。何も変えない)。
    pub fn move_project(&mut self, id: &ProjectId, to_index: usize) -> Result<bool, ProjectError> {
        let moved = self.projects.move_to(id, to_index)?;
        if moved {
            self.sync_display();
        }
        Ok(moved)
    }

    /// ターミナルのタブを、同じプロジェクトのターミナルのタブの中で動かす。`to_index` はそのプロジェクトのターミナルの
    /// タブの並びで、動かしたあとの位置。表示中のタブは変えない。位置が変わったら `true`。
    ///
    /// # Errors
    ///
    /// その ID のタブが無いとき(`TerminalError::NotFound`。何も変えない)。
    pub fn move_terminal_tab(
        &mut self,
        id: &TerminalId,
        to_index: usize,
    ) -> Result<bool, TerminalError> {
        let moved = self.tabs.move_to(id, to_index)?;
        if moved {
            self.sync_display();
        }
        Ok(moved)
    }

    /// エディタのタブを、同じプロジェクトのエディタのタブの中で動かす。`to_index` はそのプロジェクトのエディタの並びで、
    /// 動かしたあとの位置。表示中のエディタは変えない。位置が変わったら `true`。
    ///
    /// # Errors
    ///
    /// その ID のエディタが無いとき(`EditorError::NotFound`。何も変えない)。
    pub fn move_editor_tab(&mut self, id: &EditorId, to_index: usize) -> Result<bool, EditorError> {
        self.editors.move_to(id, to_index)
    }

    /// `project` のタブのうち `request` の `terminals`・`editors` に挙げたものをまとめて閉じる。ほかのプロジェクトのタブと、
    /// もう無い ID は無視する。
    ///
    /// `running` は呼び出し側が調べた動作中のターミナル。閉じるエディタは、Rust の未保存の印か、画面が未保存と知っている
    /// もの(`request.dirty_editors`。印は画面から遅れて届くことがある)を未保存とみなす。閉じるタブの動作中・未保存の
    /// うち、確認済み(`request.confirmed_terminals`・`confirmed_editors`)に無いものが 1 つでもあれば、何も変えずに
    /// 確認を求める(一覧は確認済みのものも含めて全部)。確認のあとに動作中・未保存が増えていたら、もう一度確認を求める。
    /// そうでなければ閉じ、閉じたターミナルを返す(セッションの後片付けは呼び出し側。エディタの未保存の変更は捨てる)。
    /// 表示中のタブを閉じたときの次の表示は 1 枚ずつ閉じたときと同じ(右隣、無ければ左)。
    /// 閉じたあとに表示中のターミナルもエディタも無く、エディタが残っていれば、残った最初のエディタを出す
    /// (「ほかのタブを閉じる」でエディタを残したときに、何も出ないままにしない)。
    ///
    /// # Errors
    ///
    /// その ID のプロジェクトが無いとき(`ProjectError::NotFound`。何も変えない)。
    pub fn close_tabs(
        &mut self,
        project: &ProjectId,
        request: &CloseTabsRequest,
        running: &[TerminalId],
    ) -> Result<CloseTabsResult, ProjectError> {
        let Some(project_name) = self.projects.get(project).map(Project::name) else {
            return Err(ProjectError::NotFound(project.clone()));
        };
        let closing_terminals: Vec<TerminalId> = self
            .tabs
            .tabs(project)
            .iter()
            .filter(|tab| request.terminals.contains(&tab.id))
            .map(|tab| tab.id.clone())
            .collect();
        let closing_editors: Vec<EditorId> = self
            .editors
            .tabs(project)
            .filter(|tab| request.editors.contains(&tab.id))
            .map(|tab| tab.id.clone())
            .collect();
        let busy: Vec<RunningTerminal> = self
            .running_terminals(running)
            .into_iter()
            .filter(|t| closing_terminals.contains(&t.terminal_id))
            .collect();
        let unsaved: Vec<UnsavedEditor> = self
            .editors
            .tabs(project)
            .filter(|tab| {
                closing_editors.contains(&tab.id)
                    && (tab.dirty || request.dirty_editors.contains(&tab.id))
            })
            .map(|tab| UnsavedEditor {
                editor_id: tab.id.clone(),
                project_name: project_name.clone(),
                path: tab.path.clone(),
            })
            .collect();
        let unconfirmed = busy
            .iter()
            .any(|t| !request.confirmed_terminals.contains(&t.terminal_id))
            || unsaved
                .iter()
                .any(|e| !request.confirmed_editors.contains(&e.editor_id));
        if unconfirmed {
            return Ok(CloseTabsResult::NeedsConfirmation {
                running: busy,
                unsaved,
            });
        }
        let position = self.focused_tile_position();
        for id in &closing_terminals {
            // 並びから取った ID なので必ずある。
            let _ = self.tabs.close(id);
            self.terminals.remove(id);
        }
        for id in &closing_editors {
            let _ = self.editors.close(id);
        }
        if self.tabs.active(project).is_none() && self.editors.shown(project).is_none() {
            let first = self.editors.tabs(project).next().map(|tab| tab.id.clone());
            if let Some(first) = first {
                let _ = self.editors.show(&first);
            }
        }
        self.refocus_lost_tile(position);
        self.sync_display();
        Ok(CloseTabsResult::Closed {
            terminals: closing_terminals,
        })
    }
}
