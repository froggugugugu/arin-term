//! `Workspace` が持つターミナルの状態と規則。
//! 設計書 §1.3 #2、§5.1、§6.1、§7.1「ターミナルの起動」、FR-039・FR-040・FR-043・FR-044、計画書 T32b。
//!
//! プロジェクトのタブ(通常の起動)と、プロジェクトを持たない 9 枚の検証画面(`for_verify_grid`)の両方を見る。

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use arin_core::persistence::codec::PersistedState;
use arin_core::ports::{ChildExit, PtySize};
use arin_core::project::{Project, ProjectColor, ProjectId, RootReference};
use arin_core::terminal::{
    ExitAction, ShellLaunchError, TerminalError, TerminalId, TerminalStatus, validate_size,
};
use arin_core::workspace::{
    ConnectPlan, RootResolution, SNAPSHOT_SCHEMA_VERSION, Startup, Workspace, WorkspaceError,
};
use serde_json::json;

const HOME: &str = "/Users/someone";
const ROOT: &str = "/Users/someone/dev/app";
const ZSH: &str = "/bin/zsh";

fn env() -> Vec<(&'static str, &'static str)> {
    vec![("HOME", HOME), ("USER", "someone"), ("SECRET_TOKEN", "x")]
}

fn project_id(n: u8) -> ProjectId {
    ProjectId::from_random_bytes([n; 16])
}

/// ルートが `ROOT` のプロジェクトを 1 件持ち、解決できた状態で起動した `Workspace`(タブは 1 枚)。
fn workspace() -> Workspace {
    let id = project_id(1);
    let root = RootReference::new(PathBuf::from(ROOT), Some("Ym9va21hcms=".to_owned()));
    Workspace::restore(
        "1.2.3",
        PathBuf::from(HOME),
        env(),
        Startup {
            state: PersistedState {
                projects: vec![Project::restore(
                    id.clone(),
                    root.clone(),
                    None,
                    ProjectColor::Green,
                )],
                selected_project_id: Some(id.clone()),
                ..PersistedState::default()
            },
            roots: vec![(id, RootResolution::Found(root))],
            notice: None,
        },
    )
}

fn grid() -> Workspace {
    Workspace::for_verify_grid("1.2.3", PathBuf::from(HOME), env())
}

fn only_id(ws: &Workspace) -> TerminalId {
    let snapshot = ws.snapshot();
    assert_eq!(snapshot.terminals.len(), 1, "ターミナルが 1 枚ではない");
    snapshot.terminals[0].id.clone()
}

fn status_of(ws: &Workspace, id: &TerminalId) -> Option<TerminalStatus> {
    ws.snapshot()
        .terminals
        .into_iter()
        .find(|t| &t.id == id)
        .map(|t| t.status)
}

fn ids(ws: &Workspace) -> Vec<TerminalId> {
    ws.snapshot().terminals.into_iter().map(|t| t.id).collect()
}

fn missing() -> TerminalId {
    TerminalId::new("no-such-terminal")
}

