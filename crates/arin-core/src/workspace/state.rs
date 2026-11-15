//! アプリの状態の正本(設計書 §5.1、§6.3)。プロジェクトをまたぐ操作と、写し・保存する内容の生成。
//!
//! 入出力はしない。PTY の起動・読み書きは `src-tauri/src/sessions.rs` が、ここの判定に従って行う。
//! 呼び出し側は Mutex を取ってここを読み書きし、PTY の操作のあいだは Mutex を持たない(設計書 §7.1「スレッドの分担」)。
//! ターミナルの状態と規則(接続・入力・タブ)は `terminals.rs`。

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use super::display::DisplayState;
use super::outcome::{RemoveOutcome, RootResolution, RunningTerminal, Startup};
use super::snapshot::{
    AppSnapshot, ProjectCardSnapshot, SNAPSHOT_SCHEMA_VERSION, TerminalTabSnapshot,
};
use crate::launch_options::VERIFY_GRID_TERMINAL_COUNT;
use crate::layout::panes::PaneSizes;
use crate::persistence::codec::{PersistedLayout, PersistedState};
use crate::project::{Project, ProjectError, ProjectId, ProjectStore};
use crate::terminal::{ExitPolicy, TerminalId, TerminalStatus, TerminalStore, TerminalTab};

/// ターミナル 1 枚の、タブの並び以外の状態。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TerminalEntry {
    /// 起動するときの cwd(プロジェクトのルート。検証画面ではホーム)。
    pub(super) cwd: PathBuf,
    pub(super) status: TerminalStatus,
}

/// アプリの状態の正本。
#[derive(Debug, Clone, PartialEq)]
pub struct Workspace {
    pub(super) app_version: String,
    /// ホームフォルダ(表示用のパスの `~`、検証画面の cwd)。分からなければ空のパス。
    pub(super) home: PathBuf,
    pub(super) parent_env: Vec<(OsString, OsString)>,
    pub(super) projects: ProjectStore,
    pub(super) tabs: TerminalStore,
    pub(super) terminals: HashMap<TerminalId, TerminalEntry>,
    pub(super) layout: PaneSizes,
    /// 表示モード・タイルのページ・拡大(`display.rs`)。起動のたびにタブ表示で始まる(保存は T46c)。
    pub(super) display: DisplayState,
    pub(super) notice: Option<String>,
    /// 状態ファイルに保存するか(検証画面では保存しない)。
    pub(super) persist: bool,
}

impl Workspace {
    /// 起動時の状態を作る(設計書 §7.1「起動」、FR-004・FR-039・C-22)。
    ///
    /// 読んだ一覧と選択を復元し、ルートを解決できたプロジェクトにだけタブを 1 枚ずつ、まだ起動していない状態で登録する
    /// (PTY は最初の接続で起動する。cwd はルート)。解決できなかったもの・`startup.roots` に無いもの・ゴミ箱の中や
    /// 別のプロジェクトと同じフォルダに解決されたものは「見つからない」にしてタブを作らない(保存値は読んだまま)。
    /// ペインの寸法は保存値に規則(`PaneSizes::sanitized`)を当て、無い項目は初期値にする。
    ///
    /// - `app_version`: アプリの版
    /// - `home`: ホームフォルダ。分からなければ空のパスを渡す
    /// - `parent_env`: アプリが受け取った環境変数。シェルへは許可リストの名前だけが渡る(`ShellLaunchSpec`)
    pub fn restore<I, K, V>(
        app_version: impl Into<String>,
        home: PathBuf,
        parent_env: I,
        startup: Startup,
    ) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        let Startup {
            state,
            roots,
            notice,
        } = startup;
        let mut ws = Self::empty(app_version, home, parent_env, ExitPolicy::CloseTab, true);
        ws.projects = ProjectStore::restore(state.projects, state.selected_project_id);
        ws.layout = pane_sizes(&state.layout);
        ws.notice = notice;

