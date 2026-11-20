//! `Workspace` のテーマの選び方の口(ADR-016)。持ち方・写し・保存する内容への出し方と、
//! 変わったときだけ真を返すこと(呼び出し側が写しを送り、保存が予約される)を見る。

use std::path::PathBuf;

use arin_core::layout::font_sizes::FontSizeTarget;
use arin_core::layout::theme::ThemeMode;
use arin_core::persistence::codec::PersistedState;
use arin_core::workspace::{Startup, Workspace};

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

#[test]
fn 既定はシステムで写しと保存する内容に載る() {
    let ws = restore(PersistedState::default());

    assert_eq!(ws.theme(), ThemeMode::System);
    assert_eq!(ws.snapshot().theme, ThemeMode::System);
    assert_eq!(ws.persisted_state().layout.theme, ThemeMode::System);
}

#[test]
fn set_theme_で写しと保存する内容が替わり_同じ値は変わらない() {
    let mut ws = restore(PersistedState::default());

    assert!(ws.set_theme(ThemeMode::Dark), "システム → ダークは変わる");
    assert_eq!(ws.theme(), ThemeMode::Dark);
    assert_eq!(ws.snapshot().theme, ThemeMode::Dark);
    assert_eq!(ws.persisted_state().layout.theme, ThemeMode::Dark);
    assert!(!ws.set_theme(ThemeMode::Dark), "同じ値は変わらない");

    assert!(ws.set_theme(ThemeMode::Light));
    assert_eq!(ws.snapshot().theme, ThemeMode::Light);
    assert!(ws.set_theme(ThemeMode::System));
    assert_eq!(ws.persisted_state().layout.theme, ThemeMode::System);
}

#[test]
fn 保存値のテーマを起動時に持ち直す() {
    let mut state = PersistedState::default();
    state.layout.theme = ThemeMode::Light;

    let ws = restore(state);

    assert_eq!(ws.theme(), ThemeMode::Light);
    assert_eq!(ws.snapshot().theme, ThemeMode::Light);
}

#[test]
fn 既定に戻すでテーマは変わらない() {
    let mut ws = restore(PersistedState::default());
    ws.set_theme(ThemeMode::Dark);
    ws.step_font_size(FontSizeTarget::Terminal, 2);

    assert!(ws.reset_appearance());

    assert_eq!(ws.theme(), ThemeMode::Dark);
    assert!(
        !ws.reset_appearance(),
        "テーマだけが既定と違っても、戻すものは無い"
    );
}

#[test]
fn 写しの_theme_は小文字の文字列になる() {
    let mut ws = restore(PersistedState::default());
    ws.set_theme(ThemeMode::Dark);
    let value = serde_json::to_value(ws.snapshot()).unwrap();
    assert_eq!(value["theme"], serde_json::json!("dark"));
}
