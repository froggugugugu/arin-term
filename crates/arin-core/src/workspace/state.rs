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
    AppSnapshot, EditorSettingsSnapshot, EditorTabSnapshot, ProjectCardSnapshot,
    SNAPSHOT_SCHEMA_VERSION, TerminalTabSnapshot,
};
use crate::editor::EditorStore;
use crate::layout::font_sizes::{FontSizeTarget, FontSizes};
use crate::layout::panes::PaneSizes;
use crate::layout::theme::ThemeMode;
use crate::layout::translucency::{Translucency, TranslucencyTarget};
use crate::persistence::codec::{PersistedLayout, PersistedState};
use crate::project::{Project, ProjectError, ProjectId, ProjectStore};
use crate::terminal::{TerminalId, TerminalStatus, TerminalStore, TerminalTab};

/// ターミナル 1 枚の、タブの並び以外の状態。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TerminalEntry {
    /// 起動するときの cwd(プロジェクトのルート)。
    pub(super) cwd: PathBuf,
    pub(super) status: TerminalStatus,
}

/// アプリの状態の正本。
#[derive(Debug, Clone, PartialEq)]
pub struct Workspace {
    pub(super) app_version: String,
    /// ホームフォルダ(表示用のパスの `~`)。分からなければ空のパス。
    pub(super) home: PathBuf,
    pub(super) parent_env: Vec<(OsString, OsString)>,
    pub(super) projects: ProjectStore,
    pub(super) tabs: TerminalStore,
    /// エディタのタブとプロジェクトごとの表示中のエディタ(`editors.rs`。ADR-015 の 3)。保存しない。
    pub(super) editors: EditorStore,
    pub(super) terminals: HashMap<TerminalId, TerminalEntry>,
    pub(super) layout: PaneSizes,
    /// 表示モード・タイルのページ・拡大(`display.rs`)。表示モードだけを保存し、ページと拡大中は起動のたびに初めから。
    pub(super) display: DisplayState,
    /// ターミナルと画面の文字の大きさ(`appearance.rs`。計画書 T68b、ADR-013 の 2)。
    pub(super) font_sizes: FontSizes,
    /// ウィンドウとターミナルの背景の透過(`appearance.rs`。ADR-014 の 1)。
    pub(super) translucency: Translucency,
    /// エディタの Vim のキー操作(`state.json` の `ui.editorVim`。ADR-015 の 6)。「既定に戻す」では変えない。
    pub(super) editor_vim: bool,
    /// テーマの選び方(`state.json` の `ui.theme`。ADR-016)。「既定に戻す」では変えない。
    pub(super) theme: ThemeMode,
    /// 起動時に新しい版を確かめるか(`state.json` の `ui.checkUpdates`。ADR-018)。既定は偽。
    pub(super) check_updates: bool,
    pub(super) notice: Option<String>,
}

impl Workspace {
    /// 起動時の状態を作る(設計書 §7.1「起動」、FR-004・FR-039・C-22)。
    ///
    /// 読んだ一覧と選択を復元し、ルートを解決できたプロジェクトにだけタブを 1 枚ずつ、まだ起動していない状態で登録する
    /// (PTY は最初の接続で起動する。cwd はルート)。解決できなかったもの・`startup.roots` に無いもの・ゴミ箱の中や
    /// 別のプロジェクトと同じフォルダに解決されたものは「見つからない」にしてタブを作らない(保存値は読んだまま)。
    /// ペインの寸法は保存値に規則(`PaneSizes::sanitized`)を当て、無い項目は初期値にする。
    /// 表示モードは保存値のまま(FR-048。無ければタブ表示)。ページはフォーカス中のタイル(選択中のプロジェクトの
    /// 表示中のタブ)のページになり、拡大はしていない(前提 (b))。
    /// 文字の大きさと透過は保存値に範囲の規則を当てる(読むときに当ててあるが、範囲の外を持たないように当て直す)。
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
        let mut ws = Self::empty(app_version, home, parent_env);
        ws.projects = ProjectStore::restore(state.projects, state.selected_project_id);
        ws.layout = pane_sizes(&state.layout);
        ws.display.mode = state.layout.display_mode;
        ws.font_sizes = FontSizes::default()
            .with(FontSizeTarget::Terminal, state.layout.font_sizes.terminal)
            .with(FontSizeTarget::Interface, state.layout.font_sizes.interface);
        ws.translucency = Translucency::default()
            .with(TranslucencyTarget::Window, state.layout.translucency.window)
            .with(
                TranslucencyTarget::Terminal,
                state.layout.translucency.terminal,
            );
        ws.editor_vim = state.layout.editor_vim;
        ws.theme = state.layout.theme;
        ws.check_updates = state.layout.check_updates;
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

