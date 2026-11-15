//! `state.json` 版 1 の読み書き(計画書 T30a、設計書 §6.2、決定 (d)、【仮定】2・3・13)。
//!
//! 入出力はしない。読んだバイト列を渡すと「状態」「無い」「退避する(理由つき)」のどれかが返る。

use std::path::PathBuf;

use arin_core::persistence::codec::{
    EncodeError, LoadOutcome, LoadedState, PersistedLayout, PersistedState, QuarantineReason,
    STATE_FILE_NAME, STATE_SCHEMA_VERSION, broken_file_name, decode, encode,
};
use arin_core::project::{Project, ProjectColor, ProjectId, RootReference};
use serde_json::{Value, json};

const ID_A: &str = "0123abcd-89ab-4def-8123-456789abcdef";
const ID_B: &str = "11111111-2222-4333-8444-555555555555";
const ID_C: &str = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";

fn id(text: &str) -> ProjectId {
    ProjectId::parse(text).expect("ID の形")
}

fn project(
    id_text: &str,
    path: &str,
    bookmark: Option<&str>,
    custom_name: Option<&str>,
    color: ProjectColor,
) -> Project {
    Project::restore(
        id(id_text),
        RootReference::new(PathBuf::from(path), bookmark.map(str::to_owned)),
        custom_name.map(str::to_owned),
        color,
    )
}

fn sample_state() -> PersistedState {
    PersistedState {
        projects: vec![
            project(
                ID_A,
                "/Users/me/dev/app",
                Some("Ym9va21hcmstYQ=="),
                None,
                ProjectColor::Red,
            ),
            project(
                ID_B,
                "/Users/me/dev/app/frontend",
                Some("Ym9va21hcmstYg=="),
                Some("フロント"),
                ProjectColor::Teal,
            ),
            project(ID_C, "/Users/me/notes", None, None, ProjectColor::Pink),
        ],
        selected_project_id: Some(id(ID_B)),
        layout: PersistedLayout {
            sidebar_width: Some(312.5),
            sidebar_collapsed: true,
            card_area_height: Some(240.0),
        },
    }
}

fn decode_str(text: &str) -> LoadOutcome {
    decode(Some(text.as_bytes()))
}

fn decode_value(value: &Value) -> LoadOutcome {
    decode_str(&value.to_string())
}

fn loaded(outcome: LoadOutcome) -> LoadedState {
    match outcome {
        LoadOutcome::Loaded(loaded) => loaded,
        other => panic!("状態として読めるはず: {other:?}"),
    }
}

fn quarantine(outcome: LoadOutcome) -> QuarantineReason {
    match outcome {
        LoadOutcome::Quarantine(reason) => reason,
        other => panic!("退避の指示になるはず: {other:?}"),
    }
}

fn minimal_project(id_text: &str, path: &str) -> Value {
    json!({ "id": id_text, "bookmark": "Ym0=", "lastKnownPath": path, "colorId": "green" })
}

// ---- 名前と版 ----

#[test]
fn ファイル名は_state_json_で版は_1() {
    assert_eq!(STATE_FILE_NAME, "state.json");
    assert_eq!(STATE_SCHEMA_VERSION, 1);
}

// ---- 書いて読む ----

#[test]
fn 書いて読むと同じ状態に戻る() {
    let state = sample_state();

    let text = encode(&state).expect("書ける");
    let back = loaded(decode_str(&text));

    assert_eq!(back.state, state);
    // ブックマークの無いプロジェクトは作り直しの指示に載る
    assert_eq!(back.rebuild_bookmarks, vec![id(ID_C)]);
}

#[test]
fn 書いた_json_の形() {
    let text = encode(&sample_state()).expect("書ける");
    assert!(text.ends_with('\n'), "末尾は改行");
    let value: Value = serde_json::from_str(&text).expect("JSON として読める");

    assert_eq!(
        value,
        json!({
            "schemaVersion": 1,
            "projects": [
                {
                    "id": ID_A,
                    "bookmark": "Ym9va21hcmstYQ==",
                    "lastKnownPath": "/Users/me/dev/app",
                    "customName": null,
                    "colorId": "red",
                },
                {
                    "id": ID_B,
                    "bookmark": "Ym9va21hcmstYg==",
                    "lastKnownPath": "/Users/me/dev/app/frontend",
                    "customName": "フロント",
                    "colorId": "teal",
                },
                {
                    "id": ID_C,
                    "bookmark": null,
                    "lastKnownPath": "/Users/me/notes",
                    "customName": null,
                    "colorId": "pink",
                },
            ],
            "ui": {
                "selectedProjectId": ID_B,
                "sidebarWidth": 312.5,
                "sidebarCollapsed": true,
                "cardAreaHeight": 240.0,
            },
        })
    );
}

