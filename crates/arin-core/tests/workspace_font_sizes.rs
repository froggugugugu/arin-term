//! `Workspace` の文字の大きさの口(計画書 T68b、ADR-013 の 2。AC-F06・F07・F08)。
//! 規則(範囲・既定・刻み)は `layout::font_sizes`。ここでは持ち方・写し・保存する内容への出し方と、
//! 変わったときだけ真を返すこと(呼び出し側が写しを送り、保存が予約される)を見る。

use std::path::PathBuf;

use arin_core::layout::font_sizes::{FontSizeTarget, FontSizes};
use arin_core::persistence::codec::{PersistedLayout, PersistedState};
use arin_core::workspace::{FontSizesSnapshot, Startup, Workspace};

const HOME: &str = "/Users/me";

fn restore(state: PersistedState) -> Workspace {
    Workspace::restore(
        "1.2.3",
        PathBuf::from(HOME),
        [("HOME", HOME)],
        Startup {
            state,
            roots: Vec::new(),
            notice: None,
        },
    )
}

fn empty() -> Workspace {
    restore(PersistedState::default())
}

#[test]
fn 刻むと写しの値が変わり真を返す() {
    let mut ws = empty();

    assert!(ws.step_font_size(FontSizeTarget::Terminal, 1));
    assert!(ws.step_font_size(FontSizeTarget::Terminal, 1));
    assert!(ws.step_font_size(FontSizeTarget::Interface, -1));

    let expected = FontSizes {
        terminal: 17,
        interface: 12,
    };
    assert_eq!(ws.font_sizes(), expected);
    assert_eq!(ws.snapshot().font_sizes, FontSizesSnapshot::from(expected));
    assert_eq!(ws.snapshot().font_sizes.terminal.value, 17);
    assert_eq!(ws.snapshot().font_sizes.interface.value, 12);
    assert_eq!(ws.persisted_state().layout.font_sizes, expected);
}

#[test]
fn 端で刻んでも変わらず偽を返す() {
    let mut ws = empty();
    // 28 まで上げる(15 → 28 は 13 段)
    assert!(ws.step_font_size(FontSizeTarget::Terminal, 13));
    assert_eq!(ws.font_sizes().terminal, 28);
    let before = ws.snapshot();

    assert!(!ws.step_font_size(FontSizeTarget::Terminal, 1));
    assert!(!ws.step_font_size(FontSizeTarget::Terminal, 100));
    assert!(!ws.step_font_size(FontSizeTarget::Interface, 0));

    assert_eq!(ws.snapshot(), before);

    // 下の端も同じ(13 → 11 で留まる)
    assert!(ws.step_font_size(FontSizeTarget::Interface, -5));
    assert_eq!(ws.font_sizes().interface, 11);
    assert!(!ws.step_font_size(FontSizeTarget::Interface, -1));
    assert_eq!(ws.font_sizes().interface, 11);
}

#[test]
fn 既定に戻すと両方が既定になり既定のときは偽() {
    let mut ws = empty();
    assert!(!ws.reset_font_sizes(), "初めから既定なので変わらない");

    ws.step_font_size(FontSizeTarget::Terminal, 3);
    ws.step_font_size(FontSizeTarget::Interface, 2);

    assert!(ws.reset_font_sizes());
    assert_eq!(ws.font_sizes(), FontSizes::default());
    assert_eq!(
        ws.snapshot().font_sizes,
        FontSizesSnapshot::from(FontSizes::default())
    );
    assert!(!ws.reset_font_sizes(), "既定に戻したあとは変わらない");

    // 片方だけ変えていても戻る
    ws.step_font_size(FontSizeTarget::Interface, 1);
    assert!(ws.reset_font_sizes());
    assert_eq!(ws.font_sizes(), FontSizes::default());
}

#[test]
fn 保存した大きさで復元し保存する内容にも出る() {
    let sizes = FontSizes {
        terminal: 20,
        interface: 16,
    };
    let ws = restore(PersistedState {
        layout: PersistedLayout {
            font_sizes: sizes,
            ..PersistedLayout::default()
        },
        ..PersistedState::default()
    });

    assert_eq!(ws.font_sizes(), sizes);
    assert_eq!(ws.snapshot().font_sizes, FontSizesSnapshot::from(sizes));
    assert_eq!(ws.persisted_state().layout.font_sizes, sizes);
}

#[test]
fn 範囲の外の保存値は復元のときも端に留める() {
    // 保存値はふつう読むときに留めてあるが、`PersistedLayout` を直接作ったときも範囲の外を持たない
    let ws = restore(PersistedState {
        layout: PersistedLayout {
            font_sizes: FontSizes {
                terminal: 200,
                interface: 1,
            },
            ..PersistedLayout::default()
        },
        ..PersistedState::default()
    });

    assert_eq!(
        ws.font_sizes(),
        FontSizes {
            terminal: 28,
            interface: 11,
        }
    );
}
