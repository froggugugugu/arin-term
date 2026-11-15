//! プロジェクトの一覧と選択(設計書 §6.1 の `ProjectStore`、§7.2「プロジェクトの登録」)。
//!
//! 入出力をしない。フォルダを調べる・ブックマークを作る・ID の乱数を得るのは呼び出し側(`Workspace` とコマンド)が
//! ポート越しに済ませ、結果をここへ渡す。削除はこの一覧から外すだけで、フォルダには触れない(FR-003)。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use super::path::{display_path, folder_name, is_in_trash, normalize_root};
use super::{ColorAssigner, ProjectColor, ProjectId};

/// プロジェクトの操作を受け付けられない理由。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProjectError {
    /// その ID のプロジェクトが無い。
    #[error("プロジェクト {0} が見つからない")]
    NotFound(ProjectId),
    /// 同じフォルダが既に登録されている(中身は既存のプロジェクト)。
    #[error("このフォルダはプロジェクト {0} として登録済み")]
    AlreadyRegistered(ProjectId),
    /// 同じ ID のプロジェクトが既にある(乱数が重なった)。
    #[error("プロジェクトの ID {0} が既に使われている")]
    DuplicateId(ProjectId),
    /// パスが UTF-8 でない。
    #[error("UTF-8 でないパスは登録できない")]
    NotUtf8Path,
    /// 相対パス。
    #[error("絶対パスでない: {0}")]
    RelativePath(String),
    /// `..` を含む(正規化していないパス)。
    #[error("`..` を含むパスは登録できない: {0}")]
    ParentComponent(String),
    /// ゴミ箱の中のフォルダ。
    #[error("ゴミ箱の中のフォルダは登録できない: {0}")]
    InTrash(String),
}

/// ルートフォルダの参照。ブックマーク(実体)と、最後に分かっていたパス(設計書 §6.2、FR-007)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RootReference {
    /// ブックマーク(base64)。`arin-core` は中身を見ない。無いときは `last_known_path` から作り直す。
    bookmark: Option<String>,
    /// 最後に分かっていた絶対パス。
    last_known_path: PathBuf,
}

impl RootReference {
    /// パスとブックマークから作る。パスの検査はストアに渡したときに行う。
    #[must_use]
    pub fn new(last_known_path: PathBuf, bookmark: Option<String>) -> Self {
        Self {
            bookmark,
            last_known_path,
        }
    }

    /// ブックマーク(base64)。
    #[must_use]
    pub fn bookmark(&self) -> Option<&str> {
        self.bookmark.as_deref()
    }

    /// 最後に分かっていた絶対パス。
    #[must_use]
    pub fn last_known_path(&self) -> &Path {
        &self.last_known_path
    }

    /// パスを正規化した参照にする。
    fn normalized(self) -> Result<Self, ProjectError> {
        Ok(Self {
            last_known_path: normalize_root(&self.last_known_path)?,
            bookmark: self.bookmark,
        })
    }
}

/// 登録したプロジェクト 1 件。JSON の形は `state.json` の `projects[]` の 1 件(ツリーの設定を除く)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    id: ProjectId,
    #[serde(flatten)]
    root: RootReference,
    /// 変更した表示名(FR-005。M6 で変更の操作を足す)。`None` のあいだはフォルダ名に追従する(§15.2 #5)。
    custom_name: Option<String>,
    #[serde(rename = "colorId")]
    color: ProjectColor,
    /// ルートが見つからない(表示値。保存しない)。
    #[serde(skip)]
    missing: bool,
}

impl Project {
    /// 保存した値から作る(`state.json` の読み込み用)。パスの検査は `ProjectStore::restore` が行う。
    #[must_use]
    pub fn restore(
        id: ProjectId,
        root: RootReference,
        custom_name: Option<String>,
        color: ProjectColor,
    ) -> Self {
        Self {
            id,
            root,
            custom_name,
            color,
            missing: false,
        }
    }

    /// 識別子。
    #[must_use]
    pub fn id(&self) -> &ProjectId {
        &self.id
    }

    /// ルートの参照。
    #[must_use]
    pub fn root(&self) -> &RootReference {
        &self.root
    }

    /// ルートの絶対パス(最後に分かっていたもの)。
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.root.last_known_path
    }

    /// 変更した表示名(保存値)。
    #[must_use]
    pub fn custom_name(&self) -> Option<&str> {
        self.custom_name.as_deref()
    }

    /// 表示名。変更した名前が空白だけでなければそれ、そうでなければいまのフォルダ名。
    #[must_use]
    pub fn name(&self) -> String {
        match self.custom_name.as_deref() {
            Some(name) if !name.trim().is_empty() => name.to_owned(),
            _ => folder_name(self.path()),
        }
    }

    /// 色。
    #[must_use]
    pub fn color(&self) -> ProjectColor {
        self.color
    }

    /// ルートが見つからない状態か(FR-007)。
    #[must_use]
    pub fn is_missing(&self) -> bool {
        self.missing
    }

    fn snapshot(&self, home: Option<&Path>) -> ProjectSnapshot {
        ProjectSnapshot {
            id: self.id.clone(),
            name: self.name(),
            root_path: self.path().to_string_lossy().into_owned(),
            display_path: display_path(self.path(), home),
            color: self.color,
            missing: self.missing,
        }
    }
}