#[test]
fn 空の状態も書いて読める() {
    let state = PersistedState::default();
    let text = encode(&state).expect("書ける");

    let value: Value = serde_json::from_str(&text).expect("JSON として読める");
    // 寸法が無いときはキーを書かない(読むときに既定値で補う)
    assert_eq!(
        value,
        json!({
            "schemaVersion": 1,
            "projects": [],
            "ui": { "selectedProjectId": null, "sidebarCollapsed": false },
        })
    );

    let back = loaded(decode_str(&text));
    assert_eq!(back.state, state);
    assert!(back.rebuild_bookmarks.is_empty());
}

#[test]
fn utf_8_でないパスは書けない() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let state = PersistedState {
        projects: vec![Project::restore(
            id(ID_A),
            RootReference::new(PathBuf::from(OsStr::from_bytes(b"/Users/me/\xff")), None),
            None,
            ProjectColor::Red,
        )],
        ..PersistedState::default()
    };

    let error = encode(&state).expect_err("書けない");
    assert!(matches!(error, EncodeError::Serialize(_)));
    assert!(error.to_string().contains("state.json"));
}

#[test]
fn ブックマークの文字列の往復() {
    // `arin-core` は中身を解釈しない。どんな文字列もそのまま戻る
    let odd = "not base64 !? \u{1F600} \"quoted\"";
    let state = PersistedState {
        projects: vec![project(ID_A, "/a", Some(odd), None, ProjectColor::Blue)],
        ..PersistedState::default()
    };

    let back = loaded(decode_str(&encode(&state).expect("書ける")));

    assert_eq!(back.state.projects[0].root().bookmark(), Some(odd));
    assert!(back.rebuild_bookmarks.is_empty());
}

// ---- 欠けたキー・未知のキー ----

#[test]
fn 欠けたキーは既定値で補う() {
    // projects も ui も無い
    let empty = loaded(decode_value(&json!({ "schemaVersion": 1 })));
    assert_eq!(empty.state, PersistedState::default());

    // プロジェクトの bookmark と customName が無く、ui の一部だけがある
    let partial = loaded(decode_value(&json!({
        "schemaVersion": 1,
        "projects": [{ "id": ID_A, "lastKnownPath": "/a", "colorId": "yellow" }],
        "ui": { "sidebarWidth": 200 },
    })));
    assert_eq!(
        partial.state,
        PersistedState {
            projects: vec![project(ID_A, "/a", None, None, ProjectColor::Yellow)],
            selected_project_id: None,
            layout: PersistedLayout {
                sidebar_width: Some(200.0),
                sidebar_collapsed: false,
                card_area_height: None,
            },
        }
    );
    assert_eq!(partial.rebuild_bookmarks, vec![id(ID_A)]);

    // null は無いのと同じ(寸法と選択)
    let nulls = loaded(decode_value(&json!({
        "schemaVersion": 1,
        "projects": [],
        "ui": { "selectedProjectId": null, "sidebarWidth": null, "cardAreaHeight": null },
    })));
    assert_eq!(nulls.state.layout, PersistedLayout::default());
}

#[test]
fn 寸法は数として読むだけで範囲を検査しない() {
    // 範囲の検査は T30b の規則で行う
    let outcome = loaded(decode_value(&json!({
        "schemaVersion": 1,
        "ui": { "sidebarWidth": -5, "cardAreaHeight": 1.0e9 },
    })));
    assert_eq!(outcome.state.layout.sidebar_width, Some(-5.0));
    assert_eq!(outcome.state.layout.card_area_height, Some(1.0e9));
}

#[test]
fn 未知のキーは無視する() {
    let outcome = loaded(decode_value(&json!({
        "schemaVersion": 1,
        "futureTopLevel": { "x": 1 },
        "projects": [{
            "id": ID_A,
            "bookmark": "Ym0=",
            "lastKnownPath": "/a",
            "colorId": "green",
            "tree": { "expanded": ["src"], "showHidden": true },
        }],
        "ui": { "displayMode": "tiles", "selectedProjectId": ID_A },
    })));

    assert_eq!(
        outcome.state.projects,
        vec![project(ID_A, "/a", Some("Ym0="), None, ProjectColor::Green)]
    );
    assert_eq!(outcome.state.selected_project_id, Some(id(ID_A)));
}

