//! 画面へ渡す状態の写し `AppSnapshot` の JSON の形を確かめる。
//!
//! 画面(TypeScript)は camelCase のキーで受け取る。キーの名前が変わると画面が壊れるので、
//! 形そのものを固定して見る。

use arin_core::workspace::{AppSnapshot, SNAPSHOT_SCHEMA_VERSION};
use serde_json::json;

#[test]
fn 写しはスキーマの版とアプリの版だけを_camel_case_のキーで持つ() {
    let snapshot = AppSnapshot::new("1.2.3");

    let value = serde_json::to_value(&snapshot).expect("JSON にできなかった");

    assert_eq!(
        value,
        json!({ "schemaVersion": SNAPSHOT_SCHEMA_VERSION, "appVersion": "1.2.3" })
    );
}

#[test]
fn 写しのスキーマの版は_1_から始まる() {
    assert_eq!(SNAPSHOT_SCHEMA_VERSION, 1);
    assert_eq!(AppSnapshot::new("0.1.0").schema_version, 1);
}

#[test]
fn json_から読み戻すと同じ写しになる() {
    let snapshot = AppSnapshot::new("0.1.0");

    let text = serde_json::to_string(&snapshot).expect("JSON にできなかった");
    let back: AppSnapshot = serde_json::from_str(&text).expect("JSON から読めなかった");

    assert_eq!(back, snapshot);
}
