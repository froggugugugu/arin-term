//! `Workspace` のルートへの追従(見回りの口。計画書 T33d、FR-007、設計書 §7.1「ルートフォルダへの追従」)と、
//! ペインの寸法の setter(FR-009。規則は `PaneSizes`)。

use std::path::{Path, PathBuf};

use arin_core::layout::panes::{MIN_CARD_AREA_HEIGHT, MIN_SIDEBAR_WIDTH};
use arin_core::persistence::codec::{PersistedLayout, PersistedState};
use arin_core::project::{Project, ProjectColor, ProjectError, ProjectId, RootReference};
use arin_core::terminal::TerminalId;
use arin_core::workspace::{ConnectPlan, RootResolution, Startup, Workspace, WorkspaceError};

const HOME: &str = "/Users/me";
const A: &str = "/Users/me/dev/a";
const B: &str = "/Users/me/dev/b";

fn id(n: u8) -> ProjectId {
    ProjectId::from_random_bytes([n; 16])
}

fn root(path: &str) -> RootReference {
    RootReference::new(PathBuf::from(path), Some(format!("bm:{path}")))
}

/// A(見つかる。選択中)と B(見つからない)。
fn workspace() -> Workspace {
    let a = Project::restore(id(1), root(A), None, ProjectColor::Red);
    let b = Project::restore(id(2), root(B), None, ProjectColor::Orange);
    Workspace::restore(
        "1.2.3",
        PathBuf::from(HOME),
        [("HOME", HOME)],
        Startup {
            state: PersistedState {
                projects: vec![a.clone(), b],
                selected_project_id: Some(id(1)),
                layout: PersistedLayout::default(),
            },
            roots: vec![
                (id(1), RootResolution::Found(a.root().clone())),
                (id(2), RootResolution::Missing),
            ],
            notice: None,
        },
    )
}

fn card(ws: &Workspace, project: &ProjectId) -> arin_core::workspace::ProjectCardSnapshot {
    ws.snapshot()
        .projects
        .into_iter()
        .find(|p| &p.id == project)
        .expect("カードが無い")
}

fn first_tab(ws: &Workspace, project: &ProjectId) -> TerminalId {
    card(ws, project).terminal_ids[0].clone()
}

