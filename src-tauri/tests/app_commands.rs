//! アプリのコマンド(`src-tauri/src/commands/app.rs`)を、Tauri の模擬ランタイムで画面から呼ぶ形で確かめる。
//!
//! 本物の `tauri.conf.json` と capabilities から作った設定で組み立てるので、
//! 画面に許す操作(ACL)の検査も本物と同じものを通る。
//!
//! macOS ではメニューの部品をメインスレッドでしか作れないため、標準のテストハーネスを使わず
//! (Cargo.toml で `harness = false`)、この `main` をメインスレッドで実行する。

mod support;

use std::cell::Cell;
use std::rc::Rc;

use arin_core::workspace::{AppSnapshot, SNAPSHOT_SCHEMA_VERSION};
use serde_json::json;
use support::{request, request_with};
use tauri::ipc::InvokeBody;
use tauri::test::{MockRuntime, get_ipc_response};
use tauri::{AppHandle, Manager, RunEvent, WebviewWindow};

/// 状態ファイルに置くプロジェクトの数(各 1 タブで 9 枚。既定の起動=プロジェクト 0 件・ターミナル 0 枚は
/// `app_builder.rs` が確かめる。M1 の 9 枚の検証画面の指定の代わり。T49d)。
const PROJECTS: u8 = 9;

type Check = fn(&AppHandle<MockRuntime>, &WebviewWindow<MockRuntime>, &support::TempDir);

fn 写しの取得コマンドが_app_snapshot_を返す(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    _state_dir: &support::TempDir,
) {
    let body = get_ipc_response(window, request(handle, "get_app_snapshot"))
        .unwrap_or_else(|e| panic!("get_app_snapshot が失敗した: {e}"));
    let snapshot: AppSnapshot = body
        .deserialize()
        .unwrap_or_else(|e| panic!("AppSnapshot として読めなかった: {e}"));

    assert_eq!(snapshot.schema_version, SNAPSHOT_SCHEMA_VERSION);
    assert_eq!(
        snapshot.app_version,
        handle.package_info().version.to_string()
    );
    // 9 プロジェクトの状態ファイルを置いて組み立てたので、各プロジェクトに 1 枚ずつ、ターミナルが 9 枚載る。
    let ids: Vec<&str> = snapshot.terminals.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(
        ids,
        (1..=9).map(|n| format!("terminal-{n}")).collect::<Vec<_>>()
    );
    assert_eq!(snapshot.projects.len(), usize::from(PROJECTS));
    for (n, terminal) in (1..=PROJECTS).zip(&snapshot.terminals) {
        assert_eq!(terminal.project_id, Some(support::project_id(n)));
    }
}

fn 外した起動時の指定の取得コマンドは呼べない(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    _state_dir: &support::TempDir,
) {
    // 描画方式と全角幅の方式の起動時の指定は T50b で外した(DOM 描画と `unicode11` に固定。ADR-001)。
    let result = get_ipc_response(window, request(handle, "get_launch_options"));
    assert!(
        result.is_err(),
        "外したはずの get_launch_options が呼べた: {result:?}"
    );
}

fn 準備完了の通知コマンドを画面から呼べる(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    _state_dir: &support::TempDir,
) {
    let body = get_ipc_response(window, request(handle, "notify_app_ready"))
        .unwrap_or_else(|e| panic!("notify_app_ready が失敗した: {e}"));
    // 戻り値は無い(JSON の null)。null 以外が返ると、ここで読めずに失敗する。
    body.deserialize::<()>()
        .unwrap_or_else(|e| panic!("戻り値が null ではなかった: {e}"));
}

fn スモークの指定が無ければ準備完了の通知で印のファイルを書かない(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    state_dir: &support::TempDir,
) {
    get_ipc_response(window, request(handle, "notify_app_ready"))
        .unwrap_or_else(|e| panic!("notify_app_ready が失敗した: {e}"));

    assert_eq!(
        state_dir.entries_except_state(),
        Vec::<String>::new(),
        "スモークの指定が無いのに状態フォルダに何か書いた"
    );
}

