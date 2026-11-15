//! `Workspace` のプロジェクトのコマンドの口(計画書 T33c)。
//! 登録(FR-001・FR-010。登録したら選んでタブを 1 枚開く=【仮定】6)、色の変更(FR-059)、
//! 再リンク(FR-007。タブを開かない=【仮定】6)、絶対パスの取得(FR-064。選択を変えない)、削除の応答の形(FR-003)。

use std::path::{Path, PathBuf};

use arin_core::persistence::codec::{PersistedLayout, PersistedState};
use arin_core::project::{Project, ProjectColor, ProjectError, ProjectId, RootReference};
use arin_core::terminal::TerminalId;
use arin_core::workspace::{
    AddProjectOutcome, ConnectPlan, RemoveOutcome, RemoveProjectOutcome, RootResolution,
    RunningTerminal, Startup, Workspace, WorkspaceError,
};

const HOME: &str = "/Users/me";

fn id(n: u8) -> ProjectId {
    ProjectId::from_random_bytes([n; 16])
}

fn root(path: &str) -> RootReference {
    RootReference::new(PathBuf::from(path), Some(format!("bm:{path}")))
}

fn empty() -> Workspace {
    Workspace::restore(
        "1.2.3",
        PathBuf::from(HOME),
        [("HOME", HOME)],
        Startup::default(),
    )
}

/// A(見つかる)と B(見つからない)を持ち、A を選んだ状態。
fn with_found_and_missing() -> Workspace {
    let a = Project::restore(id(1), root("/Users/me/dev/a"), None, ProjectColor::Red);
    let b = Project::restore(id(2), root("/Users/me/dev/b"), None, ProjectColor::Orange);
    let state = PersistedState {
        projects: vec![a.clone(), b],
        selected_project_id: Some(id(1)),
        layout: PersistedLayout::default(),
    };
    Workspace::restore(
        "1.2.3",
        PathBuf::from(HOME),
        [("HOME", HOME)],
        Startup {
            state,
            roots: vec![
                (id(1), RootResolution::Found(a.root().clone())),
                (id(2), RootResolution::Missing),
            ],
            notice: None,
        },
    )
}

fn tabs_of(ws: &Workspace, project: &ProjectId) -> Vec<TerminalId> {
    ws.snapshot()
        .projects
        .into_iter()
        .find(|p| &p.id == project)
        .map(|p| p.terminal_ids)
        .unwrap_or_default()
}

