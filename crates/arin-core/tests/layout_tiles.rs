//! タイルのグリッドとページの規則、表示モードの型(FR-048・FR-051・FR-052、C-10・C-11・C-23、
//! 設計書 §5.1 の `DisplayMode`・`TileLayout`、§15.2 #4 の A、ADR-011、計画書 T46a・M4 の【仮定】2・3)。
//!
//! ページ番号は 0 から数える(画面の `1/2` の表示は 1 を足して作る)。

use arin_core::layout::tiles::{
    DisplayMode, TILES_PER_PAGE, TileGrid, TileLayout, clamp_page, grid_for_count, grid_for_page,
    page_count, page_of, page_range,
};

fn grid(cols: u32, rows: u32) -> TileGrid {
    TileGrid { cols, rows }
}

#[test]
fn _1_ページは最大_9_枚() {
    assert_eq!(TILES_PER_PAGE, 9);
}

#[test]
fn _1_枚は_1x1() {
    assert_eq!(grid_for_count(1), grid(1, 1));
}

#[test]
fn _2_枚は_2x1() {
    assert_eq!(grid_for_count(2), grid(2, 1));
}

#[test]
fn _3_枚と_4_枚は_2x2() {
    for count in [3, 4] {
        assert_eq!(grid_for_count(count), grid(2, 2), "{count} 枚");
    }
}

#[test]
fn _5_枚と_6_枚は_3x2() {
    for count in [5, 6] {
        assert_eq!(grid_for_count(count), grid(3, 2), "{count} 枚");
    }
}

#[test]
fn _7_枚から_9_枚は_3x3() {
    for count in [7, 8, 9] {
        assert_eq!(grid_for_count(count), grid(3, 3), "{count} 枚");
    }
}

#[test]
fn 枚数ごとのグリッドの表() {
    // FR-051 の AC の表をそのまま書く(列, 行)。
    let table = [
        (1, (1, 1)),
        (2, (2, 1)),
        (3, (2, 2)),
        (4, (2, 2)),
        (5, (3, 2)),
        (6, (3, 2)),
        (7, (3, 3)),
        (8, (3, 3)),
        (9, (3, 3)),
    ];
    for (count, (cols, rows)) in table {
        let g = grid_for_count(count);
        assert_eq!(g, grid(cols, rows), "{count} 枚");
        // どのグリッドも枚数が収まり、1 行ぶん余らない。
        assert!(
            (g.cols * g.rows) as usize >= count,
            "{count} 枚が収まらない"
        );
        assert!(
            ((g.rows - 1) * g.cols) < count as u32,
            "{count} 枚で行が余る"
        );
    }
}

#[test]
fn _0_枚のグリッドは_0x0() {
    assert_eq!(grid_for_count(0), grid(0, 0));
}

#[test]
fn _1_ページに収まらない枚数のグリッドは_3x3_に留める() {
    for count in [10, 19, 100] {
        assert_eq!(grid_for_count(count), grid(3, 3), "{count} 枚");
    }
}

#[test]
fn グリッドは列x行で_3x2_は_3_列_2_行() {
    let g = grid_for_count(5);

    assert_eq!(g.cols, 3);
    assert_eq!(g.rows, 2);
    assert_eq!(
        serde_json::to_value(g).unwrap(),
        serde_json::json!({ "cols": 3, "rows": 2 })
    );
}

#[test]
fn _10_枚は_2_ページで_1_ページ目は_9_枚_3x3_で_2_ページ目は_1_枚_1x1() {
    assert_eq!(page_count(10), 2);

    assert_eq!(page_range(10, 0), 0..9);
    assert_eq!(grid_for_page(10, 0), grid(3, 3));

    assert_eq!(page_range(10, 1), 9..10);
    assert_eq!(grid_for_page(10, 1), grid(1, 1));
}

#[test]
fn _19_枚は_3_ページで最後は_1_枚() {
    assert_eq!(page_count(19), 3);
    assert_eq!(page_range(19, 0), 0..9);
    assert_eq!(page_range(19, 1), 9..18);
    assert_eq!(page_range(19, 2), 18..19);
    assert_eq!(grid_for_page(19, 2), grid(1, 1));
}

#[test]
fn _0_枚はページが無い() {
    assert_eq!(page_count(0), 0);
    assert_eq!(page_range(0, 0), 0..0);
    assert_eq!(page_range(0, 5), 0..0);
    assert_eq!(grid_for_page(0, 0), grid(0, 0));
    assert_eq!(clamp_page(0, 3), 0);
}

