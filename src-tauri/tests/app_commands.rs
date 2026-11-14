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
use support::request;
use tauri::test::{MockRuntime, get_ipc_response};
use tauri::{AppHandle, Manager, RunEvent, WebviewWindow};

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
        state_dir.entries(),
        Vec::<String>::new(),
        "スモークの指定が無いのに状態フォルダに何か書いた"
    );
}

const CHECKS: &[(&str, Check)] = &[
    (
        "写しの取得コマンドが_app_snapshot_を返す",
        写しの取得コマンドが_app_snapshot_を返す,
    ),
    (
        "準備完了の通知コマンドを画面から呼べる",
        準備完了の通知コマンドを画面から呼べる,
    ),
    (
        "スモークの指定が無ければ準備完了の通知で印のファイルを書かない",
        スモークの指定が無ければ準備完了の通知で印のファイルを書かない,
    ),
];

/// 1 回だけ起動し、メインのウィンドウができたところで全部の確認を順に行ってから閉じる。
/// (1 つのプロセスで模擬のアプリを何度も作らないため)
fn main() {
    // 状態フォルダは一時フォルダに差し替え、スモークの印は有効にしない(通常の起動と同じ)。
    let state_dir = support::TempDir::new("commands");
    let app = arin_lib::builder::<MockRuntime>(support::launch_in(&state_dir, false))
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
