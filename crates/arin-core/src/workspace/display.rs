//! `Workspace` の表示の状態: 表示モード・タイルのページ・拡大と、タイルへのフォーカス
//! (FR-048・FR-051〜054、NFR-009、設計書 §3.4・§6.1・§8、ADR-011、計画書 T46b)。
//!
//! フォーカス中のタイルは新しい値を持たず、「選択中のプロジェクトの表示中のタブ」(`active_terminal`)で表す
//! (ADR-011 の 3)。タイルの並びは `ordered_tabs`(プロジェクト順 → タブ順。見つからないプロジェクトのタブも並べる)。
//! グリッドとページの規則は `layout::tiles` の純粋な関数を使い、ここは状態とのつなぎだけを置く。
//!
//! 不変条件(どの口のあとも成り立つ):
//! - ページは範囲内(0 枚なら 0)。フォーカス中のタイルがあれば、ページはそのタイルのページ
//! - 拡大中はタイル表示で、拡大しているのはフォーカス中のタイル。拡大していたターミナルが消えたら拡大を解く
//! - フォーカス中のターミナルにアテンションの印は無い(計画書 T48b、ADR-011 の 7。`note_attention` はフォーカス中なら
//!   立てず、`sync_display` がフォーカス中になったものの印を消す)。タイル表示で見えているだけのタイルには立つ
//!
//! 読み取りは `resolve` で保存値から毎回計算するので、`sync_display` を呼び忘れた口があっても範囲の外を指さない。
//! `sync_display` は計算した結果を保存値に戻す(フォーカスが無くなったときのページと、拡大の付いていく先を覚える)。

use super::snapshot::DisplaySnapshot;
use super::state::Workspace;
use crate::layout::tiles::{DisplayMode, TileLayout, page_of};
use crate::project::ProjectId;
use crate::terminal::{AttentionPolicy, TerminalError, TerminalId};

/// 表示の保存値。ページと拡大は状態ファイルに保存しない(設計書 §6.1。表示モードの保存は T46c)。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct DisplayState {
    pub(super) mode: DisplayMode,
    /// 表示中のページ(0 から)。フォーカス中のタイルがあればそのページに合わせ直す。
    pub(super) page: usize,
    /// 拡大しているターミナル。写しでは真偽だけを出す。タブを替えるとフォーカス中のタイルに付いていき、
    /// このターミナルが消えたら解く。
    pub(super) maximized: Option<TerminalId>,
}

/// 保存値といまのタブから決まる表示。
struct DisplayView {
    mode: DisplayMode,
    /// 全タイルの並び。
    tiles: Vec<TerminalId>,
    focused: Option<TerminalId>,
    layout: TileLayout,
    /// 拡大しているターミナル(=フォーカス中のタイル)。拡大していなければ `None`。
    maximized: Option<TerminalId>,
}

impl Workspace {
    /// いまの表示モード。
    #[must_use]
    pub fn display_mode(&self) -> DisplayMode {
        self.display.mode
    }

    /// 表示モードを変える(FR-048)。変わったら `true`。タブ表示にすると拡大は解ける(【仮定】7)。
    /// タイル表示にすると、フォーカス中のタイルのページを出す。
    pub fn set_display_mode(&mut self, mode: DisplayMode) -> bool {
        if self.display.mode == mode {
            return false;
        }
        self.display.mode = mode;
        if mode == DisplayMode::Tabs {
            self.display.maximized = None;
        }
        self.sync_display();
        true
    }

    /// 表示モードをもう一方にする(⇧⌘\)。必ず変わるので常に `true`。
    pub fn toggle_display_mode(&mut self) -> bool {
        self.set_display_mode(self.display.mode.toggled())
    }

    /// タイルにフォーカスする(タイルのクリック。FR-053、AC-M15)。そのタブのプロジェクトを選び、そのタブを表示中にする。
    /// 表示モードによらず受け付ける。選択か表示中のタブが変わったら `true`。
    ///
    /// # Errors
    ///
    /// その ID のタブが無いとき、持ち主のプロジェクトが無いタブ(検証画面)のとき(どちらも `TerminalError::NotFound`)。
    /// どの場合も何も変えない。
    pub fn focus_terminal(&mut self, id: &TerminalId) -> Result<bool, TerminalError> {
        let project = self
            .tabs
            .tab(id)
            .and_then(|tab| tab.project.clone())
            .filter(|project| self.projects.get(project).is_some())
            .ok_or_else(|| TerminalError::NotFound(id.clone()))?;
        let before = (self.projects.selected().cloned(), self.active_terminal());
        self.focus_tile(id, Some(&project));
        Ok((self.projects.selected().cloned(), self.active_terminal()) != before)
    }