fn 画面の失敗の報告コマンドを画面から呼べて_null_を返し_印を書かない(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    state_dir: &support::TempDir,
) {
    let body = get_ipc_response(
        window,
        request_with(
            handle,
            "report_frontend_error",
            InvokeBody::Json(json!({ "message": "テストからの報告(改行\nを含む)" })),
        ),
    )
    .unwrap_or_else(|e| panic!("report_frontend_error が失敗した: {e}"));
    body.deserialize::<()>()
        .unwrap_or_else(|e| panic!("戻り値が null ではなかった: {e}"));
    assert_eq!(state_dir.entries_except_state(), Vec::<String>::new());
}

fn 画面は状態の写しのイベントを購読と解除できるが_送信はできない(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    _state_dir: &support::TempDir,
) {
    // @tauri-apps/api の listen() と、その解除の関数が送るものと同じ形。
    let listen = get_ipc_response(
        window,
        request_with(
            handle,
            "plugin:event|listen",
            InvokeBody::Json(json!({
                "event": arin_lib::SNAPSHOT_EVENT,
                "target": { "kind": "Any" },
                "handler": 7,
            })),
        ),
    )
    .unwrap_or_else(|e| panic!("イベントの購読が許されていない: {e}"));
    let event_id: u32 = listen
        .deserialize()
        .unwrap_or_else(|e| panic!("購読の ID として読めなかった: {e}"));
    get_ipc_response(
        window,
        request_with(
            handle,
            "plugin:event|unlisten",
            InvokeBody::Json(json!({ "event": arin_lib::SNAPSHOT_EVENT, "eventId": event_id })),
        ),
    )
    .unwrap_or_else(|e| panic!("イベントの解除が許されていない: {e}"));

    let emit = get_ipc_response(
        window,
        request_with(
            handle,
            "plugin:event|emit",
            InvokeBody::Json(json!({ "event": arin_lib::SNAPSHOT_EVENT, "payload": null })),
        ),
    );
    assert!(emit.is_err(), "画面からのイベントの送信が許されている");
}

const CHECKS: &[(&str, Check)] = &[
    (
        "写しの取得コマンドが_app_snapshot_を返す",
        写しの取得コマンドが_app_snapshot_を返す,
    ),
    (
        "外した起動時の指定の取得コマンドは呼べない",
        外した起動時の指定の取得コマンドは呼べない,
    ),
    (
        "準備完了の通知コマンドを画面から呼べる",
        準備完了の通知コマンドを画面から呼べる,
    ),
    (
        "スモークの指定が無ければ準備完了の通知で印のファイルを書かない",
        スモークの指定が無ければ準備完了の通知で印のファイルを書かない,
    ),
    (
        "画面の失敗の報告コマンドを画面から呼べて_null_を返し_印を書かない",
        画面の失敗の報告コマンドを画面から呼べて_null_を返し_印を書かない,
    ),
    (
        "画面は状態の写しのイベントを購読と解除できるが_送信はできない",
        画面は状態の写しのイベントを購読と解除できるが_送信はできない,
    ),
];

/// 1 回だけ起動し、メインのウィンドウができたところで全部の確認を順に行ってから閉じる。
/// (1 つのプロセスで模擬のアプリを何度も作らないため)
fn main() {
    // 状態フォルダは一時フォルダに差し替え、スモークの印は有効にしない(通常の起動と同じ)。
    let state_dir = support::TempDir::new("commands");
    let work = support::TempDir::new("commands-work");
    let roots = support::make_project_roots(&work, PROJECTS);
    let launch = support::launch_with_projects_in(&state_dir, &roots, false);
    let app = arin_lib::builder::<MockRuntime>(launch)
        .build(tauri::generate_context!())
        .unwrap_or_else(|e| panic!("組み立てに失敗した: {e}"));

    let passed = Rc::new(Cell::new(0_usize));
    let count = Rc::clone(&passed);

    app.run_return(move |handle, event| {
        if let RunEvent::Ready = event {
            let window = handle
                .get_webview_window("main")
                .expect("設定にあるウィンドウ main が作られなかった");
            for (name, check) in CHECKS {
                check(handle, &window, &state_dir);
                println!("test {name} ... ok");
                count.set(count.get() + 1);
            }
            // 模擬ランタイムのループは、ウィンドウが全部無くなると終わる。
            for window in handle.webview_windows().into_values() {
                window.destroy().expect("ウィンドウを閉じられなかった");
            }
        }
    });

    assert_eq!(passed.get(), CHECKS.len(), "起動の完了まで進まなかった");
}
