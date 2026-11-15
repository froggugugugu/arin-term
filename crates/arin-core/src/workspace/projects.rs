//! `Workspace` のプロジェクトのコマンドの口(計画書 T33c): 登録・色の変更・再リンク・ルートのパス。
//!
//! 入出力はしない。フォルダの正規化・ブックマーク・ID の乱数・ダイアログ・ペーストボードは呼び出し側(`src-tauri`)が
//! ポート越しに済ませ、結果をここへ渡す。選択(`select_project`)と削除(`remove_project`)は `state.rs`。

use std::path::PathBuf;

use super::outcome::{AddProjectOutcome, WorkspaceError};
use super::state::Workspace;
use crate::project::{ProjectColor, ProjectError, ProjectId, RootReference};
use crate::terminal::TerminalStatus;

impl Workspace {
    /// フォルダをプロジェクトとして登録する(FR-001)。末尾に足して選び、ルートを cwd にしたタブを 1 枚開く
    /// (計画書 M2 の【仮定】6。シェルは画面が接続したときに起動する)。
    ///
    /// 同じフォルダ(正規化して比べる)が登録済みなら、増やさずタブも開かず既存を選び、
    /// `AddProjectOutcome::AlreadyRegistered` を返す(FR-010。失敗にしない)。
    ///
    /// # Errors
    ///
    /// パスが使えないとき・ゴミ箱の中のとき(`WorkspaceError::Project`)。同じ ID があるとき
    /// (`ProjectError::DuplicateId`。呼び出し側は乱数を取り直す)。どの場合も何も変えない。
    pub fn register_project(
        &mut self,
        id: ProjectId,
        root: RootReference,
    ) -> Result<AddProjectOutcome, WorkspaceError> {
        match self.projects.register(id, root) {
            Ok(project_id) => {
                // 登録したばかりの ID なので必ず開ける。
                let _ = self.add_tab(&project_id);
                Ok(AddProjectOutcome::Added { project_id })
            }
            Err(ProjectError::AlreadyRegistered(project_id)) => {
                // `ProjectStore::register` が既存を選んでいる。
                Ok(AddProjectOutcome::AlreadyRegistered { project_id })
            }
            Err(e) => Err(e.into()),
        }
    }

    /// 色を変える(FR-059)。
    ///
    /// # Errors
    ///
    /// その ID のプロジェクトが無いとき(`ProjectError::NotFound`)。
    pub fn set_project_color(
        &mut self,
        id: &ProjectId,
        color: ProjectColor,
    ) -> Result<(), ProjectError> {
        self.projects.set_color(id, color)
    }

    /// 本人が選んだフォルダにつなぎ直す(FR-007 の再リンク)。ID・色・並び順・表示名を保ち、見つからない状態を解く。
    /// タブは開かない(計画書 M2 の【仮定】6。0 枚から ⌘T)。まだシェルを起動していないタブは、つなぎ直した先で始める。
    ///
    /// # Errors
    ///
    /// その ID が無いとき、パスが使えないとき、ゴミ箱の中のとき、別のプロジェクトのフォルダと同じとき
    /// (`ProjectError::AlreadyRegistered`)。どの場合も何も変えない。
    pub fn relink_project(
        &mut self,
        id: &ProjectId,
        root: RootReference,
    ) -> Result<(), WorkspaceError> {
        self.projects.relink(id, root)?;
        let Some(path) = self.projects.get(id).map(|p| p.path().to_path_buf()) else {
            return Ok(());
        };
        let tabs: Vec<_> = self
            .tabs
            .tabs(Some(id))
            .iter()
            .map(|tab| tab.id.clone())
            .collect();
        for tab in tabs {
            if let Some(entry) = self.terminals.get_mut(&tab)
                && matches!(
                    entry.status,
                    TerminalStatus::NotStarted | TerminalStatus::Failed { .. }
                )
            {
                entry.cwd.clone_from(&path);
            }
        }
        Ok(())
    }

    /// ルートの絶対パス(最後に分かっていたもの。「絶対パスをコピー」に使う。FR-064)。選択は変えない。
    ///
    /// # Errors
    ///
    /// その ID のプロジェクトが無いとき(`ProjectError::NotFound`)。
    pub fn project_root_path(&self, id: &ProjectId) -> Result<PathBuf, ProjectError> {
        self.projects
            .get(id)
            .map(|p| p.path().to_path_buf())
            .ok_or_else(|| ProjectError::NotFound(id.clone()))
    }
}
