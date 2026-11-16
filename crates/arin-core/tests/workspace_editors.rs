//! `Workspace` のエディタのタブ(ADR-015 の 3・4、計画書 T81b。【要確認】1・2 は推奨の A)。
//!
//! 開く・重ねて開く・タイル表示で開く・ターミナルとの切替・前後のタブ・⌘W の対象・閉じる確認・
//! プロジェクトを外す確認・終了の確認の材料・アテンション・保存する内容・写し。

use std::path::PathBuf;

use arin_core::editor::{EditorError, EditorId, FileStamp, FileStat, ReadOnlyReason};
use arin_core::layout::tiles::DisplayMode;
use arin_core::paths::RelPath;
use arin_core::persistence::codec::PersistedState;
use arin_core::project::{Project, ProjectColor, ProjectError, ProjectId, RootReference};
use arin_core::terminal::TerminalId;
use arin_core::workspace::{
    ActiveTab, CloseEditorOutcome, RemoveOutcome, RootResolution, Startup, UnsavedEditor,
    Workspace, WorkspaceError,
};
use serde_json::json;

const HOME: &str = "/Users/someone";

fn id(n: u8) -> ProjectId {
    ProjectId::from_random_bytes([n; 16])
}

fn rel(text: &str) -> RelPath {
    RelPath::parse(text).unwrap()
}

fn stamp() -> FileStamp {
    FileStamp::new(
        FileStat {
            dev: 1,
            ino: 2,
            size: 3,
            mtime_ns: 4,
        },
        b"abc",
    )
}

/// `count` 件のプロジェクト(名前は `p1`〜)を各 1 タブで持ち、1 件目を選んだ `Workspace`。`missing` の番号は見つからない。
fn workspace_with(count: u8, missing: &[u8]) -> Workspace {
    let roots: Vec<(ProjectId, RootReference)> = (1..=count)
        .map(|n| {
            (
                id(n),
                RootReference::new(PathBuf::from(format!("{HOME}/dev/p{n}")), None),
            )
        })
        .collect();
    Workspace::restore(
        "1.2.3",
        PathBuf::from(HOME),
        [("HOME", HOME)],
        Startup {
            state: PersistedState {
                projects: roots
                    .iter()
                    .map(|(id, root)| {
                        Project::restore(id.clone(), root.clone(), None, ProjectColor::Blue)
                    })
                    .collect(),
                selected_project_id: Some(id(1)),
                ..PersistedState::default()
            },
            roots: roots
                .into_iter()
                .enumerate()
                .map(|(i, (pid, root))| {
                    let n = u8::try_from(i + 1).unwrap();
                    if missing.contains(&n) {
                        (pid, RootResolution::Missing)
                    } else {
                        (pid, RootResolution::Found(root))
                    }
                })
                .collect(),
            notice: None,
        },
    )
}

fn workspace() -> Workspace {
    workspace_with(2, &[])
}

fn open(ws: &mut Workspace, project: u8, path: &str) -> EditorId {
    ws.open_editor(&id(project), rel(path), stamp(), None)
        .expect("開けなかった")
        .0
}

fn card(ws: &Workspace, project: u8) -> arin_core::workspace::ProjectCardSnapshot {
    ws.snapshot()
        .projects
        .into_iter()
        .find(|p| p.id == id(project))
        .unwrap()
}

fn terminal_of(ws: &Workspace, project: u8) -> TerminalId {
    card(ws, project).terminal_ids[0].clone()
}

#[test]
fn 開くとタブ表示でそのエディタが表示中() {
    let mut ws = workspace();
    let (editor, created) = ws
        .open_editor(&id(1), rel("README.md"), stamp(), None)
        .unwrap();

    assert!(created);
    assert_eq!(ws.shown_editor(), Some(editor.clone()));
    assert_eq!(ws.active_tab(), Some(ActiveTab::Editor(editor.clone())));
    let card = card(&ws, 1);
    assert_eq!(card.editor_ids, vec![editor.clone()]);
    assert_eq!(card.shown_editor_id, Some(editor));
    // ターミナルのタブは閉じない。
    assert_eq!(card.terminal_ids.len(), 1);
}

