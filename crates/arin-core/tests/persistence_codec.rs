//! `state.json` 版 1 の読み書き(計画書 T30a、設計書 §6.2、決定 (d)、【仮定】2・3・13)。
//!
//! 入出力はしない。読んだバイト列を渡すと「状態」「無い」「退避する(理由つき)」のどれかが返る。

use std::path::PathBuf;

use arin_core::layout::font_sizes::{FontSizeTarget, FontSizes};
use arin_core::layout::tiles::DisplayMode;
use arin_core::layout::translucency::{Translucency, TranslucencyTarget};
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
            )
            .with_show_hidden(true),
            project(ID_C, "/Users/me/notes", None, None, ProjectColor::Pink),
        ],
        selected_project_id: Some(id(ID_B)),
        layout: PersistedLayout {
            sidebar_width: Some(312.5),
            sidebar_collapsed: true,
            card_area_height: Some(240.0),
            display_mode: DisplayMode::Tabs,
            font_sizes: FontSizes::default(),
            translucency: Translucency::default(),
            editor_vim: false,
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
                    "tree": { "showHidden": true },
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
                display_mode: DisplayMode::Tabs,
                font_sizes: FontSizes::default(),
                translucency: Translucency::default(),
                editor_vim: false,
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
        "ui": { "futureUiKey": "x", "selectedProjectId": ID_A },
    })));

    // `tree.showHidden` は読む(計画書 T38e)。`tree.expanded` は M6 まで読まない
    assert_eq!(
        outcome.state.projects,
        vec![project(ID_A, "/a", Some("Ym0="), None, ProjectColor::Green).with_show_hidden(true)]
    );
    assert_eq!(outcome.state.selected_project_id, Some(id(ID_A)));
}

// ---- ツリーの保存設定(計画書 T38e、設計書 §6.2 の `projects[].tree.showHidden`、【仮定】3) ----

#[test]
fn 隠しファイル表示の既定はオフ() {
    assert!(!project(ID_A, "/a", None, None, ProjectColor::Red).show_hidden());

    let outcome = loaded(decode_value(&json!({
        "schemaVersion": 1,
        "projects": [minimal_project(ID_A, "/a")],
    })));
    assert!(!outcome.state.projects[0].show_hidden());
}

#[test]
fn 隠しファイル表示を書いて読むと同じに戻る() {
    let state = PersistedState {
        projects: vec![
            project(ID_A, "/a", Some("Ym0="), None, ProjectColor::Red).with_show_hidden(true),
            project(ID_B, "/b", Some("Ym0="), None, ProjectColor::Blue),
        ],
        ..PersistedState::default()
    };

    let back = loaded(decode_str(&encode(&state).expect("書ける")));

    assert_eq!(back.state, state);
    assert!(back.state.projects[0].show_hidden());
    assert!(!back.state.projects[1].show_hidden());
}

/// M2 のアプリが書いた `state.json`(版 1。`tree` のキーが無い)。利用者の手元にあるものをそのまま読めること。
const M2_STATE_JSON: &str = r#"{
  "schemaVersion": 1,
  "projects": [
    {
      "id": "0123abcd-89ab-4def-8123-456789abcdef",
      "bookmark": "Ym9va21hcmstYQ==",
      "lastKnownPath": "/Users/me/dev/app",
      "customName": null,
      "colorId": "red"
    },
    {
      "id": "11111111-2222-4333-8444-555555555555",
      "bookmark": null,
      "lastKnownPath": "/Users/me/notes",
      "customName": "メモ",
      "colorId": "teal"
    }
  ],
  "ui": {
    "selectedProjectId": "11111111-2222-4333-8444-555555555555",
    "sidebarWidth": 280.0,
    "sidebarCollapsed": false,
    "cardAreaHeight": 220.0
  }
}
"#;

