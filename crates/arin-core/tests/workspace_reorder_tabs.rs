//! `Workspace` の並べ替えとタブをまとめて閉じる口(ADR-019)。
//!
//! プロジェクトの並べ替え(写し・保存する並び・タイルの並び)、タブの並べ替え(同じ種類の中だけ)、
//! タブをまとめて閉じる(確認の材料・確認後に閉じる・表示中の移り方)。

use std::path::PathBuf;

use arin_core::editor::{EditorError, EditorId, FileStamp, FileStat};
use arin_core::layout::tiles::DisplayMode;
use arin_core::paths::RelPath;
use arin_core::persistence::codec::PersistedState;
use arin_core::project::{Project, ProjectColor, ProjectError, ProjectId, RootReference};
use arin_core::terminal::{TerminalError, TerminalId};
use arin_core::workspace::{
    ActiveTab, CloseTabsOutcome, CloseTabsRequest, CloseTabsResult, RootResolution, Startup,
    Workspace,
};

const HOME: &str = "/Users/someone";

fn id(n: u8) -> ProjectId {
    ProjectId::from_random_bytes([n; 16])
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

/// `count` 件のプロジェクト(`p1`〜)を各 1 タブで持ち、1 件目を選んだ `Workspace`。
fn workspace_with(count: u8) -> Workspace {
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
                .map(|(pid, root)| (pid, RootResolution::Found(root)))
                .collect(),
            notice: None,
        },
    )
}

fn project_order(ws: &Workspace) -> Vec<ProjectId> {
    ws.snapshot().projects.into_iter().map(|p| p.id).collect()
}

fn terminal_ids(ws: &Workspace, project: u8) -> Vec<TerminalId> {
    ws.snapshot()
        .projects
        .into_iter()
        .find(|p| p.id == id(project))
        .map(|p| p.terminal_ids)
        .unwrap_or_default()
}

fn editor_ids(ws: &Workspace, project: u8) -> Vec<EditorId> {
    ws.snapshot()
        .projects
        .into_iter()
        .find(|p| p.id == id(project))
        .map(|p| p.editor_ids)
        .unwrap_or_default()
}

fn open(ws: &mut Workspace, project: u8, path: &str) -> EditorId {
    ws.open_editor(&id(project), RelPath::parse(path).unwrap(), stamp(), None)
        .expect("開けなかった")
        .0
}

/// p1 にターミナル 3 枚(t1・t2・t3)を持つ。
fn three_terminals() -> (Workspace, Vec<TerminalId>) {
    let mut ws = workspace_with(1);
    ws.open_terminal(&id(1)).unwrap();
    ws.open_terminal(&id(1)).unwrap();
    let tabs = terminal_ids(&ws, 1);
    assert_eq!(tabs.len(), 3);
    (ws, tabs)
}

// ── プロジェクトの並べ替え ──

#[test]
fn プロジェクトを動かすと写しと保存する並びが変わり選択は変えない() {
    let mut ws = workspace_with(3);
    assert_eq!(ws.move_project(&id(1), 2), Ok(true));
    assert_eq!(project_order(&ws), [id(2), id(3), id(1)]);
    let saved: Vec<ProjectId> = ws
        .persisted_state()
        .projects
        .iter()
        .map(|p| p.id().clone())
        .collect();
    assert_eq!(saved, [id(2), id(3), id(1)]);
    assert_eq!(ws.snapshot().selected_project_id, Some(id(1)));

    assert_eq!(ws.move_project(&id(3), 0), Ok(true));
    assert_eq!(project_order(&ws), [id(3), id(2), id(1)]);
}

#[test]
fn プロジェクトを同じ位置へ動かしても変わらず末尾より先は末尾() {
    let mut ws = workspace_with(3);
    assert_eq!(ws.move_project(&id(2), 1), Ok(false));
    assert_eq!(ws.move_project(&id(1), 99), Ok(true));
    assert_eq!(project_order(&ws), [id(2), id(3), id(1)]);
}

#[test]
fn 無いプロジェクトは動かせず何も変えない() {
    let mut ws = workspace_with(2);
    assert_eq!(
        ws.move_project(&id(9), 0),
        Err(ProjectError::NotFound(id(9)))
    );
    assert_eq!(project_order(&ws), [id(1), id(2)]);
}

