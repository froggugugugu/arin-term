//! 透過の規則(ADR-014 の 1。AC-G01・G04 の規則の部分)。
//!
//! 値は「透過の度合い(%)」の整数。ウィンドウは既定 45・範囲 0〜100、ターミナルの背景は既定 20・範囲 0〜80。刻みは 5。
//! 5 の倍数でない値は近い 5 の倍数に丸め、範囲の外は端に留める。

use arin_core::layout::translucency::{
    DEFAULT_TERMINAL_TRANSLUCENCY, DEFAULT_WINDOW_TRANSLUCENCY, MAX_TERMINAL_TRANSLUCENCY,
    MAX_WINDOW_TRANSLUCENCY, MIN_TERMINAL_TRANSLUCENCY, MIN_WINDOW_TRANSLUCENCY, TRANSLUCENCY_STEP,
    Translucency, TranslucencyTarget,
};
use serde_json::{Value, json};

#[test]
fn 既定はウィンドウ_45_ターミナル_20() {
    let t = Translucency::default();

    assert_eq!(t.window, 45);
    assert_eq!(t.terminal, 20);
    assert_eq!(DEFAULT_WINDOW_TRANSLUCENCY, 45);
    assert_eq!(DEFAULT_TERMINAL_TRANSLUCENCY, 20);
    assert_eq!((MIN_WINDOW_TRANSLUCENCY, MAX_WINDOW_TRANSLUCENCY), (0, 100));
    assert_eq!(
        (MIN_TERMINAL_TRANSLUCENCY, MAX_TERMINAL_TRANSLUCENCY),
        (0, 80)
    );
    assert_eq!(TRANSLUCENCY_STEP, 5);
    assert!(t.is_default());
    for (target, min, max, default) in [
        (TranslucencyTarget::Window, 0, 100, 45),
        (TranslucencyTarget::Terminal, 0, 80, 20),
    ] {
        assert_eq!(target.min(), min, "{target:?}");
        assert_eq!(target.max(), max, "{target:?}");
        assert_eq!(target.default_value(), default, "{target:?}");
        assert_eq!(target.step(), 5, "{target:?}");
    }
}

#[test]
fn 刻みは_5_の倍数に丸めて端で留まる() {
    use TranslucencyTarget::{Terminal, Window};
    // (対象, 始めの値, 差分, 結果)
    let cases: [(TranslucencyTarget, u8, i32, u8); 16] = [
        (Window, 45, 5, 50),
        (Window, 45, -5, 40),
        (Window, 95, 5, 100),
        (Window, 100, 5, 100),
        (Window, 0, -5, 0),
        (Window, 45, 100, 100),
        (Window, 45, -100, 0),
        (Window, 45, i32::MAX, 100),
        (Window, 45, i32::MIN, 0),
        (Window, 45, 0, 45),
        // 差分が 5 の倍数でなくても、結果は近い 5 の倍数
        (Window, 45, 2, 45),
        (Window, 45, 3, 50),
        (Window, 45, -3, 40),
        (Terminal, 20, 5, 25),
        (Terminal, 75, 10, 80),
        (Terminal, 20, 80, 80),
    ];
    for (target, start, delta, expected) in cases {
        let t = Translucency::default().with(target, start);

        let next = t.stepped(target, delta);

        assert_eq!(
            next.get(target),
            expected,
            "{target:?} を {start} から {delta} 動かす"
        );
        let other = match target {
            Window => Terminal,
            Terminal => Window,
        };
        assert_eq!(
            next.get(other),
            t.get(other),
            "{target:?} で {other:?} が変わった"
        );
    }
}

#[test]
fn with_も丸めて端で留める() {
    let t = Translucency::default()
        .with(TranslucencyTarget::Window, 200)
        .with(TranslucencyTarget::Terminal, 83);
    assert_eq!(t.window, 100);
    assert_eq!(t.terminal, 80);

    let t = Translucency::default()
        .with(TranslucencyTarget::Window, 12)
        .with(TranslucencyTarget::Terminal, 13);
    assert_eq!(t.window, 10);
    assert_eq!(t.terminal, 15);
}

#[test]
fn 既定に戻すと両方が既定() {
    let t = Translucency::default()
        .stepped(TranslucencyTarget::Window, 10)
        .stepped(TranslucencyTarget::Terminal, -20);
    assert!(!t.is_default());

    let reset = t.reset();

    assert_eq!(reset, Translucency::default());
    assert!(reset.is_default());
}

#[test]
fn 保存値が無い_null_数でないときは既定() {
    let not_numbers: [Option<Value>; 7] = [
        None,
        Some(Value::Null),
        Some(json!("50")),
        Some(json!(true)),
        Some(json!([50])),
        Some(json!({ "value": 50 })),
        Some(json!("")),
    ];
    for raw in &not_numbers {
        assert_eq!(
            TranslucencyTarget::Window.from_persisted(raw.as_ref()),
            45,
            "ウィンドウ: {raw:?}"
        );
        assert_eq!(
            TranslucencyTarget::Terminal.from_persisted(raw.as_ref()),
            20,
            "ターミナル: {raw:?}"
        );
    }
}

#[test]
fn 保存値は近い_5_の倍数に丸めて端に留める() {
    use TranslucencyTarget::{Terminal, Window};
    let cases = [
        (Window, json!(50), 50),
        (Window, json!(52), 50),
        (Window, json!(53), 55),
        (Window, json!(52.4), 50),
        // ちょうど中間は大きいほう
        (Window, json!(52.5), 55),
        (Window, json!(0), 0),
        (Window, json!(-3), 0),
        (Window, json!(-30), 0),
        (Window, json!(101), 100),
        (Window, json!(1e300), 100),
        (Window, json!(u64::MAX), 100),
        (Window, json!(i64::MIN), 0),
        (Terminal, json!(81), 80),
        (Terminal, json!(83), 80),
        (Terminal, json!(100), 80),
        (Terminal, json!(17), 15),
        (Terminal, json!(18), 20),
    ];
    for (target, raw, expected) in cases {
        assert_eq!(
            target.from_persisted(Some(&raw)),
            expected,
            "{target:?}: {raw}"
        );
    }
}

#[test]
fn 対象の名前は_window_と_terminal() {
    // コマンドの引数(`step_translucency({ target, delta })`)で画面から届く形
    assert_eq!(
        serde_json::to_value(TranslucencyTarget::Window).unwrap(),
        json!("window")
    );
    assert_eq!(
        serde_json::to_value(TranslucencyTarget::Terminal).unwrap(),
        json!("terminal")
    );
    let back: TranslucencyTarget = serde_json::from_value(json!("terminal")).unwrap();
    assert_eq!(back, TranslucencyTarget::Terminal);
}