#[test]
fn ほかのプロジェクトのファイルを開くとそのプロジェクトを選ぶ() {
    let mut ws = workspace();
    let editor = open(&mut ws, 2, "a.md");
    assert_eq!(ws.snapshot().selected_project_id, Some(id(2)));
    assert_eq!(ws.shown_editor(), Some(editor));
}

#[test]
fn 同じファイルを開くと新しいタブを作らず表示する() {
    let mut ws = workspace();
    let first = open(&mut ws, 1, "README.md");
    let terminal = terminal_of(&ws, 1);
    ws.activate_terminal(&terminal).unwrap();
    assert_eq!(ws.shown_editor(), None);

    let (again, created) = ws
        .open_editor(&id(1), rel("README.md"), stamp(), None)
        .unwrap();

    assert!(!created);
    assert_eq!(again, first);
    assert_eq!(ws.shown_editor(), Some(first));
    assert_eq!(ws.snapshot().editors.len(), 1);
}

#[test]
fn 開けないものは断り何も変えない() {
    let mut ws = workspace_with(2, &[2]);
    let before = ws.snapshot();

    assert!(matches!(
        ws.open_editor(&id(1), rel("notes.txt"), stamp(), None),
        Err(WorkspaceError::NotMarkdown(_))
    ));
    assert!(matches!(
        ws.open_editor(&id(2), rel("a.md"), stamp(), None),
        Err(WorkspaceError::RootMissing(_))
    ));
    assert!(matches!(
        ws.open_editor(&id(9), rel("a.md"), stamp(), None),
        Err(WorkspaceError::Project(ProjectError::NotFound(_)))
    ));
    assert_eq!(ws.snapshot(), before);
}

#[test]
fn タイル表示で開くとタブ表示になる() {
    let mut ws = workspace();
    ws.set_display_mode(DisplayMode::Tiles);
    ws.toggle_maximize();

    let editor = open(&mut ws, 1, "a.md");

    assert_eq!(ws.display_mode(), DisplayMode::Tabs);
    assert!(!ws.snapshot().display.maximized);
    assert_eq!(ws.shown_editor(), Some(editor.clone()));
    // 保存する表示モードもタブ表示。
    assert_eq!(ws.persisted_state().layout.display_mode, DisplayMode::Tabs);

    // タイル表示に戻すとエディタは隠れる(閉じない)。⌘W の対象はフォーカス中のタイル。
    ws.set_display_mode(DisplayMode::Tiles);
    assert_eq!(ws.shown_editor(), None);
    assert_eq!(
        ws.active_tab(),
        Some(ActiveTab::Terminal(terminal_of(&ws, 1)))
    );
    assert_eq!(card(&ws, 1).editor_ids, vec![editor.clone()]);
    // タブ表示へ戻すと、また出る。
    ws.set_display_mode(DisplayMode::Tabs);
    assert_eq!(ws.shown_editor(), Some(editor));
}

#[test]
fn ターミナルのタブを選ぶとエディタは隠れるが閉じない() {
    let mut ws = workspace();
    let editor = open(&mut ws, 1, "a.md");
    let terminal = terminal_of(&ws, 1);

    ws.activate_terminal(&terminal).unwrap();
    assert_eq!(ws.shown_editor(), None);
    assert_eq!(card(&ws, 1).shown_editor_id, None);
    assert_eq!(card(&ws, 1).editor_ids, vec![editor.clone()]);

    // エディタのタブを選ぶと戻る。
    ws.select_editor(&editor).unwrap();
    assert_eq!(ws.shown_editor(), Some(editor.clone()));

    // ⌘T(新しいタブ)でもエディタは隠れる。
    let opened = ws.open_terminal_in_selected().unwrap();
    assert_eq!(ws.shown_editor(), None);
    assert_eq!(ws.active_tab(), Some(ActiveTab::Terminal(opened)));
    assert_eq!(card(&ws, 1).editor_ids, vec![editor]);
}

#[test]
fn タイルにフォーカスするとそのプロジェクトのエディタは隠れる() {
    let mut ws = workspace();
    open(&mut ws, 1, "a.md");
    ws.set_display_mode(DisplayMode::Tiles);
    ws.focus_terminal(&terminal_of(&ws, 1)).unwrap();
    ws.set_display_mode(DisplayMode::Tabs);
    assert_eq!(ws.shown_editor(), None);
}