#[test]
fn tree_キーの無い状態ファイルも読める() {
    // 退避せずに読め、隠しファイル表示は既定(オフ)になる
    let outcome = loaded(decode_str(M2_STATE_JSON));

    assert_eq!(
        outcome.state,
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
                    "/Users/me/notes",
                    None,
                    Some("メモ"),
                    ProjectColor::Teal
                ),
            ],
            selected_project_id: Some(id(ID_B)),
            layout: PersistedLayout {
                sidebar_width: Some(280.0),
                sidebar_collapsed: false,
                card_area_height: Some(220.0),
                display_mode: DisplayMode::Tabs,
                font_sizes: FontSizes::default(),
                translucency: Translucency::default(),
                editor_vim: false,
            },
        }
    );
    assert!(outcome.state.projects.iter().all(|p| !p.show_hidden()));
    assert_eq!(outcome.rebuild_bookmarks, vec![id(ID_B)]);

    // 書き直しても版は 1 のまま。隠しファイル表示が既定のあいだは `tree` を書かず、M2 と同じ形になる
    let rewritten: Value = serde_json::from_str(&encode(&outcome.state).expect("書ける")).unwrap();
    let original: Value = serde_json::from_str(M2_STATE_JSON).unwrap();
    assert_eq!(rewritten, original);
}

#[test]
fn tree_の形が違えばそのプロジェクトだけ既定値で退避しない() {
    let bad_trees = [
        json!(null),
        json!(true),
        json!(1),
        json!("showHidden"),
        json!([]),
        json!({ "showHidden": "yes" }),
        json!({ "showHidden": 1 }),
        json!({ "showHidden": null }),
        json!({ "expanded": "src" }),
    ];
    for bad in bad_trees {
        let outcome = loaded(decode_value(&json!({
            "schemaVersion": 1,
            "projects": [
                { "id": ID_A, "bookmark": "Ym0=", "lastKnownPath": "/a", "colorId": "red", "tree": bad },
                { "id": ID_B, "bookmark": "Ym0=", "lastKnownPath": "/b", "colorId": "blue", "tree": { "showHidden": true } },
            ],
            "ui": { "selectedProjectId": ID_A },
        })));

        // 形の違うプロジェクトは既定値(オフ)、ほかのプロジェクトと選択はそのまま
        assert_eq!(
            outcome.state.projects,
            vec![
                project(ID_A, "/a", Some("Ym0="), None, ProjectColor::Red),
                project(ID_B, "/b", Some("Ym0="), None, ProjectColor::Blue).with_show_hidden(true),
            ],
            "{bad}"
        );
        assert_eq!(outcome.state.selected_project_id, Some(id(ID_A)), "{bad}");
    }
}

#[test]
fn 保存する内容に展開状態を含めない() {
    // 展開状態(`tree.expanded`)は M6(T59)まで保存しない。読んだ値も書き戻さない
    let outcome = loaded(decode_value(&json!({
        "schemaVersion": 1,
        "projects": [{
            "id": ID_A,
            "bookmark": "Ym0=",
            "lastKnownPath": "/a",
            "colorId": "green",
            "tree": { "expanded": ["src", "docs"], "showHidden": true },
        }],
    })));

    let value: Value = serde_json::from_str(&encode(&outcome.state).expect("書ける")).unwrap();

    assert_eq!(value["projects"][0]["tree"], json!({ "showHidden": true }));
    assert!(!value.to_string().contains("expanded"), "{value}");
}

// ---- 表示モード(計画書 T46c、設計書 §6.2 の `ui.displayMode`、ADR-011 の 2、【仮定】16) ----

/// M3 のアプリが書いた `state.json`(版 1。`tree` はあるが `ui.displayMode` が無い)。利用者の手元にあるものをそのまま読めること。
const M3_STATE_JSON: &str = r#"{
  "schemaVersion": 1,
  "projects": [
    {
      "id": "0123abcd-89ab-4def-8123-456789abcdef",
      "bookmark": "Ym9va21hcmstYQ==",
      "lastKnownPath": "/Users/me/dev/app",
      "customName": null,
      "colorId": "red",
      "tree": {
        "showHidden": true
      }
    },
    {
      "id": "11111111-2222-4333-8444-555555555555",
      "bookmark": "Ym9va21hcmstYg==",
      "lastKnownPath": "/Users/me/notes",
      "customName": "メモ",
      "colorId": "teal"
    }
  ],
  "ui": {
    "selectedProjectId": "0123abcd-89ab-4def-8123-456789abcdef",
    "sidebarWidth": 300.0,
    "sidebarCollapsed": true,
    "cardAreaHeight": 180.0
  }
}
"#;