    /// 次のページへ送り、その先頭のタイルにフォーカスする(FR-052、【仮定】3・4)。送れたら `true`。
    /// タブ表示・拡大中・最後のページでは何もしない(循環しない)。
    pub fn next_page(&mut self) -> bool {
        let view = self.resolve();
        if !Self::can_turn_page(&view) || view.layout.page + 1 >= view.layout.page_count {
            return false;
        }
        self.turn_to_page(&view, view.layout.page + 1)
    }

    /// 前のページへ送り、その先頭のタイルにフォーカスする(FR-052、【仮定】3・4)。送れたら `true`。
    /// タブ表示・拡大中・最初のページでは何もしない(循環しない)。
    pub fn previous_page(&mut self) -> bool {
        let view = self.resolve();
        if !Self::can_turn_page(&view) || view.layout.page == 0 {
            return false;
        }
        self.turn_to_page(&view, view.layout.page - 1)
    }

    /// フォーカス中のタイルを拡大する、または拡大を解く(FR-054、前提 (d))。変わったら `true`。
    /// タブ表示のとき・フォーカス中のタイルが無いときは何もしない。拡大を解くと、フォーカス中のタイルのページに戻る。
    pub fn toggle_maximize(&mut self) -> bool {
        let view = self.resolve();
        if view.mode != DisplayMode::Tiles {
            return false;
        }
        if view.maximized.is_some() {
            self.display.maximized = None;
        } else if let Some(focused) = view.focused {
            self.display.maximized = Some(focused);
        } else {
            return false;
        }
        self.sync_display();
        true
    }

    /// 表示中のターミナルの並び(NFR-009。画面がどのターミナルを描くかに使う。アテンションの判定には使わない。
    /// そちらは `attended_terminals`)。
    ///
    /// タブ表示は選択中のプロジェクトの表示中のタブ(0 枚なら空)、タイル表示は表示中のページのタイル(並びの順)、
    /// 拡大中は拡大した 1 枚。
    #[must_use]
    pub fn visible_terminals(&self) -> Vec<TerminalId> {
        let view = self.resolve();
        match (view.mode, view.maximized) {
            (DisplayMode::Tabs, _) => view.focused.into_iter().collect(),
            (DisplayMode::Tiles, Some(maximized)) => vec![maximized],
            (DisplayMode::Tiles, None) => view.tiles[view.layout.range].to_vec(),
        }
    }

    /// アテンションの判定で「いま見ている」とみなすターミナルの並び(FR-055、ADR-011 の 7。0 枚か 1 枚)。
    ///
    /// どの表示モードでもフォーカス中のターミナル(タブ表示=選択中のプロジェクトの表示中のタブ、
    /// タイル表示=フォーカス中のタイル、拡大中=拡大した 1 枚)。タイル表示で同じページに見えているだけのタイルは含めない
    /// (含めると、FR-055 の「アテンションのあるタイルのタイトルにバッジが付く」が満たせない)。
    #[must_use]
    pub fn attended_terminals(&self) -> Vec<TerminalId> {
        self.resolve().focused.into_iter().collect()
    }

    /// アテンションの契機があった(FR-055。読み取りスレッドが `AttentionScanner` の契機のあとに呼ぶ。計画書 T48c)。
    ///
    /// そのターミナルがフォーカス中(`attended_terminals`)でなければ印を立てる(`AttentionPolicy`。§15.2 #10 の A)。
    /// 印が立ったら `true`(このときだけ写しを送る)。フォーカス中のとき・既に立っているとき・その ID のタブが無いとき
    /// (閉じたあとに届いた契機)は何もせず `false`。
    pub fn note_attention(&mut self, id: &TerminalId) -> bool {
        AttentionPolicy::should_mark(id, &self.attended_terminals()) && self.tabs.set_attention(id)
    }