#[test]
fn 別のプロジェクトのエディタを選ぶとそのプロジェクトを選びタブ表示にする() {
    let mut ws = workspace();
    let editor = open(&mut ws, 2, "a.md");
    ws.select_project(&id(1)).unwrap();
    ws.set_display_mode(DisplayMode::Tiles);

    ws.select_editor(&editor).unwrap();

    assert_eq!(ws.snapshot().selected_project_id, Some(id(2)));
    assert_eq!(ws.display_mode(), DisplayMode::Tabs);
    assert_eq!(ws.shown_editor(), Some(editor));

    let missing = EditorId::new("editor-99");
    assert_eq!(
        ws.select_editor(&missing),
        Err(EditorError::NotFound(missing))
    );
}

#[test]
fn 次のタブはターミナルのあとにエディタへ進み先頭へ戻る() {
    let mut ws = workspace();
    let t1 = terminal_of(&ws, 1);
    let t2 = ws.open_terminal(&id(1)).unwrap();
    let e1 = open(&mut ws, 1, "a.md");
    let e2 = open(&mut ws, 1, "b.md");
    // ほかのプロジェクトのエディタは並びに入らない。
    open(&mut ws, 2, "other.md");
    ws.select_project(&id(1)).unwrap();
    ws.activate_terminal(&t1).unwrap();

    assert_eq!(ws.select_next_tab(), Some(ActiveTab::Terminal(t2.clone())));
    assert_eq!(ws.select_next_tab(), Some(ActiveTab::Editor(e1.clone())));
    assert_eq!(ws.shown_editor(), Some(e1.clone()));
    assert_eq!(ws.select_next_tab(), Some(ActiveTab::Editor(e2.clone())));
    assert_eq!(ws.select_next_tab(), Some(ActiveTab::Terminal(t1.clone())));
    assert_eq!(ws.shown_editor(), None);

    // 前へは逆順で回る。
    assert_eq!(ws.select_previous_tab(), Some(ActiveTab::Editor(e2)));
    assert_eq!(ws.select_previous_tab(), Some(ActiveTab::Editor(e1)));
    assert_eq!(ws.select_previous_tab(), Some(ActiveTab::Terminal(t2)));
    assert_eq!(ws.select_previous_tab(), Some(ActiveTab::Terminal(t1)));
}

#[test]
fn ターミナルが無くてもエディタだけで前後に回る() {
    let mut ws = workspace();
    let t1 = terminal_of(&ws, 1);
    let e1 = open(&mut ws, 1, "a.md");
    let e2 = open(&mut ws, 1, "b.md");
    ws.close_terminal(&t1).unwrap();

    assert_eq!(ws.active_tab(), Some(ActiveTab::Editor(e2.clone())));
    assert_eq!(ws.select_next_tab(), Some(ActiveTab::Editor(e1.clone())));
    assert_eq!(ws.select_next_tab(), Some(ActiveTab::Editor(e2)));
}

#[test]
fn タイル表示の前後のタブはターミナルだけを回る() {
    let mut ws = workspace();
    let t1 = terminal_of(&ws, 1);
    let t2 = ws.open_terminal(&id(1)).unwrap();
    open(&mut ws, 1, "a.md");
    ws.set_display_mode(DisplayMode::Tiles);
    ws.activate_terminal(&t2).unwrap();

    assert_eq!(ws.select_next_tab(), Some(ActiveTab::Terminal(t1)));
    assert_eq!(ws.display_mode(), DisplayMode::Tiles);
}

#[test]
fn 選んでいないかタブが無ければ前後のタブは無い() {
    let mut ws = workspace();
    let t1 = terminal_of(&ws, 1);
    ws.close_terminal(&t1).unwrap();
    assert_eq!(ws.select_next_tab(), None);
    assert_eq!(ws.select_previous_tab(), None);
    assert_eq!(ws.active_tab(), None);
}