fn m3_state() -> PersistedState {
    PersistedState {
        projects: vec![
            project(
                ID_A,
                "/Users/me/dev/app",
                Some("Ym9va21hcmstYQ=="),
                None,
                ProjectColor::Red,
            )
            .with_show_hidden(true),
            project(
                ID_B,
                "/Users/me/notes",
                Some("Ym9va21hcmstYg=="),
                Some("メモ"),
                ProjectColor::Teal,
            ),
        ],
        selected_project_id: Some(id(ID_A)),
        layout: PersistedLayout {
            sidebar_width: Some(300.0),
            sidebar_collapsed: true,
            card_area_height: Some(180.0),
            display_mode: DisplayMode::Tabs,
            font_sizes: FontSizes::default(),
            translucency: Translucency::default(),
            editor_vim: false,
        },
    }
}

#[test]
fn 表示モードを書いて読むと同じモードに戻る() {
    for mode in [DisplayMode::Tabs, DisplayMode::Tiles] {
        let mut state = sample_state();
        state.layout.display_mode = mode;

        let back = loaded(decode_str(&encode(&state).expect("書ける")));

        assert_eq!(back.state.layout.display_mode, mode);
        assert_eq!(back.state, state);
    }
}

#[test]
fn display_mode_が無い状態ファイルはタブ表示で始まる() {
    // M3 までの `state.json`(文字列のまま)を退避せずに読め、表示モードはタブ表示になる。ほかの内容はそのまま
    let m3 = loaded(decode_str(M3_STATE_JSON));
    assert_eq!(m3.state, m3_state());
    assert_eq!(m3.state.layout.display_mode, DisplayMode::Tabs);
    assert!(m3.rebuild_bookmarks.is_empty());

    // M2 の `state.json`(`tree` も無い)も同じ
    let m2 = loaded(decode_str(M2_STATE_JSON));
    assert_eq!(m2.state.layout.display_mode, DisplayMode::Tabs);

    // `ui` が丸ごと無いときもタブ表示
    let bare = loaded(decode_value(&json!({ "schemaVersion": 1 })));
    assert_eq!(bare.state.layout.display_mode, DisplayMode::Tabs);
}

#[test]
fn 知らない値はタブ表示にし退避しない() {
    let unknown = [
        json!("grid"),
        json!("Tiles"),
        json!("TILES"),
        json!(""),
        json!(1),
        json!(0),
        json!(true),
        json!(null),
        json!([]),
        json!(["tiles"]),
        json!({}),
        json!({ "mode": "tiles" }),
    ];
    for bad in unknown {
        let mut value: Value = serde_json::from_str(M3_STATE_JSON).unwrap();
        value["ui"]["displayMode"] = bad.clone();

        // 退避せずに読め、表示モードだけがタブ表示になる。プロジェクト一覧・選択・ペインの寸法は保つ
        let outcome = loaded(decode_value(&value));
        assert_eq!(outcome.state, m3_state(), "{bad}");
    }
}

#[test]
fn タブ表示のあいだは_display_mode_を書かない() {
    // 【仮定】(計画書の「タブ表示のときも書く」と違う): 既定のあいだは書かず、M3 までの `state.json` と同じ形のまま残す
    // (`tree` の既定値を書かないのと同じ。読むときは無ければタブ表示)
    let outcome = loaded(decode_str(M3_STATE_JSON));
    let rewritten: Value = serde_json::from_str(&encode(&outcome.state).expect("書ける")).unwrap();
    let original: Value = serde_json::from_str(M3_STATE_JSON).unwrap();

    assert_eq!(rewritten, original);
    assert!(rewritten["ui"].get("displayMode").is_none(), "{rewritten}");
    // 版は 1 のまま
    assert_eq!(rewritten["schemaVersion"], json!(1));
}

