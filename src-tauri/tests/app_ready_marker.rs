//! スモークの印を有効にして組み立てたとき、画面からの準備完了の通知で印のファイルが書かれることを確かめる。
//!
//! 印の無い場合(通常の起動)は `app_commands.rs` で確かめる。模擬のアプリを 1 プロセスで何度も作らないため、
//! 設定ごとにテストのバイナリを分けている。
//!
//! macOS ではメニューの部品をメインスレッドでしか作れないため、標準のテストハーネスを使わず
//! (Cargo.toml で `harness = false`)、この `main` をメインスレッドで実行する。

mod support;

use std::cell::Cell;
use std::fs;
use std::rc::Rc;

use arin_core::paths::SMOKE_READY_MARKER_FILE_NAME;
use support::request;
use tauri::test::{MockRuntime, get_ipc_response};
use tauri::{Manager, RunEvent};

fn main() {
    let state_dir = support::TempDir::new("ready-marker");
    let app = arin_lib::builder::<MockRuntime>(support::launch_in(&state_dir, true))
        .build(tauri::generate_context!())
        .unwrap_or_else(|e| panic!("組み立てに失敗した: {e}"));

    let done = Rc::new(Cell::new(false));
    let seen = Rc::clone(&done);

    app.run_return(move |handle, event| {
        if let RunEvent::Ready = event {
            let window = handle
                .get_webview_window("main")
                .expect("設定にあるウィンドウ main が作られなかった");

            assert_eq!(
                state_dir.entries(),
                Vec::<String>::new(),
                "準備完了の通知より前に印を書いた"
            );

            get_ipc_response(&window, request(handle, "notify_app_ready"))
                .unwrap_or_else(|e| panic!("notify_app_ready が失敗した: {e}"));

            assert_eq!(
                state_dir.entries(),
                vec![SMOKE_READY_MARKER_FILE_NAME.to_string()],
                "準備完了の通知で印のファイルだけが書かれるはず"
            );
            // 中身はアプリのプロセス ID。スモークは自分が起こしたプロセスの印かどうかをこれで確かめる。
            let text = fs::read_to_string(state_dir.path().join(SMOKE_READY_MARKER_FILE_NAME))
                .expect("印のファイルを読めなかった");
            assert_eq!(text.trim(), std::process::id().to_string());
            println!("test スモークの指定があれば準備完了の通知で印のファイルが書かれる ... ok");
            seen.set(true);

            for window in handle.webview_windows().into_values() {
                window.destroy().expect("ウィンドウを閉じられなかった");
            }
        }
    });

    assert!(done.get(), "起動の完了まで進まなかった");
}