#[test]
fn 表示中がエディタなら閉じる対象はエディタ() {
    let mut ws = workspace();
    let terminal = terminal_of(&ws, 1);
    assert_eq!(ws.active_tab(), Some(ActiveTab::Terminal(terminal.clone())));

    let editor = open(&mut ws, 1, "a.md");
    assert_eq!(ws.active_tab(), Some(ActiveTab::Editor(editor.clone())));

    assert_eq!(
        ws.close_editor(&editor, false),
        Ok(CloseEditorOutcome::Closed)
    );
    // 閉じたらターミナルに戻る。
    assert_eq!(ws.active_tab(), Some(ActiveTab::Terminal(terminal)));
    assert!(ws.snapshot().editors.is_empty());
    assert_eq!(
        ws.close_editor(&editor, false),
        Err(EditorError::NotFound(editor))
    );
}

#[test]
fn 未保存のエディタは確認なしでは閉じない() {
    let mut ws = workspace();
    let editor = open(&mut ws, 1, "a.md");
    assert_eq!(ws.set_editor_dirty(&editor, true), Ok(true));
    let before = ws.snapshot();

    assert_eq!(
        ws.close_editor(&editor, false),
        Ok(CloseEditorOutcome::NeedsConfirmation)
    );
    assert_eq!(ws.snapshot(), before, "何も変えない");

    assert_eq!(
        ws.close_editor(&editor, true),
        Ok(CloseEditorOutcome::Closed)
    );
    assert!(ws.snapshot().editors.is_empty());
}

#[test]
fn 未保存の印と版を読み書きできる() {
    let mut ws = workspace();
    let editor = open(&mut ws, 1, "docs/a.md");
    assert_eq!(ws.set_editor_dirty(&editor, false), Ok(false));
    assert_eq!(ws.set_editor_dirty(&editor, true), Ok(true));
    assert!(ws.snapshot().editors[0].dirty);

    assert_eq!(ws.editor_stamp(&editor), Ok(stamp()));
    let newer = FileStamp::new(
        FileStat {
            dev: 1,
            ino: 9,
            size: 1,
            mtime_ns: 5,
        },
        b"x",
    );
    ws.set_editor_stamp(&editor, newer).unwrap();
    assert_eq!(ws.editor_stamp(&editor), Ok(newer));

    let tab = ws.editor(&editor).unwrap();
    assert_eq!(tab.project, id(1));
    assert_eq!(tab.path, rel("docs/a.md"));

    let missing = EditorId::new("editor-99");
    assert_eq!(
        ws.editor_stamp(&missing),
        Err(EditorError::NotFound(missing.clone()))
    );
    assert_eq!(
        ws.set_editor_dirty(&missing, true),
        Err(EditorError::NotFound(missing))
    );
}

#[test]
fn プロジェクトを外すと未保存の一覧で確認を求める() {
    let mut ws = workspace();
    let clean = open(&mut ws, 2, "clean.md");
    let dirty = open(&mut ws, 2, "docs/dirty.md");
    ws.set_editor_dirty(&dirty, true).unwrap();
    let before = ws.snapshot();

    let outcome = ws.remove_project(&id(2), &[], false).unwrap();
    assert_eq!(
        outcome,
        RemoveOutcome::NeedsConfirmation {
            running: vec![],
            unsaved: vec![UnsavedEditor {
                editor_id: dirty.clone(),
                project_name: "p2".to_owned(),
                path: rel("docs/dirty.md"),
            }],
        }
    );
    assert_eq!(ws.snapshot(), before);

    // 確認したら外し、エディタも消える。
    let outcome = ws.remove_project(&id(2), &[], true).unwrap();
    assert!(matches!(outcome, RemoveOutcome::Removed { .. }));
    let snapshot = ws.snapshot();
    assert!(snapshot.editors.is_empty());
    assert!(ws.editor(&clean).is_none());
    assert!(ws.editor(&dirty).is_none());
}

#[test]
fn 未保存が無ければプロジェクトを外すのに確認は要らない() {
    let mut ws = workspace();
    let editor = open(&mut ws, 2, "a.md");
    let outcome = ws.remove_project(&id(2), &[], false).unwrap();
    assert!(matches!(outcome, RemoveOutcome::Removed { .. }));
    assert!(ws.editor(&editor).is_none());
}