#[test]
fn タイルの並びはプロジェクトの順に付いていく() {
    let mut ws = workspace_with(3);
    ws.set_display_mode(DisplayMode::Tiles);
    let before = ws.snapshot().display.tile_ids;
    ws.move_project(&id(3), 0).unwrap();
    let after = ws.snapshot().display.tile_ids;
    assert_eq!(
        after,
        [before[2].clone(), before[0].clone(), before[1].clone()]
    );
    // フォーカス中のタイル(選択中の p1 の表示中のタブ)は変わらない
    assert_eq!(
        ws.snapshot().display.focused_terminal_id,
        Some(before[0].clone())
    );
}

// ── タブの並べ替え ──

#[test]
fn ターミナルのタブを動かしても表示中と番号は変えない() {
    let (mut ws, tabs) = three_terminals();
    let active = ws.active_terminal();
    assert_eq!(ws.move_terminal_tab(&tabs[2], 0), Ok(true));
    assert_eq!(
        terminal_ids(&ws, 1),
        [tabs[2].clone(), tabs[0].clone(), tabs[1].clone()]
    );
    assert_eq!(ws.active_terminal(), active);
    let labels: Vec<String> = ws
        .snapshot()
        .terminals
        .into_iter()
        .map(|t| t.label)
        .collect();
    assert_eq!(labels, ["ターミナル 3", "ターミナル 1", "ターミナル 2"]);
    assert_eq!(ws.move_terminal_tab(&tabs[2], 0), Ok(false));
}

#[test]
fn 無いターミナルのタブは動かせない() {
    let (mut ws, _) = three_terminals();
    let missing = TerminalId::new("terminal-99");
    assert_eq!(
        ws.move_terminal_tab(&missing, 0),
        Err(TerminalError::NotFound(missing))
    );
}

#[test]
fn エディタのタブはそのプロジェクトのエディタの中で動きほかのプロジェクトは変えない() {
    let mut ws = workspace_with(2);
    let a = open(&mut ws, 1, "a.md");
    let x = open(&mut ws, 2, "x.md");
    let b = open(&mut ws, 1, "b.md");
    let c = open(&mut ws, 1, "c.md");
    assert_eq!(ws.move_editor_tab(&c, 0), Ok(true));
    assert_eq!(editor_ids(&ws, 1), [c.clone(), a.clone(), b.clone()]);
    assert_eq!(editor_ids(&ws, 2), [x]);
    assert_eq!(ws.move_editor_tab(&a, 5), Ok(true));
    assert_eq!(editor_ids(&ws, 1), [c.clone(), b.clone(), a.clone()]);
    // 表示中のエディタ(最後に開いた c)は変わらない
    assert_eq!(ws.active_tab(), Some(ActiveTab::Editor(c)));
    let missing = EditorId::new("editor-99");
    assert_eq!(
        ws.move_editor_tab(&missing, 0),
        Err(EditorError::NotFound(missing))
    );
}

// ── タブをまとめて閉じる ──

/// 閉じる求め(確認はまだ)。
fn request(terminals: &[TerminalId], editors: &[EditorId]) -> CloseTabsRequest {
    CloseTabsRequest {
        terminals: terminals.to_vec(),
        editors: editors.to_vec(),
        ..CloseTabsRequest::default()
    }
}

/// 確認の一覧をそのまま確認済みにした求め(画面の「閉じる」)。
fn confirmed(mut request: CloseTabsRequest, result: &CloseTabsResult) -> CloseTabsRequest {
    let CloseTabsResult::NeedsConfirmation { running, unsaved } = result else {
        panic!("確認を求めていない: {result:?}");
    };
    request.confirmed_terminals = running.iter().map(|t| t.terminal_id.clone()).collect();
    request.confirmed_editors = unsaved.iter().map(|e| e.editor_id.clone()).collect();
    request
}

#[test]
fn 動作中も未保存も無ければ確認せずに閉じて閉じたターミナルを返す() {
    let (mut ws, tabs) = three_terminals();
    let result = ws
        .close_tabs(&id(1), &request(&tabs[1..], &[]), &[])
        .unwrap();
    assert_eq!(
        result,
        CloseTabsResult::Closed {
            terminals: tabs[1..].to_vec()
        }
    );
    assert_eq!(terminal_ids(&ws, 1), [tabs[0].clone()]);
    assert_eq!(ws.active_terminal(), Some(tabs[0].clone()));
}

