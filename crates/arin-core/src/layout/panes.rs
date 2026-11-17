//! ペインの寸法の保存値の規則(FR-009、設計書 §6.2 の `ui`、計画書 T30b・M2 の【仮定】10)。
//!
//! 最小寸法・初期値・項目名は画面の `src/logic/panes/paneLayout.ts` と揃える。単位は CSS ピクセル。
//! ここが受け持つのは保存値の検査だけ。ウィンドウの大きさに合わせた表示値の計算は画面が行う
//! (表示値で保存値を縮めないので、ウィンドウを広げ直すと元の寸法に戻る)。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 左ペインの最小の幅。
pub const MIN_SIDEBAR_WIDTH: f64 = 180.0;
/// カード領域(左ペインの上段)の最小の高さ。
pub const MIN_CARD_AREA_HEIGHT: f64 = 72.0;

/// 左ペインの幅の初期値。
const DEFAULT_SIDEBAR_WIDTH: f64 = 260.0;
/// カード領域の高さの初期値。
const DEFAULT_CARD_AREA_HEIGHT: f64 = 200.0;

/// 保存するペインの寸法。折りたたみ中も幅は覚えておく(戻すと同じ幅)。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PaneSizes {
    /// 左ペインの幅。
    pub sidebar_width: f64,
    /// 左ペインを折りたたんでいるか。
    pub sidebar_collapsed: bool,
    /// カード領域(左ペインの上段)の高さ。
    pub card_area_height: f64,
}

impl Default for PaneSizes {
    fn default() -> Self {
        Self {
            sidebar_width: DEFAULT_SIDEBAR_WIDTH,
            sidebar_collapsed: false,
            card_area_height: DEFAULT_CARD_AREA_HEIGHT,
        }
    }
}

impl PaneSizes {
    /// 規則を当てた値を返す。数でない値・無限大・負の値は初期値に、最小を下回る値は最小にする。
    /// 上限は持たない(ウィンドウの大きさで決まるので、画面が表示値で留める)。
    #[must_use]
    pub fn sanitized(self) -> Self {
        Self {
            sidebar_width: clamp_size(self.sidebar_width, MIN_SIDEBAR_WIDTH, DEFAULT_SIDEBAR_WIDTH),
            sidebar_collapsed: self.sidebar_collapsed,
            card_area_height: clamp_size(
                self.card_area_height,
                MIN_CARD_AREA_HEIGHT,
                DEFAULT_CARD_AREA_HEIGHT,
            ),
        }
    }

    /// 左ペインの幅を変える(`sanitized` と同じ規則を当てる)。
    pub fn set_sidebar_width(&mut self, width: f64) {
        self.sidebar_width = clamp_size(width, MIN_SIDEBAR_WIDTH, DEFAULT_SIDEBAR_WIDTH);
    }

    /// カード領域の高さを変える(`sanitized` と同じ規則を当てる)。
    pub fn set_card_area_height(&mut self, height: f64) {
        self.card_area_height = clamp_size(height, MIN_CARD_AREA_HEIGHT, DEFAULT_CARD_AREA_HEIGHT);
    }

    /// 左ペインを折りたたむ・戻す。幅は変えない。
    pub fn set_sidebar_collapsed(&mut self, collapsed: bool) {
        self.sidebar_collapsed = collapsed;
    }
}

/// 1 つの寸法に規則を当てる。
fn clamp_size(value: f64, min: f64, default: f64) -> f64 {
    if !value.is_finite() || value < 0.0 {
        default
    } else {
        value.max(min)
    }
}