#[test]
fn タイル表示のときは_display_mode_に_tiles_を書く() {
    let mut state = m3_state();
    state.layout.display_mode = DisplayMode::Tiles;

    let value: Value = serde_json::from_str(&encode(&state).expect("書ける")).unwrap();

    assert_eq!(value["schemaVersion"], json!(1));
    assert_eq!(
        value["ui"],
        json!({
            "selectedProjectId": ID_A,
            "sidebarWidth": 300.0,
            "sidebarCollapsed": true,
            "cardAreaHeight": 180.0,
            "displayMode": "tiles",
        })
    );
}

// ---- 文字の大きさ(計画書 T68b、ADR-013 の 1、`ui.terminalFontSize`・`ui.interfaceFontSize`) ----

#[test]
fn 文字の大きさが無い版_1_の_state_json_をそのまま読む() {
    // M3 までの `state.json`(M4.5 も同じ形)を退避せずに読め、文字の大きさは既定。ほかの内容はそのまま
    let m3 = loaded(decode_str(M3_STATE_JSON));
    assert_eq!(m3.state, m3_state());
    assert_eq!(m3.state.layout.font_sizes, FontSizes::default());

    let m2 = loaded(decode_str(M2_STATE_JSON));
    assert_eq!(m2.state.layout.font_sizes, FontSizes::default());

    let bare = loaded(decode_value(&json!({ "schemaVersion": 1 })));
    assert_eq!(bare.state.layout.font_sizes, FontSizes::default());
}

#[test]
fn 既定の大きさは書かない() {
    // 触っていない人の `state.json` は M4.5 までと同じ形のまま(`ui.displayMode` と同じ)
    let outcome = loaded(decode_str(M3_STATE_JSON));
    let rewritten: Value = serde_json::from_str(&encode(&outcome.state).expect("書ける")).unwrap();
    let original: Value = serde_json::from_str(M3_STATE_JSON).unwrap();
    assert_eq!(rewritten, original);
    assert!(
        rewritten["ui"].get("terminalFontSize").is_none(),
        "{rewritten}"
    );
    assert!(
        rewritten["ui"].get("interfaceFontSize").is_none(),
        "{rewritten}"
    );

    // 【仮定】既定かどうかはキーごとに見る(画面だけを変えたら画面のキーだけを書く)
    let mut state = m3_state();
    state.layout.font_sizes = FontSizes::default().stepped(FontSizeTarget::Interface, 2);
    let value: Value = serde_json::from_str(&encode(&state).expect("書ける")).unwrap();
    assert!(value["ui"].get("terminalFontSize").is_none(), "{value}");
    assert_eq!(value["ui"]["interfaceFontSize"], json!(15));
}

#[test]
fn 変えた大きさを書いて読み戻せる() {
    let mut state = m3_state();
    state.layout.font_sizes = FontSizes::default()
        .stepped(FontSizeTarget::Terminal, 5)
        .stepped(FontSizeTarget::Interface, -2);

    let text = encode(&state).expect("書ける");
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        value["ui"],
        json!({
            "selectedProjectId": ID_A,
            "sidebarWidth": 300.0,
            "sidebarCollapsed": true,
            "cardAreaHeight": 180.0,
            "terminalFontSize": 20,
            "interfaceFontSize": 11,
        })
    );

    let back = loaded(decode_str(&text));
    assert_eq!(back.state, state);
    assert_eq!(
        back.state.layout.font_sizes,
        FontSizes {
            terminal: 20,
            interface: 11,
        }
    );
}