fn cwd_of(ws: &mut Workspace, terminal: &TerminalId) -> PathBuf {
    match ws.plan_connect(terminal, Path::new("/bin/zsh"), None) {
        Ok(ConnectPlan::Spawn(spec)) => spec.cwd().to_path_buf(),
        other => panic!("起動の計画にならなかった: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 見回りの口
// ---------------------------------------------------------------------------

#[test]
fn 見回りの対象は全プロジェクトのルートと見つからないかどうか() {
    let ws = workspace();

    let roots = ws.project_roots();

    assert_eq!(roots.len(), 2);
    assert_eq!(roots[0].id, id(1));
    assert_eq!(roots[0].root, root(A));
    assert!(!roots[0].missing);
    assert_eq!(roots[1].id, id(2));
    assert!(roots[1].missing);
}

#[test]
fn 追従すると同じプロジェクトのまま表示用のパスと名前が新しくなる() {
    let mut ws = workspace();
    let before = card(&ws, &id(1));

    let changed = ws
        .follow_project_root(&id(1), Path::new(A), root("/Users/me/dev/a2"))
        .expect("追従できなかった");

    assert!(changed);
    let after = card(&ws, &id(1));
    assert_eq!(after.id, before.id);
    assert_eq!(after.color, before.color);
    assert_eq!(after.terminal_ids, before.terminal_ids);
    assert_eq!(after.display_path, "~/dev/a2");
    assert_eq!(after.name, "a2");
    let saved = ws.persisted_state();
    assert_eq!(saved.projects[0].root(), &root("/Users/me/dev/a2"));
}

#[test]
fn 追従すると起動前のタブの_cwd_も新しいルートになる() {
    let mut ws = workspace();
    let tab = first_tab(&ws, &id(1));

    ws.follow_project_root(&id(1), Path::new(A), root("/Users/me/dev/a2"))
        .expect("追従できなかった");

    assert_eq!(cwd_of(&mut ws, &tab), PathBuf::from("/Users/me/dev/a2"));
}

#[test]
fn 同じ参照への追従は変わらなかったと返す() {
    let mut ws = workspace();

    let changed = ws
        .follow_project_root(&id(1), Path::new(A), root(A))
        .expect("追従できなかった");

    assert!(!changed);
}

#[test]
fn 調べたあとにルートが変わっていたら追従しない() {
    // 見回りが調べているあいだに本人が再リンクした場合。見回りの古い結果で上書きしない。
    let mut ws = workspace();
    ws.relink_project(&id(1), root("/Users/me/dev/elsewhere"))
        .expect("再リンクできなかった");

    let changed = ws
        .follow_project_root(&id(1), Path::new(A), root("/Users/me/dev/a2"))
        .expect("追従できなかった");

    assert!(!changed);
    assert_eq!(card(&ws, &id(1)).display_path, "~/dev/elsewhere");
}

#[test]
fn ゴミ箱へ移ったら見つからないになりルートは変えない() {
    let mut ws = workspace();

    let changed = ws
        .follow_project_root(&id(1), Path::new(A), root("/Users/me/.Trash/a"))
        .expect("追従できなかった");

    assert!(changed);
    let after = card(&ws, &id(1));
    assert!(after.missing);
    assert_eq!(after.display_path, "~/dev/a");
    // 2 回目は何も変わらない(見回りが毎回写しを送らない)。
    assert!(
        !ws.follow_project_root(&id(1), Path::new(A), root("/Users/me/.Trash/a"))
            .expect("追従できなかった")
    );
}

#[test]
fn 見つからないプロジェクトが戻ると見つからないが解けタブは開かない() {
    let mut ws = workspace();
    assert!(card(&ws, &id(2)).terminal_ids.is_empty());

    let changed = ws
        .follow_project_root(&id(2), Path::new(B), root("/Users/me/dev/b-moved"))
        .expect("追従できなかった");

    assert!(changed);
    let after = card(&ws, &id(2));
    assert!(!after.missing);
    assert_eq!(after.display_path, "~/dev/b-moved");
    assert!(after.terminal_ids.is_empty(), "タブを開いた");
}

#[test]
fn 別のプロジェクトと同じフォルダへは追従せず何も変えない() {
    let mut ws = workspace();

    let result = ws.follow_project_root(&id(2), Path::new(B), root(A));

    assert_eq!(
        result,
        Err(WorkspaceError::Project(ProjectError::AlreadyRegistered(
            id(1)
        )))
    );
    assert!(card(&ws, &id(2)).missing);
    assert_eq!(card(&ws, &id(2)).display_path, "~/dev/b");
}

#[test]
fn 見つからないにするとカードとタブは残る() {
    let mut ws = workspace();
    let tabs = card(&ws, &id(1)).terminal_ids;

    let changed = ws
        .mark_project_missing(&id(1), Path::new(A))
        .expect("見つからないにできなかった");

    assert!(changed);
    let after = card(&ws, &id(1));
    assert!(after.missing);
    assert_eq!(after.terminal_ids, tabs);
    // 保存する内容は変わらない(見つからないは表示値)。
    assert_eq!(ws.persisted_state().projects[0].root(), &root(A));
}

#[test]
fn 既に見つからないなら変わらなかったと返す() {
    let mut ws = workspace();

    assert!(
        !ws.mark_project_missing(&id(2), Path::new(B))
            .expect("見つからないにできなかった")
    );
}

#[test]
fn 調べたあとにルートが変わっていたら見つからないにしない() {
    let mut ws = workspace();

    let changed = ws
        .mark_project_missing(&id(1), Path::new("/Users/me/dev/old"))
        .expect("失敗した");

    assert!(!changed);
    assert!(!card(&ws, &id(1)).missing);
}

#[test]
fn 無いプロジェクトへの追従と見つからないは失敗する() {
    let mut ws = workspace();

    assert_eq!(
        ws.follow_project_root(&id(9), Path::new(A), root(A)),
        Err(WorkspaceError::Project(ProjectError::NotFound(id(9))))
    );
    assert_eq!(
        ws.mark_project_missing(&id(9), Path::new(A)),
        Err(ProjectError::NotFound(id(9)))
    );
}

// ---------------------------------------------------------------------------
// ペインの寸法
// ---------------------------------------------------------------------------

#[test]
fn 寸法を変えると写しと保存する内容に載る() {
    let mut ws = workspace();

    assert!(ws.set_pane_sizes(300.0, 250.0));

    let layout = ws.snapshot().layout;
    assert_eq!(layout.sidebar_width, 300.0);
    assert_eq!(layout.card_area_height, 250.0);
    let saved = ws.persisted_state().layout;
    assert_eq!(saved.sidebar_width, Some(300.0));
    assert_eq!(saved.card_area_height, Some(250.0));
}

#[test]
fn 最小を下回る寸法は最小になり_負の値は初期値になる() {
    let mut ws = workspace();

    ws.set_pane_sizes(10.0, 1.0);
    let layout = ws.snapshot().layout;
    assert_eq!(layout.sidebar_width, MIN_SIDEBAR_WIDTH);
    assert_eq!(layout.card_area_height, MIN_CARD_AREA_HEIGHT);

    ws.set_pane_sizes(-5.0, f64::NAN);
    let layout = ws.snapshot().layout;
    assert_eq!(layout.sidebar_width, 260.0);
    assert_eq!(layout.card_area_height, 200.0);
}

#[test]
fn 同じ寸法は変わらなかったと返す() {
    let mut ws = workspace();
    ws.set_pane_sizes(300.0, 250.0);

    assert!(!ws.set_pane_sizes(300.0, 250.0));
    // 直したあとの値が同じなら変わらない。
    ws.set_pane_sizes(10.0, 250.0);
    assert!(!ws.set_pane_sizes(20.0, 250.0));
}

#[test]
fn 折りたたみは幅を保ち_同じ値なら変わらなかったと返す() {
    let mut ws = workspace();
    ws.set_pane_sizes(300.0, 250.0);

    assert!(ws.set_sidebar_collapsed(true));
    assert!(!ws.set_sidebar_collapsed(true));
    let layout = ws.snapshot().layout;
    assert!(layout.sidebar_collapsed);
    assert_eq!(layout.sidebar_width, 300.0);
    assert!(ws.set_sidebar_collapsed(false));
    assert!(!ws.snapshot().layout.sidebar_collapsed);
}