#[test]
fn 閉じるタブに動作中か未保存があれば何も変えずに確認を求める() {
    let (mut ws, tabs) = three_terminals();
    let doc = open(&mut ws, 1, "notes.md");
    ws.set_editor_dirty(&doc, true).unwrap();
    // 動作中は t1 と t3。閉じるのは t3 とエディタだけ(t1 は閉じないので確認に出さない)
    let running = [tabs[0].clone(), tabs[2].clone()];
    let before = ws.snapshot();
    let ask = request(&tabs[2..], std::slice::from_ref(&doc));
    let result = ws.close_tabs(&id(1), &ask, &running).unwrap();
    let CloseTabsResult::NeedsConfirmation {
        running: busy,
        unsaved,
    } = &result
    else {
        panic!("確認を求めなかった: {result:?}");
    };
    assert_eq!(
        busy.iter()
            .map(|t| t.terminal_id.clone())
            .collect::<Vec<_>>(),
        [tabs[2].clone()]
    );
    assert_eq!(busy[0].label, "ターミナル 3");
    assert_eq!(unsaved.len(), 1);
    assert_eq!(unsaved[0].editor_id, doc);
    assert_eq!(unsaved[0].path.as_str(), "notes.md");
    assert_eq!(ws.snapshot(), before);
    assert!(matches!(
        CloseTabsOutcome::from(&result),
        CloseTabsOutcome::NeedsConfirmation { .. }
    ));

    // 確認したら閉じる(未保存の変更は捨てる)
    let result = ws
        .close_tabs(&id(1), &confirmed(ask, &result), &running)
        .unwrap();
    assert_eq!(
        result,
        CloseTabsResult::Closed {
            terminals: vec![tabs[2].clone()]
        }
    );
    assert_eq!(CloseTabsOutcome::from(&result), CloseTabsOutcome::Closed);
    assert_eq!(terminal_ids(&ws, 1), tabs[..2].to_vec());
    assert!(editor_ids(&ws, 1).is_empty());
    assert!(ws.editor(&doc).is_none());
}

#[test]
fn 確認のあとに動作中や未保存が増えていたらもう一度確認を求める() {
    let (mut ws, tabs) = three_terminals();
    let doc = open(&mut ws, 1, "notes.md");
    let ask = request(&tabs[1..], std::slice::from_ref(&doc));
    // 確認した時点では t2 だけが動作中
    let first = ws.close_tabs(&id(1), &ask, &tabs[1..2]).unwrap();
    let ask = confirmed(ask, &first);
    assert_eq!(ask.confirmed_terminals, [tabs[1].clone()]);

    // 確認のあいだに t3 でもコマンドが始まった: 増えた分を含めて確認し直す
    let before = ws.snapshot();
    let again = ws.close_tabs(&id(1), &ask, &tabs[1..]).unwrap();
    let CloseTabsResult::NeedsConfirmation { running, unsaved } = &again else {
        panic!("確認し直さなかった: {again:?}");
    };
    assert_eq!(running.len(), 2);
    assert!(unsaved.is_empty());
    assert_eq!(ws.snapshot(), before);

    // 確認のあいだに文書を書き換えた: 未保存が増えたので確認し直す
    let ask = confirmed(ask, &again);
    ws.set_editor_dirty(&doc, true).unwrap();
    let third = ws.close_tabs(&id(1), &ask, &tabs[1..]).unwrap();
    let CloseTabsResult::NeedsConfirmation { unsaved, .. } = &third else {
        panic!("確認し直さなかった: {third:?}");
    };
    assert_eq!(unsaved[0].editor_id, doc);

    // 全部を確認したら閉じる
    let ask = confirmed(ask, &third);
    assert!(matches!(
        ws.close_tabs(&id(1), &ask, &tabs[1..]).unwrap(),
        CloseTabsResult::Closed { .. }
    ));
}