#[test]
fn 数でない文字の大きさは退避せず既定() {
    let not_numbers = [
        json!(null),
        json!("16"),
        json!(""),
        json!(true),
        json!([]),
        json!([16]),
        json!({}),
        json!({ "value": 16 }),
    ];
    for bad in not_numbers {
        let mut value: Value = serde_json::from_str(M3_STATE_JSON).unwrap();
        value["ui"]["terminalFontSize"] = bad.clone();
        value["ui"]["interfaceFontSize"] = bad.clone();

        // 退避せずに読め、文字の大きさだけが既定になる。プロジェクト一覧・選択・ペインの寸法は保つ
        let outcome = loaded(decode_value(&value));
        assert_eq!(outcome.state, m3_state(), "{bad}");
    }

    // 片方だけ壊れていても、もう片方は読む
    let mut value: Value = serde_json::from_str(M3_STATE_JSON).unwrap();
    value["ui"]["terminalFontSize"] = json!("大きい");
    value["ui"]["interfaceFontSize"] = json!(16);
    let outcome = loaded(decode_value(&value));
    assert_eq!(
        outcome.state.layout.font_sizes,
        FontSizes {
            terminal: 15,
            interface: 16,
        }
    );
}

#[test]
fn 範囲の外の文字の大きさは端に留める() {
    // (ターミナル, 画面, 読んだ結果)。整数でない数は四捨五入してから留める
    let cases = [
        (json!(4), json!(30), (9, 18)),
        (json!(100), json!(-1), (28, 11)),
        (json!(1e300), json!(0), (28, 11)),
        (json!(16.5), json!(12.4), (17, 12)),
    ];
    for (terminal, interface, (want_terminal, want_interface)) in cases {
        let mut value: Value = serde_json::from_str(M3_STATE_JSON).unwrap();
        value["ui"]["terminalFontSize"] = terminal.clone();
        value["ui"]["interfaceFontSize"] = interface.clone();

        let outcome = loaded(decode_value(&value));

        assert_eq!(
            outcome.state.layout.font_sizes,
            FontSizes {
                terminal: want_terminal,
                interface: want_interface,
            },
            "{terminal} / {interface}"
        );
        // ほかの内容は保つ(退避しない)
        assert_eq!(outcome.state.projects, m3_state().projects);
    }
}

#[test]
fn 文字の大きさを足しても_schema_version_は_1() {
    assert_eq!(STATE_SCHEMA_VERSION, 1);
    let mut state = sample_state();
    state.layout.font_sizes = FontSizes::default().stepped(FontSizeTarget::Terminal, 1);

    let value: Value = serde_json::from_str(&encode(&state).expect("書ける")).unwrap();

    assert_eq!(value["schemaVersion"], json!(1));
    assert_eq!(value["ui"]["terminalFontSize"], json!(16));
}

// ---- 透過(ADR-014 の 1、`ui.windowTranslucency`・`ui.terminalTranslucency`。AC-G04) ----

#[test]
fn 透過が無い版_1_の_state_json_をそのまま読み_既定は書かない() {
    let m3 = loaded(decode_str(M3_STATE_JSON));
    assert_eq!(m3.state, m3_state());
    assert_eq!(m3.state.layout.translucency, Translucency::default());

    let rewritten: Value = serde_json::from_str(&encode(&m3.state).expect("書ける")).unwrap();
    let original: Value = serde_json::from_str(M3_STATE_JSON).unwrap();
    assert_eq!(rewritten, original);
    assert!(rewritten["ui"].get("windowTranslucency").is_none());
    assert!(rewritten["ui"].get("terminalTranslucency").is_none());

    // 既定かどうかはキーごとに見る
    let mut state = m3_state();
    state.layout.translucency = Translucency::default().stepped(TranslucencyTarget::Terminal, 10);
    let value: Value = serde_json::from_str(&encode(&state).expect("書ける")).unwrap();
    assert!(value["ui"].get("windowTranslucency").is_none(), "{value}");
    assert_eq!(value["ui"]["terminalTranslucency"], json!(30));
}

