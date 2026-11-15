//! タイル表示のグリッドとページの規則、表示モードの型(FR-048・FR-051・FR-052、C-10・C-11・C-23、
//! 設計書 §5.1 の `DisplayMode`・`TileLayout`、§15.2 #4 の A、ADR-011、計画書 T46a)。
//!
//! 純粋な関数だけを置く(入出力・時計を持たない)。同じ入力なら同じ結果を返す。
//! ページ番号は 0 から数える(画面の `1/2` の表示は 1 を足して作る)。
//! タイルの並び(プロジェクト順 → タブ順)は呼ぶ側(`Workspace`)が決め、ここは枚数と位置だけを扱う。

use std::ops::Range;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 1 ページに並べるタイルの最大の枚数(C-11。10 枚目から次のページ)。
pub const TILES_PER_PAGE: usize = 9;

/// 右ペインの表示モード(FR-048。状態ファイルの `ui.displayMode`)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum DisplayMode {
    /// タブ表示(選択中のプロジェクトのタブを 1 枚ずつ出す)。既定。
    #[default]
    Tabs,
    /// タイル表示(全プロジェクトの全ターミナルをグリッドで並べる)。
    Tiles,
}

impl DisplayMode {
    /// もう一方の表示モード。
    #[must_use]
    pub fn toggled(self) -> Self {
        match self {
            Self::Tabs => Self::Tiles,
            Self::Tiles => Self::Tabs,
        }
    }
}

/// タイルのグリッドの寸法。表記は「列×行」(C-23。3×2 は 3 列 2 行)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TileGrid {
    /// 列の数。
    pub cols: u32,
    /// 行の数。
    pub rows: u32,
}

/// 1 ページの枚数からグリッドを決める(FR-051)。
///
/// 1→1×1、2→2×1、3〜4→2×2、5〜6→3×2、7〜9→3×3。0 枚は 0×0。
/// 1 ページに収まらない枚数(10 枚以上)は 3×3 に留める(1 ページは最大 9 枚)。
#[must_use]
pub fn grid_for_count(count: usize) -> TileGrid {
    let (cols, rows) = match count {
        0 => (0, 0),
        1 => (1, 1),
        2 => (2, 1),
        3 | 4 => (2, 2),
        5 | 6 => (3, 2),
        _ => (3, 3),
    };
    TileGrid { cols, rows }
}

/// 総数からページの数を決める(FR-052)。0 枚は 0 ページ。
#[must_use]
pub fn page_count(total: usize) -> usize {
    total.div_ceil(TILES_PER_PAGE)
}

/// ページ番号を範囲に丸める。範囲外は最後のページ。0 枚のときは 0。
#[must_use]
pub fn clamp_page(total: usize, page: usize) -> usize {
    page.min(page_count(total).saturating_sub(1))
}

/// n 番目(0 から)のタイルが載るページ。
#[must_use]
pub fn page_of(index: usize) -> usize {
    index / TILES_PER_PAGE
}

/// ページに載るタイルの並びの範囲(全体の並びの添字)。
///
/// ページ番号は `clamp_page` で丸めてから使う(外を指しても最後のページの範囲を返す)。0 枚は `0..0`。
#[must_use]
pub fn page_range(total: usize, page: usize) -> Range<usize> {
    let start = clamp_page(total, page) * TILES_PER_PAGE;
    let end = start.saturating_add(TILES_PER_PAGE).min(total);
    start.min(total)..end
}

/// ページのグリッド。最終ページは、そのページの枚数で決める(§15.2 #4 の A。10 枚なら 2 ページ目は 1×1)。
#[must_use]
pub fn grid_for_page(total: usize, page: usize) -> TileGrid {
    grid_for_count(page_range(total, page).len())
}

/// 総数と表示したいページから決まる、表示中のページの並び。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileLayout {
    /// 表示中のページ(丸めた後。0 から)。
    pub page: usize,
    /// ページの数(0 枚なら 0)。
    pub page_count: usize,
    /// 表示中のページのグリッド。
    pub grid: TileGrid,
    /// 表示中のページに載るタイルの範囲(全体の並びの添字)。
    pub range: Range<usize>,
}

impl TileLayout {
    /// 総数 `total` のタイルを、ページ `page`(範囲外は丸める)で並べたときの形。
    #[must_use]
    pub fn new(total: usize, page: usize) -> Self {
        let page = clamp_page(total, page);
        let range = page_range(total, page);
        Self {
            page,
            page_count: page_count(total),
            grid: grid_for_count(range.len()),
            range,
        }
    }
}
