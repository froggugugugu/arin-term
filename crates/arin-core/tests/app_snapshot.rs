//! 画面へ渡す状態の写し `AppSnapshot` の JSON の形を確かめる。
//!
//! 画面(TypeScript)は camelCase のキーで受け取る。キーの名前が変わると画面が壊れるので、
//! 形そのものを固定して見る。

use arin_core::terminal::{TerminalId, TerminalSnapshot, TerminalStatus};
use arin_core::workspace::{AppSnapshot, SNAPSHOT_SCHEMA_VERSION};
use serde_json::json;

fn sample() -> AppSnapshot {
    AppSnapshot::new(
        "1.2.3",
        vec![TerminalSnapshot {
            id: TerminalId::new("terminal-1"),
            status: TerminalStatus::Running,
        }],
    )
}

#[test]
fn 写しはスキーマの版とアプリの版とターミナルの一覧を_camel_case_のキーで持つ() {
    let value = serde_json::to_value(sample()).expect("JSON にできなかった");

    assert_eq!(
        value,
        json!({
            "schemaVersion": SNAPSHOT_SCHEMA_VERSION,
            "appVersion": "1.2.3",
            "terminals": [{ "id": "terminal-1", "status": { "kind": "running" } }],
        })
    );
}

#[test]
fn 写しのスキーマの版は_1_から始まる() {
    assert_eq!(SNAPSHOT_SCHEMA_VERSION, 1);
    assert_eq!(AppSnapshot::new("0.1.0", Vec::new()).schema_version, 1);
}

#[test]
fn json_から読み戻すと同じ写しになる() {
    let snapshot = sample();

    let text = serde_json::to_string(&snapshot).expect("JSON にできなかった");
    let back: AppSnapshot = serde_json::from_str(&text).expect("JSON から読めなかった");

    assert_eq!(back, snapshot);
}