// ---- 無い・退避 ----

#[test]
fn ファイルが無ければ空で始める() {
    assert_eq!(decode(None), LoadOutcome::Absent);
}

#[test]
fn 壊れた_json_は退避の指示になる() {
    for broken in ["", "{", "{\"schemaVersion\": 1,", "not json", "{} trailing"] {
        let reason = quarantine(decode_str(broken));
        assert!(
            matches!(reason, QuarantineReason::Malformed(_)),
            "{broken:?}: {reason:?}"
        );
    }
    // UTF-8 でないバイト列
    let reason = quarantine(decode(Some(b"{\"schemaVersion\": 1, \"x\": \"\xff\"}")));
    assert!(
        matches!(reason, QuarantineReason::Malformed(_)),
        "{reason:?}"
    );
}

#[test]
fn 最上位がオブジェクトでなければ退避の指示になる() {
    for text in ["[]", "null", "1", "\"state\""] {
        let reason = quarantine(decode_str(text));
        assert!(
            matches!(reason, QuarantineReason::NotAnObject),
            "{text:?}: {reason:?}"
        );
    }
}

#[test]
fn 版が新しすぎると退避の指示になる() {
    let reason = quarantine(decode_value(&json!({ "schemaVersion": 2, "projects": [] })));
    assert_eq!(reason, QuarantineReason::TooNew(2));

    let huge = quarantine(decode_str("{\"schemaVersion\": 18446744073709551615}"));
    assert_eq!(huge, QuarantineReason::TooNew(u64::MAX));
}

#[test]
fn 版が数でない_0_負のときも退避の指示になる() {
    for version in [
        json!("1"),
        json!(0),
        json!(-1),
        json!(1.5),
        json!(1.0),
        json!(null),
        json!(true),
    ] {
        let reason = quarantine(decode_value(&json!({ "schemaVersion": version })));
        assert_eq!(
            reason,
            QuarantineReason::InvalidVersion(version.to_string()),
            "{version}"
        );
    }
    // 版のキーが無い
    let missing = quarantine(decode_value(&json!({ "projects": [] })));
    assert_eq!(missing, QuarantineReason::MissingVersion);
}

#[test]
fn 中身の形が違えば退避の指示になる() {
    for value in [
        json!({ "schemaVersion": 1, "projects": {} }),
        json!({ "schemaVersion": 1, "projects": null }),
        json!({ "schemaVersion": 1, "projects": [{ "id": "terminal-1", "lastKnownPath": "/a", "colorId": "red" }] }),
        json!({ "schemaVersion": 1, "projects": [{ "id": ID_A, "colorId": "red" }] }),
        json!({ "schemaVersion": 1, "projects": [{ "id": ID_A, "lastKnownPath": "/a" }] }),
        json!({ "schemaVersion": 1, "projects": [{ "id": ID_A, "lastKnownPath": "/a", "colorId": "gold" }] }),
        json!({ "schemaVersion": 1, "projects": [{ "id": ID_A, "lastKnownPath": "/a", "colorId": "red", "bookmark": 12 }] }),
        json!({ "schemaVersion": 1, "ui": "tabs" }),
        json!({ "schemaVersion": 1, "ui": { "sidebarWidth": "260px" } }),
        json!({ "schemaVersion": 1, "ui": { "sidebarCollapsed": "yes" } }),
        json!({ "schemaVersion": 1, "ui": { "selectedProjectId": "terminal-1" } }),
    ] {
        let reason = quarantine(decode_value(&value));
        assert!(
            matches!(reason, QuarantineReason::InvalidContent(_)),
            "{value}: {reason:?}"
        );
    }
}

#[test]
fn 退避の理由は文言を持つ() {
    let messages = [
        QuarantineReason::Malformed("x".into()).to_string(),
        QuarantineReason::NotAnObject.to_string(),
        QuarantineReason::MissingVersion.to_string(),
        QuarantineReason::InvalidVersion("0".into()).to_string(),
        QuarantineReason::TooNew(3).to_string(),
        QuarantineReason::InvalidContent("y".into()).to_string(),
    ];
    for message in &messages {
        assert!(message.contains("state.json"), "{message}");
    }
    assert!(messages[4].contains('3') && messages[4].contains('1'));
}

