//! 文字の大きさの規則(ADR-013 の 1・2、計画書 T68a。AC-F06・F08・F09 の規則の部分)。
//!
//! 単位は CSS ピクセルの整数。範囲はターミナル 9〜28・画面 11〜18、1 刻み(【要確認】1 の推奨)。
//! 既定はターミナル 15(xterm.js 6.0.0 の既定)・画面 13(`tokens.css` の `--font-size-l`)。

use arin_core::layout::font_sizes::{
    DEFAULT_INTERFACE_FONT_SIZE, DEFAULT_TERMINAL_FONT_SIZE, FontSizeTarget, FontSizes,
    MAX_INTERFACE_FONT_SIZE, MAX_TERMINAL_FONT_SIZE, MIN_INTERFACE_FONT_SIZE,
    MIN_TERMINAL_FONT_SIZE,
};
use serde_json::{Value, json};

#[test]
fn 既定はターミナル_15_画面_13() {
    let sizes = FontSizes::default();

    assert_eq!(sizes.terminal, 15);
    assert_eq!(sizes.interface, 13);
    assert_eq!(DEFAULT_TERMINAL_FONT_SIZE, 15);
    assert_eq!(DEFAULT_INTERFACE_FONT_SIZE, 13);
    assert_eq!((MIN_TERMINAL_FONT_SIZE, MAX_TERMINAL_FONT_SIZE), (9, 28));
    assert_eq!((MIN_INTERFACE_FONT_SIZE, MAX_INTERFACE_FONT_SIZE), (11, 18));
    assert!(sizes.is_default());
    assert_eq!(FontSizeTarget::Terminal.default_value(), 15);
    assert_eq!(FontSizeTarget::Interface.default_value(), 13);
    assert_eq!(FontSizeTarget::Terminal.min(), 9);
    assert_eq!(FontSizeTarget::Terminal.max(), 28);
    assert_eq!(FontSizeTarget::Interface.min(), 11);
    assert_eq!(FontSizeTarget::Interface.max(), 18);
}

#[test]
fn 刻みは範囲の端で留まる() {
    use FontSizeTarget::{Interface, Terminal};
    // (対象, 始めの値, 刻み, 結果)
    let cases: [(FontSizeTarget, u16, i32, u16); 14] = [
        (Terminal, 15, 1, 16),
        (Terminal, 15, -1, 14),
        (Terminal, 15, 3, 18),
        (Terminal, 27, 1, 28),
        (Terminal, 28, 1, 28),
        (Terminal, 9, -1, 9),
        (Terminal, 10, -5, 9),
        (Terminal, 15, i32::MAX, 28),
        (Terminal, 15, i32::MIN, 9),
        (Terminal, 15, 0, 15),
        (Interface, 13, 1, 14),
        (Interface, 18, 1, 18),
        (Interface, 11, -1, 11),
        (Interface, 13, 100, 18),
    ];
    for (target, start, delta, expected) in cases {
        let sizes = FontSizes::default().with(target, start);

        let next = sizes.stepped(target, delta);

        assert_eq!(
            next.get(target),
            expected,
            "{target:?} を {start} から {delta} 刻む"
        );
        // もう一方は変わらない
        let other = match target {
            Terminal => Interface,
            Interface => Terminal,
        };
        assert_eq!(
            next.get(other),
            sizes.get(other),
            "{target:?} の刻みで {other:?} が変わった"
        );
    }
}

#[test]
fn 既定に戻すと両方が既定() {
    let sizes = FontSizes::default()
        .stepped(FontSizeTarget::Terminal, 4)
        .stepped(FontSizeTarget::Interface, -2);
    assert!(!sizes.is_default());

    let reset = sizes.reset();

    assert_eq!(reset, FontSizes::default());
    assert!(reset.is_default());
}

#[test]
fn 保存値が無い_null_数でないときは既定() {
    let not_numbers: [Option<Value>; 7] = [
        None,
        Some(Value::Null),
        Some(json!("16")),
        Some(json!(true)),
        Some(json!([16])),
        Some(json!({ "value": 16 })),
        Some(json!("")),
    ];
    for raw in &not_numbers {
        assert_eq!(
            FontSizeTarget::Terminal.from_persisted(raw.as_ref()),
            15,
            "ターミナル: {raw:?}"
        );
        assert_eq!(
            FontSizeTarget::Interface.from_persisted(raw.as_ref()),
            13,
            "画面: {raw:?}"
        );
    }
}

#[test]
fn 整数でない数は四捨五入する() {
    let cases = [
        (json!(16.4), 16),
        (json!(16.5), 17),
        (json!(16.6), 17),
        (json!(9.4), 9),
        (json!(20), 20),
    ];
    for (raw, expected) in cases {
        assert_eq!(
            FontSizeTarget::Terminal.from_persisted(Some(&raw)),
            expected,
            "{raw}"
        );
    }
    assert_eq!(
        FontSizeTarget::Interface.from_persisted(Some(&json!(14.5))),
        15
    );
}

#[test]
fn 範囲の外は端に留める() {
    use FontSizeTarget::{Interface, Terminal};
    let cases = [
        (Terminal, json!(8), 9),
        (Terminal, json!(0), 9),
        (Terminal, json!(-3), 9),
        (Terminal, json!(29), 28),
        (Terminal, json!(1_000_000), 28),
        (Terminal, json!(1e300), 28),
        (Terminal, json!(u64::MAX), 28),
        (Terminal, json!(i64::MIN), 9),
        // 四捨五入してから留める(8.5 → 9、28.4 → 28)
        (Terminal, json!(8.5), 9),
        (Terminal, json!(28.4), 28),
        (Interface, json!(10), 11),
        (Interface, json!(19), 18),
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
fn 対象の名前は_terminal_と_interface() {
    // コマンドの引数(`step_font_size({ target, delta })`)で画面から届く形
    assert_eq!(
        serde_json::to_value(FontSizeTarget::Terminal).unwrap(),
        json!("terminal")
    );
    assert_eq!(
        serde_json::to_value(FontSizeTarget::Interface).unwrap(),
        json!("interface")
    );
    let back: FontSizeTarget = serde_json::from_value(json!("interface")).unwrap();
    assert_eq!(back, FontSizeTarget::Interface);
}
