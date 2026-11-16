//! `Workspace` のプロジェクトをまたぐ操作(計画書 T32b)。
//! 起動時の復元(FR-004、FR-039・C-22)、切替(FR-002・FR-042)、削除の確認(FR-003)、終了の確認の材料(FR-063)、
//! 保存する内容(FR-056。表示値を含めない)、ペインの寸法の規則(FR-009)。

use std::path::{Path, PathBuf};

use arin_core::layout::font_sizes::FontSizes;
use arin_core::layout::panes::PaneSizes;
use arin_core::layout::tiles::DisplayMode;
use arin_core::layout::translucency::Translucency;
use arin_core::persistence::codec::{self, PersistedLayout, PersistedState};
use arin_core::project::{Project, ProjectColor, ProjectError, ProjectId, RootReference};
use arin_core::terminal::{TerminalId, TerminalStatus};
use arin_core::workspace::{
    RemoveOutcome, RootResolution, RunningTerminal, Startup, Workspace, WorkspaceError,
};

const HOME: &str = "/Users/me";

fn id(n: u8) -> ProjectId {
    ProjectId::from_random_bytes([n; 16])
}

fn root(path: &str) -> RootReference {
    RootReference::new(PathBuf::from(path), Some(format!("bm:{path}")))
}

fn project(n: u8, path: &str, color: ProjectColor) -> Project {
    Project::restore(id(n), root(path), None, color)
}

/// A(`~/dev/a`)・B(`~/dev/b`)・C(`~/dev/c`)を登録し B を選んだ状態。
fn state_abc() -> PersistedState {
    PersistedState {
        projects: vec![
            project(1, "/Users/me/dev/a", ProjectColor::Red),
            project(2, "/Users/me/dev/b", ProjectColor::Orange),
            project(3, "/Users/me/dev/c", ProjectColor::Yellow),
        ],
        selected_project_id: Some(id(2)),
        layout: PersistedLayout::default(),
    }
}

/// 保存した場所のまま解決できた結果。
fn found_all(state: &PersistedState) -> Vec<(ProjectId, RootResolution)> {
    state
        .projects
        .iter()
        .map(|p| (p.id().clone(), RootResolution::Found(p.root().clone())))
        .collect()
}

fn restore(startup: Startup) -> Workspace {
    Workspace::restore("1.2.3", PathBuf::from(HOME), [("HOME", HOME)], startup)
}