#[test]
fn 未保存の一覧が終了の確認に載る() {
    let mut ws = workspace_with(3, &[]);
    let b = open(&mut ws, 2, "b.md");
    let a = open(&mut ws, 1, "a.md");
    open(&mut ws, 3, "clean.md");
    ws.set_editor_dirty(&b, true).unwrap();
    ws.set_editor_dirty(&a, true).unwrap();

    // プロジェクトの並び順、その中は開いた順。
    assert_eq!(
        ws.unsaved_editors(),
        vec![
            UnsavedEditor {
                editor_id: a,
                project_name: "p1".to_owned(),
                path: rel("a.md"),
            },
            UnsavedEditor {
                editor_id: b,
                project_name: "p2".to_owned(),
                path: rel("b.md"),
            },
        ]
    );
}

#[test]
fn 表示中がエディタのプロジェクトのターミナルにはアテンションが付く() {
    let mut ws = workspace();
    let terminal = terminal_of(&ws, 1);
    assert!(
        !ws.note_attention(&terminal),
        "見ているターミナルには付かない"
    );

    open(&mut ws, 1, "a.md");
    assert!(ws.attended_terminals().is_empty());
    assert!(ws.note_attention(&terminal));
    assert!(ws.snapshot().terminals[0].attention);

    // ターミナルに戻ると見ているので印が消える。
    ws.activate_terminal(&terminal).unwrap();
    assert!(!ws.snapshot().terminals[0].attention);
}

#[test]
fn エディタを閉じて見えたターミナルの印は消える() {
    let mut ws = workspace();
    let terminal = terminal_of(&ws, 1);
    let editor = open(&mut ws, 1, "a.md");
    assert!(ws.note_attention(&terminal));

    ws.close_editor(&editor, false).unwrap();
    assert!(!ws.snapshot().terminals[0].attention);
}

#[test]
fn エディタを開いても保存する内容は変わらない() {
    let mut ws = workspace();
    let before = ws.persisted_state();
    let editor = open(&mut ws, 1, "a.md");
    ws.set_editor_dirty(&editor, true).unwrap();
    assert_eq!(ws.persisted_state(), before);
}

#[test]
fn 写しの_json_の形() {
    let mut ws = workspace();
    let editor = ws
        .open_editor(
            &id(1),
            rel("docs/Guide.md"),
            stamp(),
            Some(ReadOnlyReason::HardLink),
        )
        .unwrap()
        .0;
    ws.set_editor_dirty(&editor, true).unwrap();
    open(&mut ws, 2, "b.markdown");
    ws.select_project(&id(1)).unwrap();

    let value = serde_json::to_value(ws.snapshot()).unwrap();

    assert_eq!(
        value["editors"],
        json!([
            {
                "id": "editor-1",
                "projectId": id(1).as_str(),
                "path": "docs/Guide.md",
                "label": "Guide.md",
                "dirty": true,
                "readOnly": "hard-link",
            },
            {
                "id": "editor-2",
                "projectId": id(2).as_str(),
                "path": "b.markdown",
                "label": "b.markdown",
                "dirty": false,
                "readOnly": null,
            },
        ])
    );
    assert_eq!(value["editor"], json!({ "vim": false }));
    assert_eq!(value["projects"][0]["editorIds"], json!(["editor-1"]));
    assert_eq!(value["projects"][0]["shownEditorId"], json!("editor-1"));
    assert_eq!(value["projects"][1]["editorIds"], json!(["editor-2"]));
    assert_eq!(value["projects"][1]["shownEditorId"], json!("editor-2"));
    // 版・絶対パスは写しに載せない。
    let text = value.to_string();
    assert!(!text.contains(HOME));
    assert!(!text.contains("digest"));
}

#[test]
fn 閉じる結果と未保存の項目の_json_の形() {
    assert_eq!(
        serde_json::to_value(CloseEditorOutcome::Closed).unwrap(),
        json!({ "kind": "closed" })
    );
    assert_eq!(
        serde_json::to_value(CloseEditorOutcome::NeedsConfirmation).unwrap(),
        json!({ "kind": "needs-confirmation" })
    );
    assert_eq!(
        serde_json::to_value(UnsavedEditor {
            editor_id: EditorId::new("editor-1"),
            project_name: "app".to_owned(),
            path: rel("a.md"),
        })
        .unwrap(),
        json!({ "editorId": "editor-1", "projectName": "app", "path": "a.md" })
    );
}
