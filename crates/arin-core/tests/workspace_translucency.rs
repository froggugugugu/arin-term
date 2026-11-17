//! `Workspace` の透過の口と「既定に戻す」(ADR-014 の 1。AC-G01・G04・G05)。
//! 規則(範囲・既定・刻み・丸め)は `layout::translucency`。ここでは持ち方・写し・保存する内容への出し方と、
//! 変わったときだけ真を返すこと(呼び出し側が写しを送り、保存が予約される)を見る。

use std::path::PathBuf;

use arin_core::layout::font_sizes::{FontSizeTarget, FontSizes};
use arin_core::layout::translucency::{Translucency, TranslucencyTarget};
use arin_core::persistence::codec::{PersistedLayout, PersistedState};
use arin_core::workspace::{Startup, TranslucencySnapshot, TranslucencyValueSnapshot, Workspace};

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
fn 写しには値と範囲と既定と刻みが載る() {
    let ws = empty();

    assert_eq!(
        ws.snapshot().translucency,
        TranslucencySnapshot {
            window: TranslucencyValueSnapshot {
                value: 45,
                min: 0,
                max: 100,
                default_value: 45,
                step: 5,
            },
            terminal: TranslucencyValueSnapshot {
                value: 20,
                min: 0,
                max: 80,
                default_value: 20,
                step: 5,
            },
        }
    );
    let json = serde_json::to_value(ws.snapshot().translucency).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "window": { "value": 45, "min": 0, "max": 100, "defaultValue": 45, "step": 5 },
            "terminal": { "value": 20, "min": 0, "max": 80, "defaultValue": 20, "step": 5 },
        })
    );
}

#[test]
fn 動かすと写しの値が変わり真を返す() {
    let mut ws = empty();

    assert!(ws.step_translucency(TranslucencyTarget::Window, 5));
    assert!(ws.step_translucency(TranslucencyTarget::Window, 5));
    assert!(ws.step_translucency(TranslucencyTarget::Terminal, -5));

    let expected = Translucency {
        window: 55,
        terminal: 15,
    };
    assert_eq!(ws.translucency(), expected);
    assert_eq!(
        ws.snapshot().translucency,
        TranslucencySnapshot::from(expected)
    );
    assert_eq!(ws.persisted_state().layout.translucency, expected);
    // 文字の大きさは変わらない
    assert_eq!(ws.font_sizes(), FontSizes::default());
}

#[test]
fn 端で動かしても変わらず偽を返す() {
    let mut ws = empty();
    assert!(ws.step_translucency(TranslucencyTarget::Terminal, 80));
    assert_eq!(ws.translucency().terminal, 80);
    let before = ws.snapshot();

    assert!(!ws.step_translucency(TranslucencyTarget::Terminal, 5));
    assert!(!ws.step_translucency(TranslucencyTarget::Terminal, 1000));
    assert!(!ws.step_translucency(TranslucencyTarget::Window, 0));
    // 丸めると今の値に戻る差分も変わらない
    assert!(!ws.step_translucency(TranslucencyTarget::Window, 2));
    assert_eq!(ws.snapshot(), before);

    assert!(ws.step_translucency(TranslucencyTarget::Window, -100));
    assert_eq!(ws.translucency().window, 0);
    assert!(!ws.step_translucency(TranslucencyTarget::Window, -5));
}

#[test]
fn 既定に戻すと文字の大きさと透過の_4_つが既定になり既定のときは偽() {
    let mut ws = empty();
    assert!(!ws.reset_appearance(), "初めから既定なので変わらない");

    ws.step_font_size(FontSizeTarget::Terminal, 2);
    ws.step_translucency(TranslucencyTarget::Window, 10);
    ws.step_translucency(TranslucencyTarget::Terminal, 10);

    assert!(ws.reset_appearance());
    assert_eq!(ws.font_sizes(), FontSizes::default());
    assert_eq!(ws.translucency(), Translucency::default());
    assert!(!ws.reset_appearance(), "既定に戻したあとは変わらない");

    // 透過の片方だけ変えていても戻る
    ws.step_translucency(TranslucencyTarget::Terminal, -20);
    assert!(ws.reset_appearance());
    assert_eq!(ws.translucency(), Translucency::default());
}

#[test]
fn 保存した値で復元し保存する内容にも出る() {
    let t = Translucency {
        window: 70,
        terminal: 0,
    };
    let ws = restore(PersistedState {
        layout: PersistedLayout {
            translucency: t,
            ..PersistedLayout::default()
        },
        ..PersistedState::default()
    });

    assert_eq!(ws.translucency(), t);
    assert_eq!(ws.snapshot().translucency, TranslucencySnapshot::from(t));
    assert_eq!(ws.persisted_state().layout.translucency, t);
}

#[test]
fn 範囲の外や_5_の倍数でない保存値は復元のときも直す() {
    let ws = restore(PersistedState {
        layout: PersistedLayout {
            translucency: Translucency {
                window: 200,
                terminal: 13,
            },
            ..PersistedLayout::default()
        },
        ..PersistedState::default()
    });

    assert_eq!(
        ws.translucency(),
        Translucency {
            window: 100,
            terminal: 15,
        }
    );
}