fn restore_abc() -> Workspace {
    let state = state_abc();
    let roots = found_all(&state);
    restore(Startup {
        state,
        roots,
        notice: None,
    })
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
        Ok(arin_core::workspace::ConnectPlan::Spawn(spec)) => spec.cwd().to_path_buf(),
        other => panic!("起動の計画にならなかった: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 起動時の復元
// ---------------------------------------------------------------------------

#[test]
fn 状態が無ければプロジェクトもターミナルも無い() {
    let ws = restore(Startup::default());

    let snapshot = ws.snapshot();
    assert!(snapshot.projects.is_empty());
    assert!(snapshot.terminals.is_empty());
    assert_eq!(snapshot.selected_project_id, None);
    assert_eq!(snapshot.layout, PaneSizes::default());
    assert_eq!(snapshot.notice, None);
}

#[test]
fn 起動時に解決できたプロジェクトだけに_1_タブずつ登録し_cwd_はルート() {
    let state = state_abc();
    let mut roots = found_all(&state);
    roots[1].1 = RootResolution::Missing;
    let mut ws = restore(Startup {
        state,
        roots,
        notice: None,
    });

    let snapshot = ws.snapshot();
    assert_eq!(snapshot.terminals.len(), 2);
    let a = tabs_of(&ws, &id(1));
    let c = tabs_of(&ws, &id(3));
    assert_eq!(a.len(), 1);
    assert_eq!(c.len(), 1);
    assert!(tabs_of(&ws, &id(2)).is_empty());
    // 表示中のタブはその 1 枚。
    assert_eq!(snapshot.projects[0].active_terminal_id, Some(a[0].clone()));
    // ターミナルは起動していない(PTY は最初の接続で起動する。設計書 §7.1「起動」5)。
    assert!(
        snapshot
            .terminals
            .iter()
            .all(|t| t.status == TerminalStatus::NotStarted)
    );
    assert_eq!(cwd_of(&mut ws, &a[0]), PathBuf::from("/Users/me/dev/a"));
    assert_eq!(cwd_of(&mut ws, &c[0]), PathBuf::from("/Users/me/dev/c"));
}

#[test]
fn ルートが移動していたら新しいパスで開き_表示用のパスも新しくなる() {
    let state = state_abc();
    let mut roots = found_all(&state);
    roots[0].1 = RootResolution::Found(root("/Users/me/work/a-renamed"));
    let mut ws = restore(Startup {
        state,
        roots,
        notice: None,
    });

    let a = tabs_of(&ws, &id(1));
    assert_eq!(
        cwd_of(&mut ws, &a[0]),
        PathBuf::from("/Users/me/work/a-renamed")
    );
    let card = &ws.snapshot().projects[0];
    assert_eq!(card.display_path, "~/work/a-renamed");
    assert_eq!(card.name, "a-renamed");
    assert_eq!(card.color, ProjectColor::Red);
    // 保存する内容も新しい参照になる。
    let saved = ws.persisted_state();
    assert_eq!(saved.projects[0].root(), &root("/Users/me/work/a-renamed"));
}

#[test]
fn 見つからないプロジェクトにはタブを作らず_新しいタブも開かない() {
    let state = state_abc();
    let mut roots = found_all(&state);
    roots[1].1 = RootResolution::Missing;
    let mut ws = restore(Startup {
        state,
        roots,
        notice: None,
    });

    let err = ws.open_terminal(&id(2)).unwrap_err();

    assert_eq!(err, WorkspaceError::RootMissing(id(2)));
    assert!(tabs_of(&ws, &id(2)).is_empty());
    let card = &ws.snapshot().projects[1];
    assert!(card.missing);
    assert_eq!(card.active_terminal_id, None);
}

#[test]
fn 解決の結果が無いものとゴミ箱の中に解決されたものは見つからない扱い() {
    let state = state_abc();
    let roots = vec![
        (id(1), RootResolution::Found(root("/Users/me/.Trash/a"))),
        (id(3), RootResolution::Found(root("/Users/me/dev/c"))),
    ];
    let ws = restore(Startup {
        state,
        roots,
        notice: None,
    });

    let missing: Vec<bool> = ws.snapshot().projects.iter().map(|p| p.missing).collect();
    assert_eq!(missing, vec![true, true, false]);
    assert_eq!(ws.snapshot().terminals.len(), 1);
    // ゴミ箱の中のパスで保存し直さない。
    assert_eq!(
        ws.persisted_state().projects[0].root(),
        &root("/Users/me/dev/a")
    );
}

#[test]
fn 別のプロジェクトと同じフォルダに解決されたものは見つからない扱い() {
    let state = state_abc();
    let mut roots = found_all(&state);
    roots[2].1 = RootResolution::Found(root("/Users/me/dev/a"));
    let ws = restore(Startup {
        state,
        roots,
        notice: None,
    });

    let snapshot = ws.snapshot();
    assert!(snapshot.projects[2].missing);
    assert_eq!(snapshot.terminals.len(), 2);
}

#[test]
fn 復元すると同じ順で同じものが選択される() {
    let ws = restore_abc();

    // AC-P04 の Rust 側: 保存した内容から起動し直しても、同じ順で B が選択されている。
    let saved = ws.persisted_state();
    let text = codec::encode(&saved).unwrap();
    let codec::LoadOutcome::Loaded(loaded) = codec::decode(Some(text.as_bytes())) else {
        panic!("保存した内容を読めなかった");
    };
    let roots = found_all(&loaded.state);
    let again = restore(Startup {
        state: loaded.state,
        roots,
        notice: None,
    });

    let snapshot = again.snapshot();
    let order: Vec<&str> = snapshot.projects.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(order, vec!["a", "b", "c"]);
    assert_eq!(snapshot.selected_project_id, Some(id(2)));
    assert_eq!(again.persisted_state(), saved);
}

#[test]
fn ペインの寸法は保存値に規則を当て_無ければ初期値() {
    let mut state = state_abc();
    state.layout = PersistedLayout {
        sidebar_width: Some(10.0),
        sidebar_collapsed: true,
        card_area_height: None,
        display_mode: DisplayMode::Tabs,
        font_sizes: FontSizes::default(),
        translucency: Translucency::default(),
        editor_vim: false,
    };
    let ws = restore(Startup {
        state,
        roots: Vec::new(),
        notice: None,
    });

    let layout = ws.snapshot().layout;
    assert_eq!(
        layout,
        PaneSizes {
            sidebar_width: 180.0,
            sidebar_collapsed: true,
            card_area_height: PaneSizes::default().card_area_height,
        }
    );
    assert_eq!(
        ws.persisted_state().layout,
        PersistedLayout {
            sidebar_width: Some(180.0),
            sidebar_collapsed: true,
            card_area_height: Some(200.0),
            display_mode: DisplayMode::Tabs,
            font_sizes: FontSizes::default(),
            translucency: Translucency::default(),
            editor_vim: false,
        }
    );
}

#[test]
fn 退避の知らせは写しに載り_保存する内容には入らない() {
    let ws = restore(Startup {
        notice: Some("state.json を読めなかったので退避した".to_owned()),
        ..Startup::default()
    });

    assert_eq!(
        ws.snapshot().notice.as_deref(),
        Some("state.json を読めなかったので退避した")
    );
    assert_eq!(
        ws.persisted_state(),
        PersistedState {
            projects: Vec::new(),
            selected_project_id: None,
            layout: PersistedLayout {
                sidebar_width: Some(260.0),
                sidebar_collapsed: false,
                card_area_height: Some(200.0),
                display_mode: DisplayMode::Tabs,
                font_sizes: FontSizes::default(),
                translucency: Translucency::default(),
                editor_vim: false,
            },
        }
    );
}

// ---------------------------------------------------------------------------
// 切替(FR-002・FR-042)
// ---------------------------------------------------------------------------

#[test]
fn 選択を変えてもターミナルの状態は変わらない() {
    let mut ws = restore_abc();
    let a = tabs_of(&ws, &id(1))[0].clone();
    ws.mark_running(&a).unwrap();
    let before = ws.snapshot().terminals;

    ws.select_project(&id(1)).unwrap();
    ws.select_project(&id(3)).unwrap();

    let after = ws.snapshot();
    assert_eq!(after.selected_project_id, Some(id(3)));
    assert_eq!(after.terminals, before);
    assert_eq!(ws.check_input(&a), Ok(()));
}

#[test]
fn 登録の無いプロジェクトは選択できない() {
    let mut ws = restore_abc();

    assert_eq!(
        ws.select_project(&id(9)),
        Err(ProjectError::NotFound(id(9)))
    );
    assert_eq!(ws.snapshot().selected_project_id, Some(id(2)));
}

#[test]
fn 前後のタブの切替は選択中のプロジェクトだけに効く() {
    let mut ws = restore_abc();
    let b1 = tabs_of(&ws, &id(2))[0].clone();
    let b2 = ws.open_terminal(&id(2)).unwrap();
    let a1 = tabs_of(&ws, &id(1))[0].clone();

    assert_eq!(ws.select_next_terminal(), Some(b1.clone()));
    assert_eq!(ws.select_previous_terminal(), Some(b2));
    let snapshot = ws.snapshot();
    assert_eq!(snapshot.projects[0].active_terminal_id, Some(a1));
}

// ---------------------------------------------------------------------------
// 削除(FR-003)
// ---------------------------------------------------------------------------

#[test]
fn 動作中があれば削除は確認を求め_確認なしでは何も変えない() {
    let mut ws = restore_abc();
    let b = tabs_of(&ws, &id(2))[0].clone();
    let a = tabs_of(&ws, &id(1))[0].clone();
    let before = ws.snapshot();

    // 動作中の一覧には別のプロジェクトのターミナルも混ざってよい(そのプロジェクトの分だけを見る)。
    let outcome = ws.remove_project(&id(2), &[a, b.clone()], false).unwrap();

    assert_eq!(
        outcome,
        RemoveOutcome::NeedsConfirmation {
            running: vec![RunningTerminal {
                terminal_id: b,
                project_id: Some(id(2)),
                project_name: Some("b".to_owned()),
                label: "ターミナル 1".to_owned(),
            }],
            unsaved: vec![],
        }
    );
    assert_eq!(ws.snapshot(), before);
}

#[test]
fn 確認して削除するとそのプロジェクトのタブが閉じる対象として返る() {
    let mut ws = restore_abc();
    let b1 = tabs_of(&ws, &id(2))[0].clone();
    let b2 = ws.open_terminal(&id(2)).unwrap();

    let outcome = ws
        .remove_project(&id(2), std::slice::from_ref(&b1), true)
        .unwrap();

    assert_eq!(
        outcome,
        RemoveOutcome::Removed {
            terminals: vec![b1.clone(), b2.clone()]
        }
    );
    let snapshot = ws.snapshot();
    let names: Vec<&str> = snapshot.projects.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["a", "c"]);
    // 選択は次のカードへ(【仮定】17)。
    assert_eq!(snapshot.selected_project_id, Some(id(3)));
    assert!(snapshot.terminals.iter().all(|t| t.id != b1 && t.id != b2));
    assert_eq!(snapshot.terminals.len(), 2);
    assert!(ws.check_exists(&b1).is_err());
    // 保存する内容からも消える。
    assert_eq!(ws.persisted_state().projects.len(), 2);
}

#[test]
fn 動作中が無ければすぐ削除できる() {
    let mut ws = restore_abc();
    let a = tabs_of(&ws, &id(1))[0].clone();

    let outcome = ws.remove_project(&id(3), &[a], false).unwrap();

    assert!(matches!(outcome, RemoveOutcome::Removed { terminals } if terminals.len() == 1));
    assert_eq!(ws.snapshot().projects.len(), 2);
}

#[test]
fn 見つからないプロジェクトはタブが無いのですぐ削除できる() {
    let state = state_abc();
    let ws_roots = Vec::new();
    let mut ws = restore(Startup {
        state,
        roots: ws_roots,
        notice: None,
    });

    let outcome = ws.remove_project(&id(1), &[], false).unwrap();

    assert_eq!(
        outcome,
        RemoveOutcome::Removed {
            terminals: Vec::new()
        }
    );
}

#[test]
fn 登録の無いプロジェクトは削除できない() {
    let mut ws = restore_abc();

    assert_eq!(
        ws.remove_project(&id(9), &[], true),
        Err(ProjectError::NotFound(id(9)))
    );
}

// ---------------------------------------------------------------------------
// 終了の確認の材料(FR-063)
// ---------------------------------------------------------------------------

#[test]
fn 終了の確認の材料は動作中のターミナルのプロジェクト名とタブ名() {
    let mut ws = restore_abc();
    let a1 = tabs_of(&ws, &id(1))[0].clone();
    let c1 = tabs_of(&ws, &id(3))[0].clone();
    let a2 = ws.open_terminal(&id(1)).unwrap();

    // 渡した順ではなく、一覧の順・タブの順で返す。一覧に無い ID は無視する。
    let running = ws.running_terminals(&[c1.clone(), TerminalId::new("gone"), a2.clone()]);

    assert_eq!(
        running,
        vec![
            RunningTerminal {
                terminal_id: a2,
                project_id: Some(id(1)),
                project_name: Some("a".to_owned()),
                label: "ターミナル 2".to_owned(),
            },
            RunningTerminal {
                terminal_id: c1,
                project_id: Some(id(3)),
                project_name: Some("c".to_owned()),
                label: "ターミナル 1".to_owned(),
            },
        ]
    );
    assert!(ws.running_terminals(&[]).is_empty());
    assert!(running.iter().all(|r| r.terminal_id != a1));
}

#[test]
fn 動作中の一覧の_json_は_camel_case() {
    let item = RunningTerminal {
        terminal_id: TerminalId::new("terminal-3"),
        project_id: None,
        project_name: None,
        label: "ターミナル 3".to_owned(),
    };

    assert_eq!(
        serde_json::to_value(&item).unwrap(),
        serde_json::json!({
            "terminalId": "terminal-3",
            "projectId": null,
            "projectName": null,
            "label": "ターミナル 3",
        })
    );
}

// ---------------------------------------------------------------------------
// 保存する内容(FR-056)
// ---------------------------------------------------------------------------

#[test]
fn 保存する内容に表示値_見つからない_表示用のパス_を含めない() {
    let state = state_abc();
    let mut roots = found_all(&state);
    roots[0].1 = RootResolution::Missing;
    let ws = restore(Startup {
        state: state.clone(),
        roots,
        notice: Some("知らせ".to_owned()),
    });

    let saved = ws.persisted_state();

    // 見つからないものも一覧に残し、保存値はそのまま(読んだとおりの参照・色・順)。
    assert_eq!(saved.projects, state.projects);
    assert!(saved.projects.iter().all(|p| !p.is_missing()));
    let text = codec::encode(&saved).unwrap();
    for display_only in ["missing", "displayPath", "知らせ", "notice", "terminal"] {
        assert!(
            !text.contains(display_only),
            "保存する内容に {display_only} が入った: {text}"
        );
    }
}

// ---------------------------------------------------------------------------
// 隠しファイル表示(計画書 T38e、FR-017、【仮定】3)
// ---------------------------------------------------------------------------

fn show_hidden_of(ws: &Workspace, project: &ProjectId) -> bool {
    ws.snapshot()
        .projects
        .into_iter()
        .find(|p| &p.id == project)
        .map(|p| p.show_hidden)
        .expect("写しにプロジェクトがある")
}

#[test]
fn 隠しファイル表示の既定はオフ() {
    let mut ws = restore_abc();
    ws.register_project(id(9), root("/Users/me/dev/new"))
        .expect("登録できる");

    for n in [1, 2, 3, 9] {
        assert!(!show_hidden_of(&ws, &id(n)), "プロジェクト {n}");
    }
}

#[test]
fn 隠しファイル表示を変えると写しが変わる() {
    let mut ws = restore_abc();
    let before = ws.snapshot();

    ws.set_project_show_hidden(&id(1), true)
        .expect("変えられる");

    let after = ws.snapshot();
    assert_ne!(after, before);
    assert!(show_hidden_of(&ws, &id(1)));
    // ほかのプロジェクトと選択は変わらない
    assert!(!show_hidden_of(&ws, &id(2)));
    assert!(!show_hidden_of(&ws, &id(3)));
    assert_eq!(after.selected_project_id, Some(id(2)));

    ws.set_project_show_hidden(&id(1), false).expect("戻せる");
    assert_eq!(ws.snapshot(), before);
}

#[test]
fn 隠しファイル表示は保存する内容に載り起動時に戻る() {
    let mut ws = restore_abc();
    ws.set_project_show_hidden(&id(3), true).unwrap();

    let saved = ws.persisted_state();
    let flags: Vec<bool> = saved.projects.iter().map(Project::show_hidden).collect();
    assert_eq!(flags, [false, false, true]);

    // 書いて読んで起動し直しても保たれる
    let text = codec::encode(&saved).unwrap();
    let codec::LoadOutcome::Loaded(loaded) = codec::decode(Some(text.as_bytes())) else {
        panic!("読めるはず: {text}");
    };
    let roots = found_all(&loaded.state);
    let ws = restore(Startup {
        state: loaded.state,
        roots,
        notice: None,
    });
    assert!(show_hidden_of(&ws, &id(3)));
    assert!(!show_hidden_of(&ws, &id(1)));
}

#[test]
fn 見つからないプロジェクトも隠しファイル表示を保つ() {
    let state = PersistedState {
        projects: vec![project(1, "/Users/me/dev/a", ProjectColor::Red).with_show_hidden(true)],
        ..PersistedState::default()
    };
    let roots = vec![(id(1), RootResolution::Missing)];
    let ws = restore(Startup {
        state,
        roots,
        notice: None,
    });

    assert!(show_hidden_of(&ws, &id(1)));
    assert!(ws.persisted_state().projects[0].show_hidden());
}

#[test]
fn 登録の無いプロジェクトの隠しファイル表示は変えられない() {
    let mut ws = restore_abc();
    let before = ws.snapshot();

    assert_eq!(
        ws.set_project_show_hidden(&id(7), true),
        Err(ProjectError::NotFound(id(7)))
    );
    assert_eq!(ws.snapshot(), before);
}