        let ids: Vec<ProjectId> = ws
            .projects
            .projects()
            .iter()
            .map(|project| project.id().clone())
            .collect();
        for id in ids {
            let resolved =
                roots
                    .iter()
                    .find(|(project, _)| project == &id)
                    .and_then(|(_, resolution)| match resolution {
                        RootResolution::Found(root) => Some(root.clone()),
                        RootResolution::Missing => None,
                    });
            let usable = resolved.is_some_and(|root| {
                ws.projects.update_root(&id, root).is_ok()
                    && ws.projects.get(&id).is_some_and(|p| !p.is_missing())
            });
            if usable {
                let _ = ws.add_tab(&id);
            } else {
                // 一覧から読んだばかりの ID なので失敗しない。
                let _ = ws.projects.mark_missing(&id);
            }
        }
        ws
    }

    /// 9 枚の検証画面(`ARIN_VERIFY_GRID`。M1 の検証用、M4 の T49 で外す)の状態を作る。
    ///
    /// プロジェクトを持たないターミナルを `terminal-1`〜`terminal-9` の 9 枚、ホームフォルダを cwd にして登録する。
    /// シェルが終わってもタブを閉じない(`ExitPolicy::KeepTab`)。状態ファイルには保存しない(`persisted_state` が `None`)。
    pub fn for_verify_grid<I, K, V>(
        app_version: impl Into<String>,
        home: PathBuf,
        parent_env: I,
    ) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        let mut ws = Self::empty(app_version, home, parent_env, ExitPolicy::KeepTab, false);
        for _ in 0..VERIFY_GRID_TERMINAL_COUNT {
            let id = ws.tabs.add(None);
            let cwd = ws.home.clone();
            ws.terminals.insert(
                id,
                TerminalEntry {
                    cwd,
                    status: TerminalStatus::NotStarted,
                },
            );
        }
        ws
    }

    fn empty<I, K, V>(
        app_version: impl Into<String>,
        home: PathBuf,
        parent_env: I,
        exit_policy: ExitPolicy,
        persist: bool,
    ) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        Self {
            app_version: app_version.into(),
            home,
            parent_env: parent_env
                .into_iter()
                .map(|(k, v)| (k.as_ref().to_owned(), v.as_ref().to_owned()))
                .collect(),
            projects: ProjectStore::new(),
            tabs: TerminalStore::new(exit_policy),
            terminals: HashMap::new(),
            layout: PaneSizes::default(),
            display: DisplayState::default(),
            notice: None,
            persist,
        }
    }

    /// プロジェクトを選択する(FR-002)。ターミナルとセッションには触れない(FR-042)。
    ///
    /// タイル表示では、そのプロジェクトの最初のタイルのページへ移る(FR-053。表示中のタブがそのページに無ければ
    /// 最初のタイルを表示中にする。タブが 0 枚ならページは変えない。【仮定】5)。
    ///
    /// # Errors
    ///
    /// その ID のプロジェクトが無いとき(`ProjectError::NotFound`)。
    pub fn select_project(&mut self, id: &ProjectId) -> Result<(), ProjectError> {
        self.projects.select(id)?;
        self.align_selected_project_to_tiles();
        self.sync_display();
        Ok(())
    }

    /// プロジェクトを一覧から外す(FR-003。フォルダには触れない)。
    ///
    /// `running` は呼び出し側が調べた動作中のターミナル(ほかのプロジェクトのものが混ざってよい)。
    /// そのプロジェクトに動作中のものがあり `confirmed` が偽なら、何も変えずに確認を求める。
    /// そうでなければタブをすべて外してから一覧から外し、外したタブを返す(セッションの後片付けは呼び出し側)。
    /// 選択中だったなら選択は次(無ければ前、0 件なら無し)に移る。
    ///
    /// # Errors
    ///
    /// その ID のプロジェクトが無いとき(`ProjectError::NotFound`)。
    pub fn remove_project(
        &mut self,
        id: &ProjectId,
        running: &[TerminalId],
        confirmed: bool,
    ) -> Result<RemoveOutcome, ProjectError> {
        if self.projects.get(id).is_none() {
            return Err(ProjectError::NotFound(id.clone()));
        }
        let busy: Vec<RunningTerminal> = self
            .running_terminals(running)
            .into_iter()
            .filter(|t| t.project_id.as_ref() == Some(id))
            .collect();
        if !busy.is_empty() && !confirmed {
            return Ok(RemoveOutcome::NeedsConfirmation(busy));
        }
        let terminals = self.tabs.remove_project(id);
        for terminal in &terminals {
            self.terminals.remove(terminal);
        }
        self.projects.remove(id)?;
        self.sync_display();
        Ok(RemoveOutcome::Removed { terminals })
    }

    /// 動作中のターミナル(`running`。呼び出し側が調べたもの)を、プロジェクト名とタブ名つきで返す(FR-063 の確認の材料)。
    /// 順はプロジェクトの並び順・タブの順で、持ち主の無いもの(検証画面)は最後。いま無い ID は無視する。
    #[must_use]
    pub fn running_terminals(&self, running: &[TerminalId]) -> Vec<RunningTerminal> {
        self.ordered_tabs()
            .filter(|tab| running.contains(&tab.id))
            .map(|tab| RunningTerminal {
                terminal_id: tab.id.clone(),
                project_id: tab.project.clone(),
                project_name: tab
                    .project
                    .as_ref()
                    .and_then(|id| self.projects.get(id))
                    .map(Project::name),
                label: tab.label.clone(),
            })
            .collect()
    }

    /// 状態ファイルに保存する内容(FR-056)。表示値(見つからない・表示用のパス・知らせ・タブ)を含めない。
    /// 検証画面では保存しないので `None`。
    #[must_use]
    pub fn persisted_state(&self) -> Option<PersistedState> {
        if !self.persist {
            return None;
        }
        Some(PersistedState {
            projects: self
                .projects
                .projects()
                .iter()
                .map(|p| {
                    Project::restore(
                        p.id().clone(),
                        p.root().clone(),
                        p.custom_name().map(ToOwned::to_owned),
                        p.color(),
                    )
                    .with_show_hidden(p.show_hidden())
                })
                .collect(),
            selected_project_id: self.projects.selected().cloned(),
            layout: PersistedLayout {
                sidebar_width: Some(self.layout.sidebar_width),
                sidebar_collapsed: self.layout.sidebar_collapsed,
                card_area_height: Some(self.layout.card_area_height),
            },
        })
    }

    /// 画面へ渡す状態の写し。
    #[must_use]
    pub fn snapshot(&self) -> AppSnapshot {
        let list = self.projects.snapshot(self.home());
        let projects = list
            .projects
            .into_iter()
            .map(|p| {
                let terminal_ids = self
                    .tabs
                    .tabs(Some(&p.id))
                    .iter()
                    .map(|tab| tab.id.clone())
                    .collect();
                let active_terminal_id = self.tabs.active(Some(&p.id)).cloned();
                let show_hidden = self.projects.get(&p.id).is_some_and(Project::show_hidden);
                ProjectCardSnapshot {
                    id: p.id,
                    name: p.name,
                    color: p.color,
                    display_path: p.display_path,
                    missing: p.missing,
                    show_hidden,
                    terminal_ids,
                    active_terminal_id,
                }
            })
            .collect();
        let terminals = self
            .ordered_tabs()
            .map(|tab| TerminalTabSnapshot {
                id: tab.id.clone(),
                project_id: tab.project.clone(),
                label: tab.label.clone(),
                status: self
                    .terminals
                    .get(&tab.id)
                    .map_or(TerminalStatus::NotStarted, |entry| entry.status.clone()),
            })
            .collect();
        AppSnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            app_version: self.app_version.clone(),
            projects,
            selected_project_id: list.selected_project_id,
            terminals,
            layout: self.layout,
            notice: self.notice.clone(),
            display: self.display_snapshot(),
        }
    }

    /// プロジェクトの末尾にタブを足し、ルートを cwd にして登録する。プロジェクトが無ければ `None`。
    pub(super) fn add_tab(&mut self, project: &ProjectId) -> Option<TerminalId> {
        let cwd = self.projects.get(project)?.path().to_path_buf();
        let id = self.tabs.add(Some(project));
        self.terminals.insert(
            id.clone(),
            TerminalEntry {
                cwd,
                status: TerminalStatus::NotStarted,
            },
        );
        self.sync_display();
        Some(id)
    }

    /// すべてのタブ。プロジェクトの並び順、その中はタブの順。持ち主の無いもの(検証画面)は最後。
    pub(super) fn ordered_tabs(&self) -> impl Iterator<Item = &TerminalTab> {
        self.projects
            .projects()
            .iter()
            .flat_map(|p| self.tabs.tabs(Some(p.id())))
            .chain(self.tabs.tabs(None))
    }

    /// 表示用のパスに使うホーム。分からなければ `None`。
    fn home(&self) -> Option<&Path> {
        (!self.home.as_os_str().is_empty()).then_some(self.home.as_path())
    }
}

/// 保存値のペインの寸法に規則を当てる。無い項目は初期値。
fn pane_sizes(layout: &PersistedLayout) -> PaneSizes {
    let defaults = PaneSizes::default();
    PaneSizes {
        sidebar_width: layout.sidebar_width.unwrap_or(defaults.sidebar_width),
        sidebar_collapsed: layout.sidebar_collapsed,
        card_area_height: layout.card_area_height.unwrap_or(defaults.card_area_height),
    }
    .sanitized()
}
