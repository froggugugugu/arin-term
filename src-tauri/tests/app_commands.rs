//! アプリのコマンド(`src-tauri/src/commands/app.rs`)を、Tauri の模擬ランタイムで画面から呼ぶ形で確かめる。
//!
//! 本物の `tauri.conf.json` と capabilities から作った設定で組み立てるので、
//! 画面に許す操作(ACL)の検査も本物と同じものを通る。
//!
//! macOS ではメニューの部品をメインスレッドでしか作れないため、標準のテストハーネスを使わず
//! (Cargo.toml で `harness = false`)、この `main` をメインスレッドで実行する。

use std::cell::Cell;
use std::rc::Rc;

use arin_core::workspace::{AppSnapshot, SNAPSHOT_SCHEMA_VERSION};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{MockRuntime, get_ipc_response};
use tauri::webview::InvokeRequest;
use tauri::{AppHandle, Manager, RunEvent, WebviewWindow};

/// 画面(自アプリの画面)からコマンドを呼んだときの要求を作る。
fn request(handle: &AppHandle<MockRuntime>, cmd: &str) -> InvokeRequest {
    InvokeRequest {
        cmd: cmd.into(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        url: "tauri://localhost".parse().expect("URL が不正"),
        body: InvokeBody::default(),
        headers: Default::default(),
        invoke_key: handle.invoke_key().to_string(),
    }
}

type Check = fn(&AppHandle<MockRuntime>, &WebviewWindow<MockRuntime>);

fn 写しの取得コマンドが_app_snapshot_を返す(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
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
) {
    let body = get_ipc_response(window, request(handle, "notify_app_ready"))
        .unwrap_or_else(|e| panic!("notify_app_ready が失敗した: {e}"));
    // 戻り値は無い(JSON の null)。null 以外が返ると、ここで読めずに失敗する。
    body.deserialize::<()>()
        .unwrap_or_else(|e| panic!("戻り値が null ではなかった: {e}"));
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
];

/// 1 回だけ起動し、メインのウィンドウができたところで全部の確認を順に行ってから閉じる。
/// (1 つのプロセスで模擬のアプリを何度も作らないため)
fn main() {
    let app = arin_lib::builder::<MockRuntime>()
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
                check(handle, &window);
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
