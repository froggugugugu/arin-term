//! `Workspace` のプロジェクトのコマンドの口(計画書 T33c): 登録・色の変更・再リンク・ルートのパス・隠しファイル表示(T38e)。
//! と、ルートへの追従の見回りの口(計画書 T33d): 対象の一覧・追従・見つからない。
//!
//! 入出力はしない。フォルダの正規化・ブックマーク・ID の乱数・ダイアログ・ペーストボード・開いたフォルダの現在位置は
//! 呼び出し側(`src-tauri`)がポート越しに済ませ、結果をここへ渡す。選択(`select_project`)と削除(`remove_project`)は `state.rs`。

use std::path::{Path, PathBuf};

use super::outcome::{AddProjectOutcome, ProjectRoot, WorkspaceError};
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
                // `ProjectStore::register` が既存を選んでいる。選択が変わるので表示を合わせ直す
                // (見えたタブのアテンションの印を消す。計画書 T48b)。
                self.sync_display();
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

    /// 隠しファイル表示を切り替える(FR-017、計画書 T38e)。写しの `showHidden` と保存する内容が変わる
    /// (写しを送り保存を予約するのは呼び出し側)。見つからないプロジェクトでも変えられる。
    ///
    /// # Errors
    ///
    /// その ID のプロジェクトが無いとき(`ProjectError::NotFound`。何も変えない)。
    pub fn set_project_show_hidden(
        &mut self,
        id: &ProjectId,
        show_hidden: bool,
    ) -> Result<(), ProjectError> {
        self.projects.set_show_hidden(id, show_hidden)
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
        self.retarget_unstarted_tabs(id);
        Ok(())
    }

    /// 見回りの対象(全プロジェクトのルートの参照と、見つからないかどうか。並び順のとおり。計画書 T33d)。
    #[must_use]
    pub fn project_roots(&self) -> Vec<ProjectRoot> {
        self.projects
            .projects()
            .iter()
            .map(|p| ProjectRoot {
                id: p.id().clone(),
                root: p.root().clone(),
                missing: p.is_missing(),
            })
            .collect()
    }

    /// 見回りやブックマークの解決で分かったいまの位置に追従する(FR-007。本人の再リンクは `relink_project`)。
    /// ID・色・並び順・タブを保ち、見つからない状態を解く。ゴミ箱の中なら、ルートは変えずに見つからないにする。
    /// タブは開かない。まだシェルを起動していないタブは、新しいルートで始める。
    ///
    /// `expected` は見回りが調べたときのルートのパス。いまのパスと違えば(調べているあいだに再リンクされた)何もしない。
    /// 何かが変わったら `true`(呼び出し側が写しを送る)。
    ///
    /// # Errors
    ///
    /// その ID が無いとき、パスが使えないとき、別のプロジェクトのフォルダと同じとき
    /// (`ProjectError::AlreadyRegistered`)。どの場合も何も変えない。
    pub fn follow_project_root(
        &mut self,
        id: &ProjectId,
        expected: &Path,
        root: RootReference,
    ) -> Result<bool, WorkspaceError> {
        let project = self
            .projects
            .get(id)
            .ok_or_else(|| ProjectError::NotFound(id.clone()))?;
        if project.path() != expected {
            return Ok(false);
        }
        let before = (project.root().clone(), project.is_missing());
        self.projects.update_root(id, root)?;
        let Some(project) = self.projects.get(id) else {
            return Ok(false);
        };
        let moved = project.path() != before.0.last_known_path();
        let changed = (project.root().clone(), project.is_missing()) != before;
        if moved {
            self.retarget_unstarted_tabs(id);
        }
        Ok(changed)
    }

    /// 見回りでルートが取れなくなったとき、見つからない状態にする(FR-007。カードとタブは残す。C-06)。
    /// `expected` は `follow_project_root` と同じ。既に見つからない・いまのパスと違うときは何もせず `false`。
    ///
    /// # Errors
    ///
    /// その ID のプロジェクトが無いとき(`ProjectError::NotFound`)。
    pub fn mark_project_missing(
        &mut self,
        id: &ProjectId,
        expected: &Path,
    ) -> Result<bool, ProjectError> {
        let project = self
            .projects
            .get(id)
            .ok_or_else(|| ProjectError::NotFound(id.clone()))?;
        if project.path() != expected || project.is_missing() {
            return Ok(false);
        }
        self.projects.mark_missing(id)?;
        Ok(true)
    }

    /// まだシェルを起動していないタブ(起動に失敗したものを含む)の cwd を、いまのルートにする。
    fn retarget_unstarted_tabs(&mut self, id: &ProjectId) {
        let Some(path) = self.projects.get(id).map(|p| p.path().to_path_buf()) else {
            return;
        };
        let tabs: Vec<_> = self
            .tabs
            .tabs(id)
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
