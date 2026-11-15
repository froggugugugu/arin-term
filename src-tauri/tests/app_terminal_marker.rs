//! スモークの印を有効にして組み立てたとき、最初の処理済みの通知(0 より大きい)でターミナルの印が書かれることを確かめる。
//!
//! PTY は偽物(`builder_with_pty`)。印が無効な場合は `launch.rs` の単体テストで確かめる。
//! 模擬のアプリを 1 プロセスで何度も作らないため、設定ごとにテストのバイナリを分けている。
//!
//! macOS ではメニューの部品をメインスレッドでしか作れないため、標準のテストハーネスを使わず
//! (Cargo.toml で `harness = false`)、この `main` をメインスレッドで実行する。

mod support;

use std::cell::Cell;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use arin_core::paths::{SMOKE_READY_MARKER_FILE_NAME, SMOKE_TERMINAL_MARKER_FILE_NAME};
use arin_core::ports::{ChildExit, PtyError, PtyPort, PtySession, PtySize, SpawnedPty};
use arin_core::terminal::ShellLaunchSpec;
use arin_lib::sessions::ShellDefaults;
use serde_json::json;
use support::{request, request_with};
use tauri::ipc::{InvokeBody, InvokeResponseBody};
use tauri::test::{MockRuntime, get_ipc_response};
use tauri::{AppHandle, Manager, RunEvent, WebviewWindow};

const WAIT: Duration = Duration::from_secs(10);

/// 起動すると "% " を 1 回出し、あとは出力しない偽の PTY。出力の送り口を持ち続けて読み取りを待たせる。
struct FakePty(Mutex<Vec<Sender<Vec<u8>>>>);

impl PtyPort for FakePty {
    fn spawn(&self, _spec: &ShellLaunchSpec, _size: PtySize) -> Result<SpawnedPty, PtyError> {
        let (tx, rx) = mpsc::channel();
        tx.send(b"% ".to_vec()).expect("送れなかった");
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(tx);
        Ok(SpawnedPty {
            session: Box::new(FakeSession),
            output: Box::new(FakeOutput(rx)),
        })
    }
}

struct FakeOutput(Receiver<Vec<u8>>);

impl Read for FakeOutput {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.0.recv() {
            Ok(chunk) => {
                buf[..chunk.len()].copy_from_slice(&chunk);
                Ok(chunk.len())
            }
            Err(_) => Ok(0),
        }
    }
}

struct FakeSession;

impl PtySession for FakeSession {
    fn write(&mut self, _data: &[u8]) -> Result<(), PtyError> {
        Ok(())
    }

    fn resize(&mut self, _size: PtySize) -> Result<(), PtyError> {
        Ok(())
    }

    fn kill(&mut self) -> Result<(), PtyError> {
        Ok(())
    }

    fn try_wait(&mut self) -> Result<Option<ChildExit>, PtyError> {
        Ok(None)
    }
}

fn call(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    cmd: &str,
    args: serde_json::Value,
) {
    get_ipc_response(window, request_with(handle, cmd, InvokeBody::Json(args)))
        .unwrap_or_else(|e| panic!("{cmd} が失敗した: {e}"));
}

fn main() {
    let state_dir = support::TempDir::new("terminal-marker");
    let received: Arc<Mutex<Vec<u8>>> = Arc::default();
    let sink = Arc::clone(&received);
    let app = arin_lib::builder_with_pty::<MockRuntime>(
        support::launch_verify_grid_in(&state_dir, true).with_terminal_env(
            arin_lib::launch::TerminalEnv {
                home: Some(PathBuf::from("/Users/someone")),
                user: None,
                vars: vec![("HOME".into(), "/Users/someone".into())],
            },
        ),
        Arc::new(FakePty(Mutex::default())),
        Arc::new(|| ShellDefaults {
            shell: PathBuf::from("/bin/zsh"),
            locale: None,
        }),
    )
    .channel_interceptor(move |_webview, _callback, _index, body| {
        if let InvokeResponseBody::Raw(bytes) = body {
            sink.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .extend_from_slice(bytes);
        }
        true
    })
    .build(tauri::generate_context!())
    .unwrap_or_else(|e| panic!("組み立てに失敗した: {e}"));

    let done = Rc::new(Cell::new(false));
    let seen = Rc::clone(&done);

    app.run_return(move |handle, event| {
        if let RunEvent::Ready = event {
            let window = handle
                .get_webview_window("main")
                .expect("設定にあるウィンドウ main が作られなかった");
            let snapshot: arin_core::workspace::AppSnapshot =
                get_ipc_response(&window, request(handle, "get_app_snapshot"))
                    .unwrap_or_else(|e| panic!("get_app_snapshot が失敗した: {e}"))
                    .deserialize()
                    .expect("写しとして読めなかった");
            let id = snapshot.terminals[0].id.as_str().to_owned();

            call(
                handle,
                &window,
                "connect_terminal",
                json!({ "terminalId": id, "rows": 24, "cols": 80, "onOutput": "__CHANNEL__:9" }),
            );
            let deadline = Instant::now() + WAIT;
            while received
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .is_empty()
            {
                assert!(Instant::now() < deadline, "出力がチャネルに届かなかった");
                thread::sleep(Duration::from_millis(1));
            }
            assert_eq!(
                state_dir.entries(),
                Vec::<String>::new(),
                "処理済みの通知より前にターミナルの印を書いた"
            );

            call(
                handle,
                &window,
                "ack_terminal_output",
                json!({ "terminalId": id, "bytes": 0 }),
            );
            assert_eq!(
                state_dir.entries(),
                Vec::<String>::new(),
                "0 バイトの通知でターミナルの印を書いた"
            );
            println!("test 0 バイトの処理済みの通知ではターミナルの印を書かない ... ok");

            call(
                handle,
                &window,
                "ack_terminal_output",
                json!({ "terminalId": id, "bytes": 2 }),
            );
            assert_eq!(
                state_dir.entries(),
                vec![SMOKE_TERMINAL_MARKER_FILE_NAME.to_string()],
                "最初の処理済みの通知でターミナルの印だけが書かれるはず"
            );
            let marker = state_dir.path().join(SMOKE_TERMINAL_MARKER_FILE_NAME);
            let text = fs::read_to_string(&marker).expect("印のファイルを読めなかった");
            assert_eq!(text.trim(), std::process::id().to_string());
            println!("test 最初の処理済みの通知でターミナルの印にプロセス_id_を書く ... ok");

            // 2 回目は書き直さない(消しておいても作り直さない)。
            fs::remove_file(&marker).expect("印を消せなかった");
            call(
                handle,
                &window,
                "ack_terminal_output",
                json!({ "terminalId": id, "bytes": 2 }),
            );
            assert_eq!(state_dir.entries(), Vec::<String>::new());
            println!("test 2_回目以降の処理済みの通知では印を書かない ... ok");

            // 準備完了の印とは別のファイル。
            get_ipc_response(&window, request(handle, "notify_app_ready"))
                .unwrap_or_else(|e| panic!("notify_app_ready が失敗した: {e}"));
            assert_eq!(
                state_dir.entries(),
                vec![SMOKE_READY_MARKER_FILE_NAME.to_string()]
            );
            seen.set(true);

            if let Some(sessions) = handle.try_state::<arin_lib::sessions::SessionRegistry>() {
                sessions.shutdown();
            }
            for window in handle.webview_windows().into_values() {
                window.destroy().expect("ウィンドウを閉じられなかった");
            }
        }
    });

    assert!(done.get(), "起動の完了まで進まなかった");
}