/// 状態の写しに載せるプロジェクト 1 件(画面へ渡す)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSnapshot {
    /// 識別子。
    pub id: ProjectId,
    /// 表示名。
    pub name: String,
    /// ルートの絶対パス(「絶対パスをコピー」に使う。FR-064)。
    pub root_path: String,
    /// 表示用のパス(ホームの中は `~` で始める。ツールチップ用。FR-007)。
    pub display_path: String,
    /// 色の識別子。
    pub color: ProjectColor,
    /// ルートが見つからない。
    pub missing: bool,
}

/// 状態の写しに載せるプロジェクトの一覧。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectListSnapshot {
    /// 一覧(並び順のとおり)。
    pub projects: Vec<ProjectSnapshot>,
    /// 選択中のプロジェクト。
    pub selected_project_id: Option<ProjectId>,
}

/// プロジェクトの一覧(順序つき)と選択中のプロジェクト。
///
/// 守る約束: ID は重ならない。正規化したパスは重ならない(入れ子は許す)。選択中の ID は一覧にある。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProjectStore {
    projects: Vec<Project>,
    selected: Option<ProjectId>,
}

impl ProjectStore {
    /// 空の一覧。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 保存した一覧と選択から作る。パスを正規化し、正規化できないもの、先に出た ID・フォルダと重なるものは捨てる。
    /// 選択中の ID が残った一覧に無ければ選択は無し。
    #[must_use]
    pub fn restore(projects: Vec<Project>, selected: Option<ProjectId>) -> Self {
        let mut store = Self::new();
        for project in projects {
            let Ok(root) = project.root.normalized() else {
                continue;
            };
            if store.index_of(&project.id).is_some()
                || store.find_by_path(&root.last_known_path).is_some()
            {
                continue;
            }
            store.projects.push(Project {
                root,
                missing: false,
                ..project
            });
        }
        store.selected = selected.filter(|id| store.index_of(id).is_some());
        store
    }

    /// 一覧(並び順のとおり)。
    #[must_use]
    pub fn projects(&self) -> &[Project] {
        &self.projects
    }

    /// ID のプロジェクト。
    #[must_use]
    pub fn get(&self, id: &ProjectId) -> Option<&Project> {
        self.projects.iter().find(|project| &project.id == id)
    }

    /// 選択中のプロジェクト。
    #[must_use]
    pub fn selected(&self) -> Option<&ProjectId> {
        self.selected.as_ref()
    }

    /// そのフォルダ(正規化して比べる)を登録しているプロジェクト。入れ子の親や子は該当しない。
    #[must_use]
    pub fn find_by_path(&self, path: &Path) -> Option<&ProjectId> {
        let path = normalize_root(path).ok()?;
        self.projects
            .iter()
            .find(|project| project.path() == path)
            .map(|project| &project.id)
    }

    /// 登録する(FR-001)。末尾に足して選択し、色を割り当てる(FR-059)。
    ///
    /// `root` のパスは `RootLocatorPort::canonical_dir` が返したもの、`id` は `IdSource` の乱数から作ったものを渡す。
    ///
    /// # Errors
    ///
    /// - 同じフォルダが登録済みのとき(`AlreadyRegistered`)。**一覧は変えず、既存のプロジェクトを選択する**(FR-010)
    /// - パスが使えないとき(`NotUtf8Path`・`RelativePath`・`ParentComponent`)、ゴミ箱の中のとき(`InTrash`)
    /// - 同じ ID があるとき(`DuplicateId`)。呼び出し側は乱数を取り直す
    pub fn register(
        &mut self,
        id: ProjectId,
        root: RootReference,
    ) -> Result<ProjectId, ProjectError> {
        let root = self.accept_root(root, None)?;
        if self.index_of(&id).is_some() {
            return Err(ProjectError::DuplicateId(id));
        }
        let color = ColorAssigner::pick(self.projects.iter().map(Project::color));
        self.projects
            .push(Project::restore(id.clone(), root, None, color));
        self.selected = Some(id.clone());
        Ok(id)
    }

