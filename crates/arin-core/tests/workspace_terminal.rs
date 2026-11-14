//! `Workspace` が持つターミナルの状態と規則(M1: 単一プロジェクト・単一タブ)。
//! 設計書 §1.3 #2、§5.1、§6.1、§7.1「ターミナルの起動」、FR-039・FR-040・FR-043。

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use arin_core::ports::{ChildExit, PtySize};
use arin_core::terminal::{
    ShellLaunchError, TerminalError, TerminalId, TerminalStatus, validate_size,
};
use arin_core::workspace::{ConnectPlan, SNAPSHOT_SCHEMA_VERSION, Workspace};
use serde_json::json;

const HOME: &str = "/Users/someone";
const ZSH: &str = "/bin/zsh";

fn workspace() -> Workspace {
    Workspace::new(
        "1.2.3",
        PathBuf::from(HOME),
        vec![("HOME", HOME), ("USER", "someone"), ("SECRET_TOKEN", "x")],
    )
}

fn only_id(ws: &Workspace) -> TerminalId {
    let snapshot = ws.snapshot();
    assert_eq!(snapshot.terminals.len(), 1, "ターミナルが 1 枚ではない");
    snapshot.terminals[0].id.clone()
}

fn status(ws: &Workspace) -> TerminalStatus {
    ws.snapshot().terminals[0].status.clone()
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

#[test]
fn 作った直後の写しにはまだ起動していないターミナルが_1_枚載る() {
    let ws = workspace();

    let snapshot = ws.snapshot();

    assert_eq!(snapshot.schema_version, SNAPSHOT_SCHEMA_VERSION);
    assert_eq!(snapshot.app_version, "1.2.3");
    assert_eq!(snapshot.terminals.len(), 1);
    assert_eq!(snapshot.terminals[0].status, TerminalStatus::NotStarted);
    assert!(!snapshot.terminals[0].id.as_str().is_empty());
}

#[test]
fn 写しの_json_はターミナルの_id_と状態を_camel_case_で持つ() {
    let ws = workspace();
    let id = only_id(&ws);

    let value = serde_json::to_value(ws.snapshot()).expect("JSON にできなかった");

    assert_eq!(
        value,
        json!({
            "schemaVersion": SNAPSHOT_SCHEMA_VERSION,
            "appVersion": "1.2.3",
            "terminals": [{ "id": id.as_str(), "status": { "kind": "notStarted" } }],
        })
    );
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
fn 最初の接続はホームフォルダでログインシェルを起動する計画になる() {
    let mut ws = workspace();
    let id = only_id(&ws);

    let spec = spawn_plan(&mut ws, &id);

    assert_eq!(spec.program(), Path::new(ZSH));
    assert_eq!(spec.argv0(), OsStr::new("-zsh"));
    assert_eq!(spec.cwd(), Path::new(HOME));
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
    assert_eq!(status(&ws), TerminalStatus::NotStarted);
}

#[test]
fn 起動できたら_running_になり_2_回目の接続は付け替えになる() {
    let mut ws = workspace();
    let id = only_id(&ws);
    let _ = spawn_plan(&mut ws, &id);

    ws.mark_running(&id).unwrap();

    assert_eq!(status(&ws), TerminalStatus::Running);
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
        status(&ws),
        TerminalStatus::Failed {
            message: "シェルが無い".to_owned()
        }
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
    assert!(matches!(status(&ws), TerminalStatus::Failed { message } if message.contains("zsh")));
}

#[test]
fn ホームフォルダが分からなければ起動の計画は失敗する() {
    let mut ws = Workspace::new("1.2.3", PathBuf::new(), Vec::<(String, String)>::new());
    let id = only_id(&ws);

    let err = ws.plan_connect(&id, Path::new(ZSH), None).unwrap_err();

    assert_eq!(err, TerminalError::Launch(ShellLaunchError::EmptyCwd));
}

#[test]
fn シェルが終わったら_exited_になり終わり方を持つ() {
    let mut ws = workspace();
    let id = only_id(&ws);
    ws.mark_running(&id).unwrap();

    let changed = ws.mark_exited(
        &id,
        Some(ChildExit {
            exit_code: 1,
            signal: Some("Hangup".to_owned()),
        }),
    );

    assert!(changed);
    assert_eq!(
        status(&ws),
        TerminalStatus::Exited {
            code: Some(1),
            signal: Some("Hangup".to_owned())
        }
    );
}

#[test]
fn 終わり方が分からなくても_exited_にする() {
    let mut ws = workspace();
    let id = only_id(&ws);
    ws.mark_running(&id).unwrap();

    assert!(ws.mark_exited(&id, None));

    assert_eq!(
        status(&ws),
        TerminalStatus::Exited {
            code: None,
            signal: None
        }
    );
}

#[test]
fn 終了の反映は_running_のときだけ効き_2_回目は変化なし() {
    let mut ws = workspace();
    let id = only_id(&ws);

    assert!(!ws.mark_exited(&id, None), "起動前に exited になった");
    assert_eq!(status(&ws), TerminalStatus::NotStarted);

    ws.mark_running(&id).unwrap();
    assert!(ws.mark_exited(&id, None));
    assert!(!ws.mark_exited(&id, None));
    assert!(!ws.mark_exited(&missing(), None));
}

#[test]
fn 終わったターミナルには接続できない() {
    let mut ws = workspace();
    let id = only_id(&ws);
    ws.mark_running(&id).unwrap();
    ws.mark_exited(&id, None);

    assert_eq!(
        ws.plan_connect(&id, Path::new(ZSH), None),
        Err(TerminalError::Exited(id))
    );
}

#[test]
fn 入力は_running_のときだけ受け付ける() {
    let mut ws = workspace();
    let id = only_id(&ws);

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
    let mut ws = workspace();
    let id = only_id(&ws);

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
        TerminalError::InvalidSize { rows: 0, cols: 80 }
            .to_string()
            .contains("0")
    );
    assert!(
        TerminalError::Launch(ShellLaunchError::EmptyCwd)
            .to_string()
            .contains("作業フォルダ")
    );
}

#[test]
fn ターミナルの_id_は文字列として_json_になる() {
    let id = TerminalId::new("terminal-1");
    assert_eq!(serde_json::to_value(&id).unwrap(), json!("terminal-1"));
    assert_eq!(id.to_string(), "terminal-1");
}
