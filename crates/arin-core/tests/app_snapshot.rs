//! 画面へ渡す状態の写し `AppSnapshot` の JSON の形を確かめる(計画書 T32b の「写しの形」)。
//!
//! 画面(TypeScript)は camelCase のキーで受け取る。キーの名前が変わると画面が壊れるので、
//! 形そのものを固定して見る。絶対パスは写しに載せない(設計書 §12)。

use std::path::PathBuf;

use arin_core::persistence::codec::PersistedState;
use arin_core::project::{Project, ProjectColor, ProjectId, RootReference};
use arin_core::workspace::{
    AppSnapshot, RootResolution, SNAPSHOT_SCHEMA_VERSION, Startup, Workspace,
};
use serde_json::json;

const HOME: &str = "/Users/me";

fn id(n: u8) -> ProjectId {
    ProjectId::from_random_bytes([n; 16])
}

/// A(見つかる)と B(見つからない)を登録し、A を選んだ状態。
fn sample() -> Workspace {
    let a = RootReference::new(PathBuf::from("/Users/me/dev/a"), Some("YQ==".to_owned()));
    let b = RootReference::new(PathBuf::from("/Volumes/ext/b"), Some("Yg==".to_owned()));
    Workspace::restore(
        "1.2.3",
        PathBuf::from(HOME),
        [("HOME", HOME)],
        Startup {
            state: PersistedState {
                projects: vec![
                    Project::restore(id(1), a.clone(), None, ProjectColor::Green),
                    Project::restore(id(2), b, None, ProjectColor::Blue),
                ],
                selected_project_id: Some(id(1)),
                ..PersistedState::default()
            },
            roots: vec![
                (id(1), RootResolution::Found(a)),
                (id(2), RootResolution::Missing),
            ],
            notice: Some("退避した".to_owned()),
        },
    )
}

#[test]
fn 写しの_json_の形() {
    let value = serde_json::to_value(sample().snapshot()).expect("JSON にできなかった");

    assert_eq!(
        value,
        json!({
            "schemaVersion": SNAPSHOT_SCHEMA_VERSION,
            "appVersion": "1.2.3",
            "projects": [
                {
                    "id": id(1).as_str(),
                    "name": "a",
                    "color": "green",
                    "displayPath": "~/dev/a",
                    "missing": false,
                    "showHidden": false,
                    "terminalIds": ["terminal-1"],
                    "activeTerminalId": "terminal-1",
                },
                {
                    "id": id(2).as_str(),
                    "name": "b",
                    "color": "blue",
                    "displayPath": "/Volumes/ext/b",
                    "missing": true,
                    "showHidden": false,
                    "terminalIds": [],
                    "activeTerminalId": null,
                },
            ],
            "selectedProjectId": id(1).as_str(),
            "terminals": [
                {
                    "id": "terminal-1",
                    "projectId": id(1).as_str(),
                    "label": "ターミナル 1",
                    "status": { "kind": "notStarted" },
                },
            ],
            "layout": { "sidebarWidth": 260.0, "sidebarCollapsed": false, "cardAreaHeight": 200.0 },
            "notice": "退避した",
        })
    );
}

#[test]
fn 写しの_json_に_show_hidden_が載る() {
    // 計画書 T38e。隠しファイル表示(FR-017)はカードごとの真偽で、既定はオフ
    let mut ws = sample();
    ws.set_project_show_hidden(&id(2), true)
        .expect("変えられる");

    let value = serde_json::to_value(ws.snapshot()).unwrap();

    assert_eq!(value["projects"][0]["showHidden"], json!(false));
    assert_eq!(value["projects"][1]["showHidden"], json!(true));
}

#[test]
fn 写しに絶対パスとブックマークを載せない() {
    let text = serde_json::to_string(&sample().snapshot()).unwrap();

    // ホームの中のパスは `~` で始まる表示用のパスだけ。ホームの外は表示用のパス(=絶対パス)を出すが、
    // コピー用の `rootPath` という項目は持たない(コピーは Rust が行う。設計書 §12)。
    assert!(!text.contains("/Users/me/dev/a"), "{text}");
    assert!(!text.contains("rootPath"), "{text}");
    assert!(!text.contains("YQ=="), "{text}");
    assert!(!text.contains("bookmark"), "{text}");
}

#[test]
fn 検証画面の写しのターミナルはプロジェクトを持たない() {
    let ws = Workspace::for_verify_grid("1.2.3", PathBuf::from(HOME), [("HOME", HOME)]);

    let value = serde_json::to_value(ws.snapshot()).unwrap();

    assert_eq!(value["projects"], json!([]));
    assert_eq!(value["selectedProjectId"], json!(null));
    assert_eq!(value["notice"], json!(null));
    assert_eq!(
        value["terminals"][0],
        json!({
            "id": "terminal-1",
            "projectId": null,
            "label": "ターミナル 1",
            "status": { "kind": "notStarted" },
        })
    );
}

#[test]
fn 写しのスキーマの版は_1_のまま() {
    // 項目を足しただけで意味は変えていないので、版は上げない。
    assert_eq!(SNAPSHOT_SCHEMA_VERSION, 1);
    assert_eq!(sample().snapshot().schema_version, 1);
}

#[test]
fn json_から読み戻すと同じ写しになる() {
    let snapshot = sample().snapshot();

    let text = serde_json::to_string(&snapshot).expect("JSON にできなかった");
    let back: AppSnapshot = serde_json::from_str(&text).expect("JSON から読めなかった");

    assert_eq!(back, snapshot);
}