    /// 一覧から外す(FR-003)。フォルダには触れない。外したものが選択中なら、選択は次(無ければ前、0 件なら無し)に移る。
    ///
    /// 動作中のプロセスの確認とターミナルの後片付けは `Workspace` が先に行う。
    ///
    /// # Errors
    ///
    /// その ID が無いとき(`NotFound`)。
    pub fn remove(&mut self, id: &ProjectId) -> Result<Project, ProjectError> {
        let index = self.require(id)?;
        let removed = self.projects.remove(index);
        if self.selected.as_ref() == Some(id) {
            let next = index.min(self.projects.len().saturating_sub(1));
            self.selected = self.projects.get(next).map(|project| project.id.clone());
        }
        Ok(removed)
    }

    /// 選択する(FR-002)。
    ///
    /// # Errors
    ///
    /// その ID が無いとき(`NotFound`)。
    pub fn select(&mut self, id: &ProjectId) -> Result<(), ProjectError> {
        self.require(id)?;
        self.selected = Some(id.clone());
        Ok(())
    }

    /// 色を変える(FR-059)。
    ///
    /// # Errors
    ///
    /// その ID が無いとき(`NotFound`)。
    pub fn set_color(&mut self, id: &ProjectId, color: ProjectColor) -> Result<(), ProjectError> {
        let index = self.require(id)?;
        self.projects[index].color = color;
        Ok(())
    }

    /// 見回りやブックマークの解決で分かった、いまの位置に追従する(FR-007)。ID・色・並び順は変えない。
    /// 見つからない状態は解除する。ゴミ箱の中に移っていたら、ルートは変えずに見つからない状態にする(§15.2 #7)。
    ///
    /// # Errors
    ///
    /// その ID が無いとき(`NotFound`)、パスが使えないとき、別のプロジェクトのフォルダと同じとき(`AlreadyRegistered`。何も変えない)。
    pub fn update_root(&mut self, id: &ProjectId, root: RootReference) -> Result<(), ProjectError> {
        let index = self.require(id)?;
        match self.accept_root(root, Some(index)) {
            Ok(root) => {
                let project = &mut self.projects[index];
                project.root = root;
                project.missing = false;
                Ok(())
            }
            Err(ProjectError::InTrash(_)) => {
                self.projects[index].missing = true;
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    /// ルートが見つからない状態にする(FR-007。ブックマークも開いたフォルダも解決できなかったとき)。カードは残す(C-06)。
    ///
    /// # Errors
    ///
    /// その ID が無いとき(`NotFound`)。
    pub fn mark_missing(&mut self, id: &ProjectId) -> Result<(), ProjectError> {
        let index = self.require(id)?;
        self.projects[index].missing = true;
        Ok(())
    }

    /// 本人が選んだ別のフォルダにつなぎ直す(FR-007 の再リンク)。ID・色・並び順・表示名を保ち、見つからない状態を解除する。
    ///
    /// # Errors
    ///
    /// その ID が無いとき(`NotFound`)、パスが使えないとき、ゴミ箱の中のとき(`InTrash`)、
    /// 別のプロジェクトのフォルダと同じとき(`AlreadyRegistered`)。どの場合も何も変えない。
    pub fn relink(&mut self, id: &ProjectId, root: RootReference) -> Result<(), ProjectError> {
        let index = self.require(id)?;
        let root = self.accept_root(root, Some(index))?;
        let project = &mut self.projects[index];
        project.root = root;
        project.missing = false;
        Ok(())
    }

    /// 画面へ渡す写し。`home` はホームフォルダ(表示用のパスの `~`)。
    #[must_use]
    pub fn snapshot(&self, home: Option<&Path>) -> ProjectListSnapshot {
        ProjectListSnapshot {
            projects: self
                .projects
                .iter()
                .map(|project| project.snapshot(home))
                .collect(),
            selected_project_id: self.selected.clone(),
        }
    }

    fn index_of(&self, id: &ProjectId) -> Option<usize> {
        self.projects.iter().position(|project| &project.id == id)
    }

    fn require(&self, id: &ProjectId) -> Result<usize, ProjectError> {
        self.index_of(id)
            .ok_or_else(|| ProjectError::NotFound(id.clone()))
    }

    /// ルートの参照を正規化し、使えるか確かめる。`own` の位置のプロジェクト自身とは重なってよい。
    /// 登録(`own` が無い)で重なったときは、既存のプロジェクトを選択する(FR-010)。
    fn accept_root(
        &mut self,
        root: RootReference,
        own: Option<usize>,
    ) -> Result<RootReference, ProjectError> {
        let root = root.normalized()?;
        if is_in_trash(&root.last_known_path) {
            return Err(ProjectError::InTrash(
                root.last_known_path.to_string_lossy().into_owned(),
            ));
        }
        let clash =
            self.projects.iter().enumerate().find(|(index, project)| {
                Some(*index) != own && project.path() == root.last_known_path
            });
        if let Some((_, existing)) = clash {
            let existing = existing.id.clone();
            if own.is_none() {
                self.selected = Some(existing.clone());
            }
            return Err(ProjectError::AlreadyRegistered(existing));
        }
        Ok(root)
    }
}