#[test]
fn 画面が未保存と知っているエディタはラストの印がまだでも確認に出す() {
    let mut ws = workspace_with(1);
    let doc = open(&mut ws, 1, "draft.md");
    // Rust の印はまだ立っていない(set_document_dirty が届く前)
    let mut ask = request(&[], std::slice::from_ref(&doc));
    ask.dirty_editors = vec![doc.clone()];
    let result = ws.close_tabs(&id(1), &ask, &[]).unwrap();
    let CloseTabsResult::NeedsConfirmation { unsaved, .. } = &result else {
        panic!("確認を求めなかった: {result:?}");
    };
    assert_eq!(unsaved[0].editor_id, doc);
    assert!(ws.editor(&doc).is_some());
    // 閉じないエディタの知らせは見ない
    let other = open(&mut ws, 1, "other.md");
    let mut ask = request(&[], std::slice::from_ref(&other));
    ask.dirty_editors = vec![doc.clone()];
    assert!(matches!(
        ws.close_tabs(&id(1), &ask, &[]).unwrap(),
        CloseTabsResult::Closed { .. }
    ));
}

#[test]
fn ほかのタブを閉じるでエディタを残すとそのエディタを出す() {
    let (mut ws, tabs) = three_terminals();
    let a = open(&mut ws, 1, "a.md");
    let b = open(&mut ws, 1, "b.md");
    // ターミナルを表示中にしてから、a 以外を閉じる
    ws.activate_terminal(&tabs[0]).unwrap();
    ws.close_tabs(&id(1), &request(&tabs, &[b]), &[]).unwrap();
    assert!(terminal_ids(&ws, 1).is_empty());
    assert_eq!(ws.active_tab(), Some(ActiveTab::Editor(a)));
}

#[test]
fn 表示中のエディタを閉じてターミナルだけが残ると表示中のターミナルを出す() {
    let (mut ws, tabs) = three_terminals();
    ws.activate_terminal(&tabs[1]).unwrap();
    let a = open(&mut ws, 1, "a.md");
    let b = open(&mut ws, 1, "b.md");
    assert_eq!(ws.active_tab(), Some(ActiveTab::Editor(b.clone())));
    // ターミナル 3 枚の右(エディタ 2 つ)を閉じる
    ws.close_tabs(&id(1), &request(&[], &[a, b]), &[]).unwrap();
    assert!(editor_ids(&ws, 1).is_empty());
    assert_eq!(ws.active_tab(), Some(ActiveTab::Terminal(tabs[1].clone())));
}

#[test]
fn ほかのタブを閉じるでターミナルだけを残すとそのターミナルを出す() {
    let (mut ws, tabs) = three_terminals();
    let a = open(&mut ws, 1, "a.md");
    // エディタを表示中のまま、t2 以外を閉じる(表示中のターミナルは t3 で、それも閉じる)
    assert_eq!(ws.active_tab(), Some(ActiveTab::Editor(a.clone())));
    let others = [tabs[0].clone(), tabs[2].clone()];
    ws.close_tabs(&id(1), &request(&others, &[a]), &[]).unwrap();
    assert_eq!(terminal_ids(&ws, 1), [tabs[1].clone()]);
    assert_eq!(ws.active_tab(), Some(ActiveTab::Terminal(tabs[1].clone())));
}

#[test]
fn ほかのプロジェクトのタブと無いタブは無視し無いプロジェクトは失敗() {
    let mut ws = workspace_with(2);
    let other = terminal_ids(&ws, 2);
    let gone = TerminalId::new("terminal-99");
    let result = ws
        .close_tabs(&id(1), &request(&[other[0].clone(), gone], &[]), &[])
        .unwrap();
    assert_eq!(result, CloseTabsResult::Closed { terminals: vec![] });
    assert_eq!(terminal_ids(&ws, 2), other);
    assert_eq!(
        ws.close_tabs(&id(9), &CloseTabsRequest::default(), &[]),
        Err(ProjectError::NotFound(id(9)))
    );
}

#[test]
fn 表示中のタブを閉じると残ったうちの隣が表示中になる() {
    let (mut ws, tabs) = three_terminals();
    // 表示中は最後に開いた t3。t2 と t3 を閉じると t1
    ws.close_tabs(&id(1), &request(&tabs[1..], &[]), &[])
        .unwrap();
    assert_eq!(ws.active_tab(), Some(ActiveTab::Terminal(tabs[0].clone())));
}
