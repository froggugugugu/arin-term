//! ペインの寸法の保存値の規則(FR-009、計画書 M2 の【仮定】10)。
//!
//! 最小寸法と初期値は画面の `src/logic/panes/paneLayout.ts` と同じ値にする。
//! ウィンドウの大きさに合わせた表示値の計算は画面が行うので、ここでは扱わない。

use arin_core::layout::panes::{MIN_CARD_AREA_HEIGHT, MIN_SIDEBAR_WIDTH, PaneSizes};

#[test]
fn 初期値と最小寸法は画面と同じ() {
    let sizes = PaneSizes::default();

    assert_eq!(sizes.sidebar_width, 260.0);
    assert!(!sizes.sidebar_collapsed);
    assert_eq!(sizes.card_area_height, 200.0);
    assert_eq!(MIN_SIDEBAR_WIDTH, 180.0);
    assert_eq!(MIN_CARD_AREA_HEIGHT, 72.0);
}

#[test]
fn 最小を下回る寸法は最小に留める() {
    let sizes = PaneSizes {
        sidebar_width: 100.0,
        sidebar_collapsed: false,
        card_area_height: 0.0,
    }
    .sanitized();

    assert_eq!(sizes.sidebar_width, MIN_SIDEBAR_WIDTH);
    assert_eq!(sizes.card_area_height, MIN_CARD_AREA_HEIGHT);
}

#[test]
fn 最小以上の寸法はそのまま() {
    let given = PaneSizes {
        sidebar_width: 180.0,
        sidebar_collapsed: true,
        card_area_height: 512.5,
    };

    assert_eq!(given.sanitized(), given);
}

#[test]
fn 数でない値_無限大_負の値は既定値() {
    let defaults = PaneSizes::default();
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0, -500.0] {
        let sizes = PaneSizes {
            sidebar_width: bad,
            sidebar_collapsed: false,
            card_area_height: bad,
        }
        .sanitized();

        assert_eq!(sizes.sidebar_width, defaults.sidebar_width, "{bad}");
        assert_eq!(sizes.card_area_height, defaults.card_area_height, "{bad}");
    }
}

#[test]
fn 寸法を変えるときも同じ規則を当てる() {
    let mut sizes = PaneSizes::default();

    sizes.set_sidebar_width(400.0);
    sizes.set_card_area_height(300.0);
    assert_eq!(
        (sizes.sidebar_width, sizes.card_area_height),
        (400.0, 300.0)
    );

    sizes.set_sidebar_width(10.0);
    sizes.set_card_area_height(10.0);
    assert_eq!(
        (sizes.sidebar_width, sizes.card_area_height),
        (MIN_SIDEBAR_WIDTH, MIN_CARD_AREA_HEIGHT)
    );

    sizes.set_sidebar_width(f64::NAN);
    sizes.set_card_area_height(-3.0);
    assert_eq!(
        (sizes.sidebar_width, sizes.card_area_height),
        (260.0, 200.0)
    );
}

#[test]
fn 折りたたんでも幅を覚えていて戻すと同じ幅() {
    let mut sizes = PaneSizes::default();
    sizes.set_sidebar_width(333.0);

    sizes.set_sidebar_collapsed(true);
    assert!(sizes.sidebar_collapsed);
    assert_eq!(sizes.sanitized().sidebar_width, 333.0);

    sizes.set_sidebar_collapsed(false);
    assert!(!sizes.sidebar_collapsed);
    assert_eq!(sizes.sidebar_width, 333.0);
}

#[test]
fn json_の項目名は画面と同じ_camel_case() {
    let json = serde_json::to_value(PaneSizes::default()).unwrap();

    assert_eq!(
        json,
        serde_json::json!({ "sidebarWidth": 260.0, "sidebarCollapsed": false, "cardAreaHeight": 200.0 })
    );
    let back: PaneSizes = serde_json::from_value(json).unwrap();
    assert_eq!(back, PaneSizes::default());
}