    /// 写しの `display`。
    pub(super) fn display_snapshot(&self) -> DisplaySnapshot {
        let view = self.resolve();
        let mut attention_pages: Vec<u32> = Vec::new();
        for (index, tab) in self.ordered_tabs().enumerate() {
            let page = to_u32(page_of(index));
            if tab.attention && attention_pages.last() != Some(&page) {
                attention_pages.push(page);
            }
        }
        DisplaySnapshot {
            mode: view.mode,
            maximized: view.maximized.is_some(),
            page: to_u32(view.layout.page),
            page_count: to_u32(view.layout.page_count),
            grid: view.layout.grid,
            tile_ids: view.tiles[view.layout.range].to_vec(),
            focused_terminal_id: view.focused,
            attention_pages,
        }
    }

    /// カードを選んだあと(`select_project`)に、タイル表示ならそのプロジェクトの最初のタイルのページへ移す(FR-053、【仮定】5)。
    /// 表示中のタブがそのページに無ければ、最初のタイルを表示中にする。タブが 0 枚ならページは変えない。
    /// 拡大中はページが見えないので、表示中のタブのまま拡大して出す(【仮定】7)。
    pub(super) fn align_selected_project_to_tiles(&mut self) {
        if self.display.mode != DisplayMode::Tiles || self.display.maximized.is_some() {
            return;
        }
        let Some(project) = self.projects.selected().cloned() else {
            return;
        };
        let tiles = self.tile_ids();
        let position = |id: &TerminalId| tiles.iter().position(|t| t == id);
        let Some(first) = self.tabs.tabs(Some(&project)).first().map(|t| t.id.clone()) else {
            return;
        };
        let first_page = position(&first).map(page_of);
        let active_page = self
            .tabs
            .active(Some(&project))
            .and_then(position)
            .map(page_of);
        if first_page != active_page {
            // `first` は直前に並びから取ったものなので必ずある。
            let _ = self.tabs.activate(&first);
        }
    }

    /// 保存値を、いまのタブから決まる表示に合わせ直す(選択・タブの追加・閉じる・切替・削除・シェルの終了のあとに呼ぶ)。
    /// フォーカス中になったターミナルのアテンションの印を消す(設計書 §7.1「アテンション」の 5)。
    /// ページを送っただけで見えるようになったタイルの印は消さない。
    pub(super) fn sync_display(&mut self) {
        let view = self.resolve();
        self.display.page = view.layout.page;
        self.display.maximized = view.maximized;
        if let Some(focused) = &view.focused {
            self.tabs.clear_attention(focused);
        }
    }

    fn resolve(&self) -> DisplayView {
        let tiles = self.tile_ids();
        let focused = self.active_terminal();
        let focused_index = focused
            .as_ref()
            .and_then(|f| tiles.iter().position(|t| t == f));
        let page = focused_index.map_or(self.display.page, page_of);
        let layout = TileLayout::new(tiles.len(), page);
        let maximized = match (&self.display.maximized, &focused) {
            (Some(maximized), Some(_))
                if self.display.mode == DisplayMode::Tiles && tiles.contains(maximized) =>
            {
                focused.clone()
            }
            _ => None,
        };
        DisplayView {
            mode: self.display.mode,
            tiles,
            focused,
            layout,
            maximized,
        }
    }

    fn tile_ids(&self) -> Vec<TerminalId> {
        self.ordered_tabs().map(|tab| tab.id.clone()).collect()
    }

    fn can_turn_page(view: &DisplayView) -> bool {
        view.mode == DisplayMode::Tiles && view.maximized.is_none()
    }

    fn turn_to_page(&mut self, view: &DisplayView, page: usize) -> bool {
        let range = TileLayout::new(view.tiles.len(), page).range;
        let Some(first) = view.tiles.get(range.start).cloned() else {
            return false;
        };
        let project = self.tabs.tab(&first).and_then(|tab| tab.project.clone());
        self.display.page = page;
        self.focus_tile(&first, project.as_ref());
        true
    }

    /// そのタイルの持ち主を選び(持ち主が無ければ選択は変えない)、タブを表示中にしてから表示を合わせ直す。
    fn focus_tile(&mut self, id: &TerminalId, project: Option<&ProjectId>) {
        if let Some(project) = project {
            // 呼ぶ側が持ち主のプロジェクトがあることを確かめている。
            let _ = self.projects.select(project);
        }
        // 呼ぶ側がタブがあることを確かめている。
        let _ = self.tabs.activate(id);
        self.sync_display();
    }
}

fn to_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}