#[test]
fn ページ数の境界() {
    let table = [
        (1, 1),
        (8, 1),
        (9, 1),
        (10, 2),
        (17, 2),
        (18, 2),
        (19, 3),
        (27, 3),
        (28, 4),
    ];
    for (total, pages) in table {
        assert_eq!(page_count(total), pages, "{total} 枚");
    }
}

#[test]
fn ページ番号から並びの範囲が決まる() {
    assert_eq!(page_range(1, 0), 0..1);
    assert_eq!(page_range(9, 0), 0..9);
    assert_eq!(page_range(18, 1), 9..18);
    assert_eq!(page_range(20, 2), 18..20);

    // どの枚数でも、ページの範囲を順につなぐと 0..総数 を重なりなく覆う。
    for total in 0..=40 {
        let mut next = 0;
        for page in 0..page_count(total) {
            let range = page_range(total, page);
            assert_eq!(range.start, next, "{total} 枚の {page} ページ目");
            assert!(!range.is_empty(), "{total} 枚の {page} ページ目が空");
            assert!(range.len() <= TILES_PER_PAGE);
            next = range.end;
        }
        assert_eq!(next, total, "{total} 枚");
    }
}

#[test]
fn n_番目のターミナルのページ() {
    assert_eq!(page_of(0), 0);
    assert_eq!(page_of(8), 0);
    assert_eq!(page_of(9), 1);
    assert_eq!(page_of(17), 1);
    assert_eq!(page_of(18), 2);

    // n 番目は、そのページの範囲に入っている。
    for total in 1..=40 {
        for index in 0..total {
            let page = page_of(index);
            assert!(page < page_count(total));
            assert!(
                page_range(total, page).contains(&index),
                "{total} 枚の {index} 番目"
            );
        }
    }
}

#[test]
fn 範囲外のページ番号は最後のページに丸める() {
    assert_eq!(clamp_page(10, 1), 1);
    assert_eq!(clamp_page(10, 2), 1);
    assert_eq!(clamp_page(10, usize::MAX), 1);
    assert_eq!(clamp_page(9, 1), 0);
    assert_eq!(clamp_page(1, 7), 0);
    assert_eq!(clamp_page(19, 0), 0);

    // 範囲の計算も丸めたページで行う(外を指しても空にならない)。
    assert_eq!(page_range(10, 5), 9..10);
    assert_eq!(grid_for_page(10, 5), grid(1, 1));
    assert_eq!(page_range(19, usize::MAX), 18..19);
}

#[test]
fn タイルの並びはページの丸めと計算をまとめて返す() {
    let layout = TileLayout::new(10, 4);

    assert_eq!(
        layout,
        TileLayout {
            page: 1,
            page_count: 2,
            grid: grid(1, 1),
            range: 9..10,
        }
    );
    assert_eq!(
        TileLayout::new(0, 0),
        TileLayout {
            page: 0,
            page_count: 0,
            grid: grid(0, 0),
            range: 0..0,
        }
    );
    assert_eq!(
        TileLayout::new(9, 0),
        TileLayout {
            page: 0,
            page_count: 1,
            grid: grid(3, 3),
            range: 0..9,
        }
    );
}

#[test]
fn 同じ入力なら同じ結果() {
    for total in 0..=30 {
        for page in 0..5 {
            assert_eq!(TileLayout::new(total, page), TileLayout::new(total, page));
        }
    }
}

#[test]
fn 表示モードの_json_は_tabs_と_tiles() {
    assert_eq!(
        serde_json::to_value(DisplayMode::Tabs).unwrap(),
        serde_json::json!("tabs")
    );
    assert_eq!(
        serde_json::to_value(DisplayMode::Tiles).unwrap(),
        serde_json::json!("tiles")
    );
    assert_eq!(
        serde_json::from_value::<DisplayMode>(serde_json::json!("tabs")).unwrap(),
        DisplayMode::Tabs
    );
    assert_eq!(
        serde_json::from_value::<DisplayMode>(serde_json::json!("tiles")).unwrap(),
        DisplayMode::Tiles
    );
    assert!(serde_json::from_value::<DisplayMode>(serde_json::json!("grid")).is_err());
}

#[test]
fn 表示モードの既定はタブ表示で切り替えると互いに入れ替わる() {
    assert_eq!(DisplayMode::default(), DisplayMode::Tabs);
    assert_eq!(DisplayMode::Tabs.toggled(), DisplayMode::Tiles);
    assert_eq!(DisplayMode::Tiles.toggled(), DisplayMode::Tabs);
}