// ---- ブックマークの作り直し ----

#[test]
fn bookmark_が_null_なら_last_known_path_から作り直す指示() {
    let outcome = loaded(decode_value(&json!({
        "schemaVersion": 1,
        "projects": [
            { "id": ID_A, "bookmark": null, "lastKnownPath": "/a", "colorId": "red" },
            { "id": ID_B, "bookmark": "Ym0=", "lastKnownPath": "/b", "colorId": "orange" },
            { "id": ID_C, "lastKnownPath": "/c", "colorId": "yellow" },
        ],
    })));

    // 並び順のとおり。ブックマークのあるものは載らない
    assert_eq!(outcome.rebuild_bookmarks, vec![id(ID_A), id(ID_C)]);
    // 作り直すパスは状態の lastKnownPath
    assert_eq!(
        outcome.state.projects[0].root().last_known_path(),
        PathBuf::from("/a")
    );
    assert_eq!(outcome.state.projects[0].root().bookmark(), None);
}

// ---- 一覧の整え ----

#[test]
fn 選択中の_id_が一覧に無ければ_null_にする() {
    let outcome = loaded(decode_value(&json!({
        "schemaVersion": 1,
        "projects": [minimal_project(ID_A, "/a")],
        "ui": { "selectedProjectId": ID_B },
    })));
    assert_eq!(outcome.state.selected_project_id, None);

    let none = loaded(decode_value(&json!({
        "schemaVersion": 1,
        "ui": { "selectedProjectId": ID_A },
    })));
    assert_eq!(none.state.selected_project_id, None);
}

#[test]
fn 同じ_id_が_2_回出たら後のものを捨てる() {
    let outcome = loaded(decode_value(&json!({
        "schemaVersion": 1,
        "projects": [
            minimal_project(ID_A, "/first"),
            minimal_project(ID_B, "/b"),
            { "id": ID_A, "bookmark": null, "lastKnownPath": "/second", "colorId": "red" },
        ],
        "ui": { "selectedProjectId": ID_A },
    })));

    let paths: Vec<_> = outcome
        .state
        .projects
        .iter()
        .map(|project| project.path().to_path_buf())
        .collect();
    assert_eq!(paths, [PathBuf::from("/first"), PathBuf::from("/b")]);
    // 捨てた側のブックマークの指示は出さない
    assert!(outcome.rebuild_bookmarks.is_empty());
    assert_eq!(outcome.state.selected_project_id, Some(id(ID_A)));
}

#[test]
fn 大文字の_id_も同じ_id_として扱う() {
    let outcome = loaded(decode_value(&json!({
        "schemaVersion": 1,
        "projects": [
            minimal_project(ID_A, "/first"),
            minimal_project(&ID_A.to_ascii_uppercase(), "/second"),
        ],
    })));
    assert_eq!(outcome.state.projects.len(), 1);
}

// ---- 退避の名前 ----

#[test]
fn 退避の名前は_utc_の日時() {
    let none: [&str; 0] = [];
    // 2026-11-15T09:30:00Z
    assert_eq!(
        broken_file_name(1_794_735_000, none),
        "state.json.broken-20261115T093000Z"
    );
    assert_eq!(
        broken_file_name(0, none),
        "state.json.broken-19700101T000000Z"
    );
    // うるう日
    assert_eq!(
        broken_file_name(1_709_251_199, none),
        "state.json.broken-20240229T235959Z"
    );
    // 400 年の区切りのうるう年の翌日・100 年の区切り
    assert_eq!(
        broken_file_name(951_868_800, none),
        "state.json.broken-20000301T000000Z"
    );
    assert_eq!(
        broken_file_name(4_102_444_800, none),
        "state.json.broken-21000101T000000Z"
    );
}

#[test]
fn 退避の名前が既にあれば_2_を足す() {
    let base = "state.json.broken-20261115T093000Z";

    assert_eq!(broken_file_name(1_794_735_000, [base]), format!("{base}-2"));
    assert_eq!(
        broken_file_name(
            1_794_735_000,
            vec![base.to_owned(), format!("{base}-2"), format!("{base}-4")]
        ),
        format!("{base}-3")
    );
    // 関係の無い名前は見ない
    assert_eq!(
        broken_file_name(1_794_735_000, ["state.json", "settings.toml"]),
        base
    );
}