    fn empty<I, K, V>(app_version: impl Into<String>, home: PathBuf, parent_env: I) -> Self
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
            tabs: TerminalStore::default(),
            editors: EditorStore::default(),
            terminals: HashMap::new(),
            layout: PaneSizes::default(),
            display: DisplayState::default(),
            font_sizes: FontSizes::default(),
            translucency: Translucency::default(),
            editor_vim: false,
            theme: ThemeMode::default(),
            check_updates: false,
            notice: None,
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
    /// そのプロジェクトに動作中のものか未保存のエディタがあり `confirmed` が偽なら、何も変えずに確認を求める
    /// (ADR-015 の 4)。そうでなければタブとエディタをすべて外してから一覧から外し、外したタブを返す
    /// (セッションの後片付けは呼び出し側。エディタの未保存の変更は捨てる)。
    /// 選択中だったなら選択は次(無ければ前、0 件なら無し)に移る。タイル表示で、それでフォーカス中のタイルが無くなり
    /// ほかにタイルが残っていれば、消したプロジェクトのタイルがあった位置のタイル(末尾だったら 1 つ前)にフォーカスを移す
    /// (`close_terminal` と同じ規則。M4 のレビューの S4)。
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
        let unsaved = self.unsaved_editors_in(Some(id));
        if (!busy.is_empty() || !unsaved.is_empty()) && !confirmed {
            return Ok(RemoveOutcome::NeedsConfirmation {
                running: busy,
                unsaved,
            });
        }
        // 消すプロジェクトのタイルがあった位置(先頭のタイルの位置)。閉じる前にフォーカスがあったときだけ使う。
        let position = self
            .active_terminal()
            .and_then(|_| self.tabs.tabs(id).first().map(|tab| tab.id.clone()))
            .and_then(|first| self.ordered_tabs().position(|tab| tab.id == first));
        let terminals = self.tabs.remove_project(id);
        self.editors.remove_project(id);
        for terminal in &terminals {
            self.terminals.remove(terminal);
        }
        self.projects.remove(id)?;
        self.refocus_lost_tile(position);
        self.sync_display();
        Ok(RemoveOutcome::Removed { terminals })
    }

    /// 動作中のターミナル(`running`。呼び出し側が調べたもの)を、プロジェクト名とタブ名つきで返す(FR-063 の確認の材料)。
    /// 順はプロジェクトの並び順・タブの順。いま無い ID は無視する。
    #[must_use]
    pub fn running_terminals(&self, running: &[TerminalId]) -> Vec<RunningTerminal> {
        self.ordered_tabs()
            .filter(|tab| running.contains(&tab.id))
            .map(|tab| RunningTerminal {
                terminal_id: tab.id.clone(),
                project_id: Some(tab.project.clone()),
                project_name: self.projects.get(&tab.project).map(Project::name),
                label: tab.label.clone(),
            })
            .collect()
    }

    /// 状態ファイルに保存する内容(FR-056)。表示値(見つからない・表示用のパス・知らせ・タブ・エディタ)を含めない
    /// (エディタは再起動で開き直さない。ADR-015 の【要確認】2 の A)。
    #[must_use]
    pub fn persisted_state(&self) -> PersistedState {
        PersistedState {
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
                // ページと拡大中は保存しない(設計書 §6.1)
                display_mode: self.display.mode,
                font_sizes: self.font_sizes,
                translucency: self.translucency,
                editor_vim: self.editor_vim,
                theme: self.theme,
                check_updates: self.check_updates,
            },
        }
    }

    /// いまのテーマの選び方。
    #[must_use]
    pub fn theme(&self) -> ThemeMode {
        self.theme
    }

    /// テーマの選び方を替える(ADR-016)。変わったら `true`(このときだけ写しを送る。写しを送ると保存が予約され、
    /// システム以外のときだけ `ui.theme` が書かれる)。「既定に戻す」(`reset_appearance`)はこれを変えない
    /// (数値の調整を戻すボタンで、選んだテーマまでは戻さない)。
    pub fn set_theme(&mut self, theme: ThemeMode) -> bool {
        let changed = self.theme != theme;
        self.theme = theme;
        changed
    }

    /// 起動時に新しい版を確かめるか。
    #[must_use]
    pub fn check_updates(&self) -> bool {
        self.check_updates
    }

    /// 起動時の新しい版の確認を切り替える(ADR-018)。変わったら `true`(このときだけ写しを送る。真のときだけ
    /// `ui.checkUpdates` が書かれる)。「既定に戻す」はこれを変えない。
    pub fn set_check_updates(&mut self, enabled: bool) -> bool {
        let changed = self.check_updates != enabled;
        self.check_updates = enabled;
        changed
    }

    /// エディタの Vim のキー操作を切り替える(ADR-015 の 6、計画書 T81e)。変わったら `true`(このときだけ写しを送る。
    /// 写しを送ると保存が予約され、真のときだけ `ui.editorVim` が書かれる)。見た目の「既定に戻す」(`reset_appearance`)は
    /// これを変えない(見た目ではなく操作の選択なので)。
    pub fn set_editor_vim(&mut self, enabled: bool) -> bool {
        let changed = self.editor_vim != enabled;
        self.editor_vim = enabled;
        changed
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
                    .tabs(&p.id)
                    .iter()
                    .map(|tab| tab.id.clone())
                    .collect();
                let active_terminal_id = self.tabs.active(&p.id).cloned();
                let show_hidden = self.projects.get(&p.id).is_some_and(Project::show_hidden);
                let editor_ids = self.editors.tabs(&p.id).map(|t| t.id.clone()).collect();
                let shown_editor_id = self.editors.shown(&p.id).cloned();
                ProjectCardSnapshot {
                    id: p.id,
                    name: p.name,
                    color: p.color,
                    display_path: p.display_path,
                    missing: p.missing,
                    show_hidden,
                    terminal_ids,
                    active_terminal_id,
                    editor_ids,
                    shown_editor_id,
                }
            })
            .collect();
        let terminals = self
            .ordered_tabs()
            .map(|tab| TerminalTabSnapshot {
                id: tab.id.clone(),
                project_id: Some(tab.project.clone()),
                label: tab.label.clone(),
                status: self
                    .terminals
                    .get(&tab.id)
                    .map_or(TerminalStatus::NotStarted, |entry| entry.status.clone()),
                attention: tab.attention,
            })
            .collect();
        let editors = self
            .ordered_editors()
            .map(|tab| EditorTabSnapshot {
                id: tab.id.clone(),
                project_id: tab.project.clone(),
                path: tab.path.clone(),
                label: tab.label().to_owned(),
                dirty: tab.dirty,
                read_only: tab.read_only,
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
            font_sizes: self.font_sizes.into(),
            translucency: self.translucency.into(),
            editors,
            editor: EditorSettingsSnapshot {
                vim: self.editor_vim,
            },
            theme: self.theme,
            check_updates: self.check_updates,
        }
    }

    /// プロジェクトの末尾にタブを足し、ルートを cwd にして登録する。プロジェクトが無ければ `None`。
    /// 新しいタブが表示中になるので、そのプロジェクトの表示中のエディタは隠す(閉じない)。
    pub(super) fn add_tab(&mut self, project: &ProjectId) -> Option<TerminalId> {
        let cwd = self.projects.get(project)?.path().to_path_buf();
        let id = self.tabs.add(project);
        self.editors.hide(project);
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

    /// すべてのタブ。プロジェクトの並び順、その中はタブの順。
    pub(super) fn ordered_tabs(&self) -> impl Iterator<Item = &TerminalTab> {
        self.projects
            .projects()
            .iter()
            .flat_map(|p| self.tabs.tabs(p.id()))
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