fn cwd_of(ws: &mut Workspace, terminal: &TerminalId) -> PathBuf {
    match ws.plan_connect(terminal, Path::new("/bin/zsh"), None) {
        Ok(ConnectPlan::Spawn(spec)) => spec.cwd().to_path_buf(),
        other => panic!("起動の計画にならなかった: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 登録
// ---------------------------------------------------------------------------

#[test]
fn 登録すると末尾に増えて選ばれ_ルートを_cwd_にしたタブが_1_枚開く() {
    let mut ws = empty();

    let outcome = ws
        .register_project(id(1), root("/Users/me/dev/app"))
        .expect("登録できなかった");

    assert_eq!(outcome, AddProjectOutcome::Added { project_id: id(1) });
    let s = ws.snapshot();
    assert_eq!(s.projects.len(), 1);
    assert_eq!(s.selected_project_id, Some(id(1)));
    let tabs = tabs_of(&ws, &id(1));
    assert_eq!(tabs.len(), 1, "タブが 1 枚開いていない");
    assert_eq!(s.projects[0].active_terminal_id.as_ref(), Some(&tabs[0]));
    assert_eq!(
        cwd_of(&mut ws, &tabs[0]),
        PathBuf::from("/Users/me/dev/app")
    );
}

#[test]
fn 同じフォルダを登録すると増えずタブも開かず既存が選ばれる() {
    let mut ws = empty();
    ws.register_project(id(1), root("/Users/me/dev/app"))
        .expect("登録できなかった");
    ws.register_project(id(2), root("/Users/me/dev/other"))
        .expect("登録できなかった");

    let outcome = ws
        .register_project(id(3), root("/Users/me/dev/app/"))
        .expect("既存の選択は失敗にしない");

    assert_eq!(
        outcome,
        AddProjectOutcome::AlreadyRegistered { project_id: id(1) }
    );
    let s = ws.snapshot();
    assert_eq!(s.projects.len(), 2);
    assert_eq!(s.selected_project_id, Some(id(1)));
    assert_eq!(
        tabs_of(&ws, &id(1)).len(),
        1,
        "既存のプロジェクトにタブが増えた"
    );
}

#[test]
fn 入れ子のフォルダは別のプロジェクトとして登録できる() {
    let mut ws = empty();
    ws.register_project(id(1), root("/Users/me/dev/app"))
        .expect("登録できなかった");

    let outcome = ws
        .register_project(id(2), root("/Users/me/dev/app/frontend"))
        .expect("登録できなかった");

    assert_eq!(outcome, AddProjectOutcome::Added { project_id: id(2) });
    assert_eq!(ws.snapshot().projects.len(), 2);
}

#[test]
fn 同じ_id_は登録できず何も変わらない() {
    let mut ws = empty();
    ws.register_project(id(1), root("/Users/me/dev/a"))
        .expect("登録できなかった");
    let before = ws.snapshot();

    let error = ws
        .register_project(id(1), root("/Users/me/dev/b"))
        .expect_err("同じ ID で登録できた");

    assert_eq!(
        error,
        WorkspaceError::Project(ProjectError::DuplicateId(id(1)))
    );
    assert_eq!(ws.snapshot(), before);
}

#[test]
fn ゴミ箱の中のフォルダは登録できない() {
    let mut ws = empty();

    let error = ws
        .register_project(id(1), root("/Users/me/.Trash/app"))
        .expect_err("ゴミ箱の中を登録できた");

    assert!(matches!(
        error,
        WorkspaceError::Project(ProjectError::InTrash(_))
    ));
    assert!(ws.snapshot().projects.is_empty());
    assert!(ws.snapshot().terminals.is_empty());
}

#[test]
fn 登録したプロジェクトは保存する内容に入る() {
    let mut ws = empty();
    ws.register_project(id(1), root("/Users/me/dev/app"))
        .expect("登録できなかった");

    let saved = ws.persisted_state();

    assert_eq!(saved.projects.len(), 1);
    assert_eq!(saved.projects[0].id(), &id(1));
    assert_eq!(
        saved.projects[0].root().bookmark(),
        Some("bm:/Users/me/dev/app")
    );
    assert_eq!(saved.selected_project_id, Some(id(1)));
}

// ---------------------------------------------------------------------------
// 色の変更・パスの取得
// ---------------------------------------------------------------------------

#[test]
fn 色を変えると写しと保存する内容に出る() {
    let mut ws = with_found_and_missing();

    ws.set_project_color(&id(1), ProjectColor::Teal)
        .expect("色を変えられなかった");

    assert_eq!(ws.snapshot().projects[0].color, ProjectColor::Teal);
    let saved = ws.persisted_state();
    assert_eq!(saved.projects[0].color(), ProjectColor::Teal);
}

#[test]
fn 無い_id_の色は変えられない() {
    let mut ws = with_found_and_missing();

    assert_eq!(
        ws.set_project_color(&id(9), ProjectColor::Teal),
        Err(ProjectError::NotFound(id(9)))
    );
}

#[test]
fn ルートの絶対パスを返し選択を変えない() {
    let ws = with_found_and_missing();

    assert_eq!(
        ws.project_root_path(&id(2)),
        Ok(PathBuf::from("/Users/me/dev/b"))
    );
    assert_eq!(ws.snapshot().selected_project_id, Some(id(1)));
    assert_eq!(
        ws.project_root_path(&id(9)),
        Err(ProjectError::NotFound(id(9)))
    );
}

// ---------------------------------------------------------------------------
// 再リンク
// ---------------------------------------------------------------------------

#[test]
fn 再リンクすると_id_と色を保ち見つからないが解け_タブは開かない() {
    let mut ws = with_found_and_missing();

    ws.relink_project(&id(2), root("/Users/me/dev/b2"))
        .expect("再リンクできなかった");

    let s = ws.snapshot();
    let b = s.projects.iter().find(|p| p.id == id(2)).expect("B が無い");
    assert!(!b.missing);
    assert_eq!(b.color, ProjectColor::Orange);
    assert_eq!(b.display_path, "~/dev/b2");
    assert!(b.terminal_ids.is_empty(), "再リンクでタブが開いた");
    assert_eq!(
        s.selected_project_id,
        Some(id(1)),
        "再リンクで選択が変わった"
    );
    // 0 枚から開くタブはつなぎ直した先で始まる。
    let opened = ws.open_terminal(&id(2)).expect("タブを開けなかった");
    assert_eq!(cwd_of(&mut ws, &opened), PathBuf::from("/Users/me/dev/b2"));
}

#[test]
fn 再リンクするとまだ起動していないタブはつなぎ直した先で始まる() {
    let mut ws = with_found_and_missing();
    let tab = tabs_of(&ws, &id(1))[0].clone();

    ws.relink_project(&id(1), root("/Users/me/dev/a2"))
        .expect("再リンクできなかった");

    assert_eq!(cwd_of(&mut ws, &tab), PathBuf::from("/Users/me/dev/a2"));
}

#[test]
fn 再リンク先が別のプロジェクトのフォルダなら何も変えない() {
    let mut ws = with_found_and_missing();
    let before = ws.snapshot();

    let error = ws
        .relink_project(&id(2), root("/Users/me/dev/a"))
        .expect_err("別のプロジェクトのフォルダに再リンクできた");

    assert_eq!(
        error,
        WorkspaceError::Project(ProjectError::AlreadyRegistered(id(1)))
    );
    assert_eq!(ws.snapshot(), before);
}

// ---------------------------------------------------------------------------
// 削除の応答の形
// ---------------------------------------------------------------------------

#[test]
fn 削除の結果から画面への応答を作る() {
    let running = vec![RunningTerminal {
        terminal_id: TerminalId::new("terminal-1"),
        project_id: Some(id(1)),
        project_name: Some("a".into()),
        label: "ターミナル 1".into(),
    }];

    assert_eq!(
        RemoveProjectOutcome::from(&RemoveOutcome::NeedsConfirmation(running.clone())),
        RemoveProjectOutcome::NeedsConfirmation { running }
    );
    assert_eq!(
        RemoveProjectOutcome::from(&RemoveOutcome::Removed {
            terminals: vec![TerminalId::new("terminal-1")]
        }),
        RemoveProjectOutcome::Removed
    );
}

#[test]
fn 応答の_json_の形() {
    assert_eq!(
        serde_json::to_value(AddProjectOutcome::Added { project_id: id(1) }).unwrap(),
        serde_json::json!({ "kind": "added", "projectId": id(1).as_str() })
    );
    assert_eq!(
        serde_json::to_value(AddProjectOutcome::AlreadyRegistered { project_id: id(1) }).unwrap(),
        serde_json::json!({ "kind": "alreadyRegistered", "projectId": id(1).as_str() })
    );
    assert_eq!(
        serde_json::to_value(AddProjectOutcome::Cancelled).unwrap(),
        serde_json::json!({ "kind": "cancelled" })
    );
    assert_eq!(
        serde_json::to_value(RemoveProjectOutcome::Removed).unwrap(),
        serde_json::json!({ "kind": "removed" })
    );
    assert_eq!(
        serde_json::to_value(RemoveProjectOutcome::NeedsConfirmation { running: vec![] }).unwrap(),
        serde_json::json!({ "kind": "needsConfirmation", "running": [] })
    );
}
