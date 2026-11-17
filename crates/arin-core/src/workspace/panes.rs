//! `Workspace` のペインの寸法の口(計画書 T33d、FR-009)。規則(最小寸法・不正な値)は `layout::panes::PaneSizes`。

use super::state::Workspace;

impl Workspace {
    /// 左ペインの幅とカード領域の高さを変える。不正な値は `PaneSizes` の規則で直す。
    /// 直したあとの値が前と違えば `true`(呼び出し側が写しを送り、保存が予約される)。
    pub fn set_pane_sizes(&mut self, sidebar_width: f64, card_area_height: f64) -> bool {
        let before = self.layout;
        self.layout.set_sidebar_width(sidebar_width);
        self.layout.set_card_area_height(card_area_height);
        self.layout != before
    }

    /// 左ペインを折りたたむ・戻す。幅は変えない。前と違えば `true`。
    pub fn set_sidebar_collapsed(&mut self, collapsed: bool) -> bool {
        let before = self.layout;
        self.layout.set_sidebar_collapsed(collapsed);
        self.layout != before
    }
}