fn spawn_plan(ws: &mut Workspace, id: &TerminalId) -> arin_core::terminal::ShellLaunchSpec {
    match ws.plan_connect(id, Path::new(ZSH), Some("ja_JP")) {
        Ok(ConnectPlan::Spawn(spec)) => spec,
        other => panic!("起動の計画にならなかった: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 起動と接続
// ---------------------------------------------------------------------------

#[test]
fn 作った直後の写しにはまだ起動していないターミナルがプロジェクトのタブとして載る() {
    let ws = workspace();

    let snapshot = ws.snapshot();

    assert_eq!(snapshot.schema_version, SNAPSHOT_SCHEMA_VERSION);
    assert_eq!(snapshot.app_version, "1.2.3");
    assert_eq!(snapshot.terminals.len(), 1);
    let terminal = &snapshot.terminals[0];
    assert_eq!(terminal.status, TerminalStatus::NotStarted);
    assert_eq!(terminal.id.as_str(), "terminal-1");
    assert_eq!(terminal.project_id, Some(project_id(1)));
    assert_eq!(terminal.label, "ターミナル 1");
}

#[test]
fn 状態の_json_は種類を_kind_に持つ() {
    let cases = [
        (TerminalStatus::NotStarted, json!({ "kind": "notStarted" })),
        (TerminalStatus::Running, json!({ "kind": "running" })),
        (
            TerminalStatus::Exited {
                code: Some(0),
                signal: None,
            },
            json!({ "kind": "exited", "code": 0, "signal": null }),
        ),
        (
            TerminalStatus::Failed {
                message: "だめ".to_owned(),
            },
            json!({ "kind": "failed", "message": "だめ" }),
        ),
    ];
    for (status, expected) in cases {
        assert_eq!(serde_json::to_value(&status).unwrap(), expected);
        let back: TerminalStatus = serde_json::from_value(expected).unwrap();
        assert_eq!(back, status);
    }
}

#[test]
fn 最初の接続はプロジェクトのルートでログインシェルを起動する計画になる() {
    let mut ws = workspace();
    let id = only_id(&ws);

    let spec = spawn_plan(&mut ws, &id);

    assert_eq!(spec.program(), Path::new(ZSH));
    assert_eq!(spec.argv0(), OsStr::new("-zsh"));
    // FR-039: cwd はプロジェクトのルート。
    assert_eq!(spec.cwd(), Path::new(ROOT));
    assert_eq!(
        spec.env().get(OsStr::new("LANG")).map(|v| v.as_os_str()),
        Some(OsStr::new("ja_JP.UTF-8"))
    );
    // 親の環境変数は許可リストの名前だけが渡る。
    assert!(!spec.env().contains_key(OsStr::new("SECRET_TOKEN")));
    assert_eq!(
        spec.env().get(OsStr::new("USER")).map(|v| v.as_os_str()),
        Some(OsStr::new("someone"))
    );
    // 計画を立てただけでは状態を変えない(起動できてから running にする)。
    assert_eq!(status_of(&ws, &id), Some(TerminalStatus::NotStarted));
}

#[test]
fn 起動できたら_running_になり_2_回目の接続は付け替えになる() {
    let mut ws = workspace();
    let id = only_id(&ws);
    let _ = spawn_plan(&mut ws, &id);

    ws.mark_running(&id).unwrap();

    assert_eq!(status_of(&ws, &id), Some(TerminalStatus::Running));
    assert_eq!(
        ws.plan_connect(&id, Path::new(ZSH), None).unwrap(),
        ConnectPlan::Reattach
    );
}

#[test]
fn 起動に失敗したら_failed_になり_次の接続でもう一度起動を試す() {
    let mut ws = workspace();
    let id = only_id(&ws);

    ws.mark_failed(&id, "シェルが無い").unwrap();

    assert_eq!(
        status_of(&ws, &id),
        Some(TerminalStatus::Failed {
            message: "シェルが無い".to_owned()
        })
    );
    let _ = spawn_plan(&mut ws, &id);
}

#[test]
fn 起動の仕様を組み立てられなければ_failed_にして理由を返す() {
    let mut ws = workspace();
    let id = only_id(&ws);

    let err = ws
        .plan_connect(&id, Path::new("zsh"), None)
        .expect_err("相対パスのシェルで起動の計画になった");

    assert_eq!(
        err,
        TerminalError::Launch(ShellLaunchError::RelativeShell(PathBuf::from("zsh")))
    );
    assert!(
        matches!(status_of(&ws, &id), Some(TerminalStatus::Failed { message }) if message.contains("zsh"))
    );
}

#[test]
fn 検証画面でホームフォルダが分からなければ起動の計画は失敗する() {
    let mut ws =
        Workspace::for_verify_grid("1.2.3", PathBuf::new(), Vec::<(String, String)>::new());
    let id = ids(&ws)[0].clone();

    let err = ws.plan_connect(&id, Path::new(ZSH), None).unwrap_err();

    assert_eq!(err, TerminalError::Launch(ShellLaunchError::EmptyCwd));
}

// ---------------------------------------------------------------------------
// シェルの終了
// ---------------------------------------------------------------------------

#[test]
fn シェルが終わるとプロジェクトのタブは閉じ_写しから消える() {
    let mut ws = workspace();
    let id = only_id(&ws);
    ws.mark_running(&id).unwrap();

    let action = ws.mark_exited(
        &id,
        Some(ChildExit {
            exit_code: 0,
            signal: None,
        }),
    );

    assert!(
        matches!(&action, Some(ExitAction::Closed(tab)) if tab.id == id),
        "タブを閉じなかった: {action:?}"
    );
    let snapshot = ws.snapshot();
    assert!(snapshot.terminals.is_empty());
    assert!(snapshot.projects[0].terminal_ids.is_empty());
    assert_eq!(snapshot.projects[0].active_terminal_id, None);
    // 閉じたあとはどの操作でも見つからない。
    assert_eq!(ws.check_exists(&id), Err(TerminalError::NotFound(id)));
}

#[test]
fn 検証画面ではシェルが終わっても_exited_のまま残り終わり方を持つ() {
    let mut ws = grid();
    let id = ids(&ws)[0].clone();
    ws.mark_running(&id).unwrap();

    let action = ws.mark_exited(
        &id,
        Some(ChildExit {
            exit_code: 1,
            signal: Some("Hangup".to_owned()),
        }),
    );

    assert_eq!(action, Some(ExitAction::Kept));
    assert_eq!(
        status_of(&ws, &id),
        Some(TerminalStatus::Exited {
            code: Some(1),
            signal: Some("Hangup".to_owned())
        })
    );
}

#[test]
fn 終わり方が分からなくても_exited_にする() {
    let mut ws = grid();
    let id = ids(&ws)[0].clone();
    ws.mark_running(&id).unwrap();

    assert_eq!(ws.mark_exited(&id, None), Some(ExitAction::Kept));

    assert_eq!(
        status_of(&ws, &id),
        Some(TerminalStatus::Exited {
            code: None,
            signal: None
        })
    );
}

#[test]
fn 終了の反映は_running_のときだけ効き_2_回目は変化なし() {
    let mut ws = grid();
    let id = ids(&ws)[0].clone();

    assert_eq!(ws.mark_exited(&id, None), None, "起動前に exited になった");
    assert_eq!(status_of(&ws, &id), Some(TerminalStatus::NotStarted));

    ws.mark_running(&id).unwrap();
    assert!(ws.mark_exited(&id, None).is_some());
    assert_eq!(ws.mark_exited(&id, None), None);
    assert_eq!(ws.mark_exited(&missing(), None), None);
}

#[test]
fn 終わったターミナルには接続できない() {
    let mut ws = grid();
    let id = ids(&ws)[0].clone();
    ws.mark_running(&id).unwrap();
    ws.mark_exited(&id, None);

    assert_eq!(
        ws.plan_connect(&id, Path::new(ZSH), None),
        Err(TerminalError::Exited(id))
    );
}

#[test]
fn 入力は_running_のときだけ受け付ける() {
    let mut ws = grid();
    let id = ids(&ws)[0].clone();

    assert_eq!(
        ws.check_input(&id),
        Err(TerminalError::NotStarted(id.clone()))
    );
    ws.mark_failed(&id, "x").unwrap();
    assert_eq!(
        ws.check_input(&id),
        Err(TerminalError::NotStarted(id.clone()))
    );

    ws.mark_running(&id).unwrap();
    assert_eq!(ws.check_input(&id), Ok(()));

    ws.mark_exited(&id, None);
    assert_eq!(ws.check_input(&id), Err(TerminalError::Exited(id)));
}

#[test]
fn 寸法の変更は起動前なら断り_終了後なら何もしない() {
    let mut ws = grid();
    let id = ids(&ws)[0].clone();

    assert_eq!(
        ws.check_resize(&id),
        Err(TerminalError::NotStarted(id.clone()))
    );
    ws.mark_running(&id).unwrap();
    assert_eq!(ws.check_resize(&id), Ok(true));
    ws.mark_exited(&id, None);
    // 終わったあとにウィンドウの大きさを変えても失敗にしない(画面にエラーを出し続けない)。
    assert_eq!(ws.check_resize(&id), Ok(false));
}

#[test]
fn 存在しない_id_はどの操作でも_not_found_になる() {
    let mut ws = workspace();
    let id = missing();
    let not_found = Err(TerminalError::NotFound(id.clone()));

    assert_eq!(ws.plan_connect(&id, Path::new(ZSH), None), not_found);
    assert_eq!(
        ws.mark_running(&id),
        Err(TerminalError::NotFound(id.clone()))
    );
    assert_eq!(
        ws.mark_failed(&id, "x"),
        Err(TerminalError::NotFound(id.clone()))
    );
    assert_eq!(
        ws.check_input(&id),
        Err(TerminalError::NotFound(id.clone()))
    );
    assert_eq!(
        ws.check_resize(&id),
        Err(TerminalError::NotFound(id.clone()))
    );
    assert_eq!(
        ws.close_terminal(&id),
        Err(TerminalError::NotFound(id.clone()))
    );
    assert_eq!(
        ws.activate_terminal(&id),
        Err(TerminalError::NotFound(id.clone()))
    );
    assert_eq!(ws.check_exists(&id), Err(TerminalError::NotFound(id)));
    assert_eq!(ws.check_exists(&only_id(&ws)), Ok(()));
}

#[test]
fn 寸法は行と桁が_1_以上のときだけ受け付ける() {
    assert_eq!(validate_size(24, 80), Ok(PtySize { rows: 24, cols: 80 }));
    assert_eq!(validate_size(1, 1), Ok(PtySize { rows: 1, cols: 1 }));
    for (rows, cols) in [(0, 80), (24, 0), (0, 0)] {
        assert_eq!(
            validate_size(rows, cols),
            Err(TerminalError::InvalidSize { rows, cols })
        );
    }
}

#[test]
fn 失敗の文言に_id_や理由が載る() {
    let id = TerminalId::new("t-9");
    assert!(
        TerminalError::NotFound(id.clone())
            .to_string()
            .contains("t-9")
    );
    assert!(
        TerminalError::NotStarted(id.clone())
            .to_string()
            .contains("t-9")
    );
    assert!(TerminalError::Exited(id).to_string().contains("t-9"));
    assert!(
        TerminalError::Launch(ShellLaunchError::EmptyCwd)
            .to_string()
            .contains("作業フォルダ")
    );
    let project = project_id(7);
    assert!(
        WorkspaceError::RootMissing(project.clone())
            .to_string()
            .contains(project.as_str())
    );
}

#[test]
fn ターミナルの_id_は文字列として_json_になる() {
    let id = TerminalId::new("terminal-1");
    assert_eq!(serde_json::to_value(&id).unwrap(), json!("terminal-1"));
    assert_eq!(id.to_string(), "terminal-1");
}

// ---------------------------------------------------------------------------
// タブの操作(FR-039・FR-044)
// ---------------------------------------------------------------------------

#[test]
fn 新しいタブはプロジェクトの末尾に増えて表示中になり_cwd_はルート() {
    let mut ws = workspace();
    let first = only_id(&ws);

    let second = ws.open_terminal(&project_id(1)).unwrap();

    let snapshot = ws.snapshot();
    assert_eq!(
        snapshot.projects[0].terminal_ids,
        vec![first, second.clone()]
    );
    assert_eq!(
        snapshot.projects[0].active_terminal_id,
        Some(second.clone())
    );
    assert_eq!(snapshot.terminals[1].label, "ターミナル 2");
    assert_eq!(spawn_plan(&mut ws, &second).cwd(), Path::new(ROOT));
}

#[test]
fn 登録の無いプロジェクトには新しいタブを開けない() {
    let mut ws = workspace();

    let err = ws.open_terminal(&project_id(9)).unwrap_err();

    assert!(matches!(err, WorkspaceError::Project(_)), "{err:?}");
    assert_eq!(ws.snapshot().terminals.len(), 1);
}

#[test]
fn 最後の_1_枚を閉じると_0_枚になり_0_枚から開ける() {
    let mut ws = workspace();
    let id = only_id(&ws);

    let closed = ws.close_terminal(&id).unwrap();

    assert_eq!(closed.id, id);
    assert!(ws.snapshot().terminals.is_empty());
    assert_eq!(ws.snapshot().projects[0].active_terminal_id, None);
    assert_eq!(
        ws.check_exists(&id),
        Err(TerminalError::NotFound(id.clone()))
    );

    let reopened = ws.open_terminal(&project_id(1)).unwrap();
    assert_ne!(reopened, id, "ターミナルの ID を使い回した");
    assert_eq!(ws.snapshot().projects[0].active_terminal_id, Some(reopened));
}

#[test]
fn 選択中のプロジェクトの中で前後のタブに切り替わり端で回り込む() {
    let mut ws = workspace();
    let a = only_id(&ws);
    let b = ws.open_terminal(&project_id(1)).unwrap();
    let c = ws.open_terminal(&project_id(1)).unwrap();

    assert_eq!(ws.select_next_terminal(), Some(a.clone()));
    assert_eq!(ws.select_previous_terminal(), Some(c.clone()));
    ws.activate_terminal(&b).unwrap();
    assert_eq!(ws.snapshot().projects[0].active_terminal_id, Some(b));
}

// ---------------------------------------------------------------------------
// 9 枚の検証画面(T22。M4 の T49 で外す)
// ---------------------------------------------------------------------------

#[test]
fn 検証画面は_terminal_1_から順に_9_枚がまだ起動していない状態で載る() {
    let ws = grid();

    let snapshot = ws.snapshot();

    assert_eq!(
        ids(&ws).iter().map(TerminalId::as_str).collect::<Vec<_>>(),
        (1..=9).map(|n| format!("terminal-{n}")).collect::<Vec<_>>()
    );
    assert!(
        snapshot
            .terminals
            .iter()
            .all(|t| t.status == TerminalStatus::NotStarted)
    );
    assert!(snapshot.terminals.iter().all(|t| t.project_id.is_none()));
}

#[test]
fn _9_枚ともホームフォルダで起動を計画し_状態はそれぞれ別に変わる() {
    let mut ws = grid();
    let ids = ids(&ws);

    for id in &ids {
        let spec = spawn_plan(&mut ws, id);
        assert_eq!(spec.cwd(), Path::new(HOME));
        ws.mark_running(id).unwrap();
    }
    // 1 枚が終わっても、ほかの 8 枚は動いたまま入力を受け付ける(NFR-006)。
    assert!(
        ws.mark_exited(
            &ids[4],
            Some(ChildExit {
                exit_code: 0,
                signal: None,
            })
        )
        .is_some()
    );
    ws.mark_failed(&ids[8], "偽の失敗").unwrap();

    assert_eq!(
        ws.check_input(&ids[4]),
        Err(TerminalError::Exited(ids[4].clone()))
    );
    for (i, id) in ids.iter().enumerate() {
        if i == 4 || i == 8 {
            continue;
        }
        assert_eq!(ws.check_input(id), Ok(()), "{id} が入力を受け付けない");
    }
    let running = ws
        .snapshot()
        .terminals
        .iter()
        .filter(|t| t.status == TerminalStatus::Running)
        .count();
    assert_eq!(running, 7);
    // 起動に失敗した 1 枚は、次の接続で起動し直せる。
    assert_eq!(spawn_plan(&mut ws, &ids[8]).cwd(), Path::new(HOME));
}