#[test]
fn 変えた透過を書いて読み戻せ_schema_version_は_1() {
    let mut state = m3_state();
    state.layout.translucency = Translucency::default()
        .stepped(TranslucencyTarget::Window, -45)
        .stepped(TranslucencyTarget::Terminal, 60);

    let text = encode(&state).expect("書ける");
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["schemaVersion"], json!(1));
    assert_eq!(value["ui"]["windowTranslucency"], json!(0));
    assert_eq!(value["ui"]["terminalTranslucency"], json!(80));

    let back = loaded(decode_str(&text));
    assert_eq!(back.state, state);
    assert_eq!(
        back.state.layout.translucency,
        Translucency {
            window: 0,
            terminal: 80,
        }
    );
}

#[test]
fn 壊れた透過は退避せず既定_端_丸めにする() {
    // 数でない値は既定(ほかの内容は保つ)
    for bad in [json!(null), json!("50"), json!(true), json!([]), json!({})] {
        let mut value: Value = serde_json::from_str(M3_STATE_JSON).unwrap();
        value["ui"]["windowTranslucency"] = bad.clone();
        value["ui"]["terminalTranslucency"] = bad.clone();
        let outcome = loaded(decode_value(&value));
        assert_eq!(outcome.state, m3_state(), "{bad}");
    }

    // (ウィンドウ, ターミナル, 読んだ結果)
    let cases = [
        (json!(-10), json!(200), (0, 80)),
        (json!(1e300), json!(81), (100, 80)),
        (json!(52), json!(13), (50, 15)),
        (json!(57.5), json!(22.4), (60, 20)),
        (json!("濃い"), json!(35), (45, 35)),
    ];
    for (window, terminal, (want_window, want_terminal)) in cases {
        let mut value: Value = serde_json::from_str(M3_STATE_JSON).unwrap();
        value["ui"]["windowTranslucency"] = window.clone();
        value["ui"]["terminalTranslucency"] = terminal.clone();

        let outcome = loaded(decode_value(&value));

        assert_eq!(
            outcome.state.layout.translucency,
            Translucency {
                window: want_window,
                terminal: want_terminal,
            },
            "{window} / {terminal}"
        );
        assert_eq!(outcome.state.projects, m3_state().projects);
    }
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

// ---- エディタの Vim のキー操作(ADR-015 の 6、`ui.editorVim`。計画書 T81e、AC-E08) ----

#[test]
fn editor_vim_が無い版_1_をそのまま読む() {
    let m3 = loaded(decode_str(M3_STATE_JSON));
    assert_eq!(m3.state, m3_state());
    assert!(!m3.state.layout.editor_vim);
}

#[test]
fn 偽は書かない() {
    let state = m3_state();
    assert!(!state.layout.editor_vim);
    let value: Value = serde_json::from_str(&encode(&state).expect("書ける")).unwrap();
    assert!(value["ui"].get("editorVim").is_none(), "{value}");
    let original: Value = serde_json::from_str(M3_STATE_JSON).unwrap();
    assert_eq!(value, original);
}

#[test]
fn 真を書いて読み戻せる() {
    let mut state = m3_state();
    state.layout.editor_vim = true;
    let text = encode(&state).expect("書ける");
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["schemaVersion"], json!(1));
    assert_eq!(value["ui"]["editorVim"], json!(true));
    let back = loaded(decode_str(&text));
    assert_eq!(back.state, state);
    assert!(back.state.layout.editor_vim);
}

#[test]
fn 真偽値でない値は退避せず偽() {
    for bad in [
        json!(null),
        json!("true"),
        json!(1),
        json!(0),
        json!([]),
        json!({}),
    ] {
        let mut value: Value = serde_json::from_str(M3_STATE_JSON).unwrap();
        value["ui"]["editorVim"] = bad.clone();
        let outcome = loaded(decode_value(&value));
        assert_eq!(outcome.state, m3_state(), "{bad}");
        assert!(!outcome.state.layout.editor_vim, "{bad}");
    }
    let mut value: Value = serde_json::from_str(M3_STATE_JSON).unwrap();
    value["ui"]["editorVim"] = json!(false);
    assert!(!loaded(decode_value(&value)).state.layout.editor_vim);
}
