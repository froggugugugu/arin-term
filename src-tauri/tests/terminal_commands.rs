//! ターミナルのコマンド(`src-tauri/src/commands/terminal.rs`)を、Tauri の模擬ランタイムで画面から呼ぶ形で確かめる。
//!
//! - PTY は偽物(`builder_with_pty`)。本物の `tauri.conf.json` と capabilities で組み立てる
//! - 出力のチャネルは、画面が渡すのと同じ `__CHANNEL__:<番号>` の文字列で渡し、Tauri の
//!   `channel_interceptor` で送られたバイト列を受け取る
//! - どの待ちにも上限時間を付ける
//! - 模擬のアプリを 2 回、順に組み立てる。1 回目は 9 枚の検証画面(起動直後からターミナルがある)、2 回目は
//!   一時フォルダのプロジェクトを 1 件持つ状態ファイルを置いた通常の起動(タブのコマンド。T33b)
//!
//! macOS ではメニューの部品をメインスレッドでしか作れないため、標準のテストハーネスを使わず
//! (Cargo.toml で `harness = false`)、この `main` をメインスレッドで実行する。

mod support;

use std::cell::Cell;
use std::collections::HashMap;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use arin_core::ports::{ChildExit, PtyError, PtyPort, PtySession, PtySize, SpawnedPty};
use arin_core::terminal::{ShellLaunchSpec, TerminalStatus};
use arin_core::workspace::{AppSnapshot, ConnectOutcome};
use arin_lib::sessions::ShellDefaults;
use serde_json::json;
use support::{request, request_with};
use tauri::ipc::{InvokeBody, InvokeResponseBody};
use tauri::test::{MockRuntime, get_ipc_response};
use tauri::{AppHandle, Listener, Manager, RunEvent, WebviewWindow};

const WAIT: Duration = Duration::from_secs(10);
const CHANNEL: u32 = 4242;

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn wait_until(mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        thread::sleep(Duration::from_millis(1));
    }
    condition()
}

/// 偽の PTY の記録と操作。
#[derive(Default)]
struct Control {
    spawned: Mutex<Vec<(ShellLaunchSpec, PtySize)>>,
    feed: Mutex<Option<Sender<Vec<u8>>>>,
    written: Mutex<Vec<u8>>,
    sizes: Mutex<Vec<PtySize>>,
    killed: AtomicBool,
    exit: Mutex<Option<ChildExit>>,
}

impl Control {
    fn feed(&self, data: &[u8]) {
        lock(&self.feed)
            .as_ref()
            .expect("まだ起動していない")
            .send(data.to_vec())
            .expect("読み取り側がいない");
    }
}

struct FakePty(Arc<Control>);

impl PtyPort for FakePty {
    fn spawn(&self, spec: &ShellLaunchSpec, size: PtySize) -> Result<SpawnedPty, PtyError> {
        lock(&self.0.spawned).push((spec.clone(), size));
        let (tx, rx) = mpsc::channel();
        // シェルが起動直後に出す出力(接続の前の出力)。
        tx.send(b"% ".to_vec()).unwrap();
        *lock(&self.0.feed) = Some(tx);
        Ok(SpawnedPty {
            session: Box::new(FakeSession(Arc::clone(&self.0))),
            output: Box::new(FakeOutput(rx)),
        })
    }
}

struct FakeOutput(Receiver<Vec<u8>>);

impl Read for FakeOutput {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.0.recv() {
            Ok(chunk) => {
                // テストで送るのは小さい塊だけ。
                buf[..chunk.len()].copy_from_slice(&chunk);
                Ok(chunk.len())
            }
            Err(_) => Ok(0),
        }
    }
}

struct FakeSession(Arc<Control>);

impl PtySession for FakeSession {
    fn write(&mut self, data: &[u8]) -> Result<(), PtyError> {
        lock(&self.0.written).extend_from_slice(data);
        Ok(())
    }

    fn resize(&mut self, size: PtySize) -> Result<(), PtyError> {
        lock(&self.0.sizes).push(size);
        Ok(())
    }

    fn kill(&mut self) -> Result<(), PtyError> {
        self.0.killed.store(true, Ordering::SeqCst);
        lock(&self.0.feed).take();
        Ok(())
    }

    fn try_wait(&mut self) -> Result<Option<ChildExit>, PtyError> {
        Ok(lock(&self.0.exit).clone())
    }
}

/// テストで共有するもの。
struct Ctx {
    control: Arc<Control>,
    /// チャネルの番号ごとの、届いたバイト列。
    received: Arc<Mutex<HashMap<u32, Vec<u8>>>>,
    /// 状態の写しのイベントで届いた写し。
    events: Arc<Mutex<Vec<AppSnapshot>>>,
    /// プロジェクトのルート(通常の起動のとき)。
    root: Option<PathBuf>,
}

impl Ctx {
    fn received(&self, channel: u32) -> Vec<u8> {
        lock(&self.received)
            .get(&channel)
            .cloned()
            .unwrap_or_default()
    }
}

type Check = fn(&AppHandle<MockRuntime>, &WebviewWindow<MockRuntime>, &Ctx);

fn call(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    cmd: &str,
    args: serde_json::Value,
) -> Result<InvokeResponseBody, serde_json::Value> {
    get_ipc_response(window, request_with(handle, cmd, InvokeBody::Json(args)))
}

fn snapshot(handle: &AppHandle<MockRuntime>, window: &WebviewWindow<MockRuntime>) -> AppSnapshot {
    get_ipc_response(window, request(handle, "get_app_snapshot"))
        .unwrap_or_else(|e| panic!("get_app_snapshot が失敗した: {e}"))
        .deserialize()
        .unwrap_or_else(|e| panic!("AppSnapshot として読めなかった: {e}"))
}

fn terminal_id(handle: &AppHandle<MockRuntime>, window: &WebviewWindow<MockRuntime>) -> String {
    snapshot(handle, window).terminals[0].id.as_str().to_owned()
}

fn error_text(result: Result<InvokeResponseBody, serde_json::Value>) -> String {
    match result {
        Ok(_) => panic!("失敗するはずが成功した"),
        Err(e) => e
            .as_str()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| e.to_string()),
    }
}

fn 写しにまだ起動していないターミナルが載る(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let snapshot = snapshot(handle, window);

    // 9 枚の検証画面の指定で組み立てた(support::launch_verify_grid_in)。
    assert_eq!(snapshot.terminals.len(), 9);
    assert_eq!(snapshot.terminals[0].status, TerminalStatus::NotStarted);
    assert!(lock(&ctx.control.spawned).is_empty(), "接続の前に起動した");
}

fn 存在しない_id_や寸法が_0_の接続は失敗して起動しない(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let id = terminal_id(handle, window);
    let channel = format!("__CHANNEL__:{CHANNEL}");

    let missing = error_text(call(
        handle,
        window,
        "connect_terminal",
        json!({ "terminalId": "no-such", "rows": 24, "cols": 80, "onOutput": channel }),
    ));
    assert!(missing.contains("no-such"), "{missing}");

    let zero = error_text(call(
        handle,
        window,
        "connect_terminal",
        json!({ "terminalId": id, "rows": 0, "cols": 80, "onOutput": channel }),
    ));
    assert!(zero.contains("寸法"), "{zero}");

    let not_started = error_text(call(
        handle,
        window,
        "write_terminal",
        json!({ "terminalId": id, "data": "ls\r" }),
    ));
    assert!(not_started.contains("起動していない"), "{not_started}");

    assert!(lock(&ctx.control.spawned).is_empty());
}

fn 接続でシェルを起動し_接続の前の出力からチャネルに届く(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let id = terminal_id(handle, window);

    call(
        handle,
        window,
        "connect_terminal",
        json!({ "terminalId": id, "rows": 24, "cols": 80, "onOutput": format!("__CHANNEL__:{CHANNEL}") }),
    )
    .unwrap_or_else(|e| panic!("connect_terminal が失敗した: {e}"))
    .deserialize::<ConnectOutcome>()
    .map(|outcome| {
        assert_eq!(outcome, ConnectOutcome { size_applied: true }, "起動では寸法を反映する");
    })
    .unwrap_or_else(|e| panic!("応答が ConnectOutcome ではなかった: {e}"));
    ctx.control.feed(b"hello\r\n");

    assert!(
        wait_until(|| ctx.received(CHANNEL) == b"% hello\r\n"),
        "チャネルに届いた出力が違う: {:?}",
        String::from_utf8_lossy(&ctx.received(CHANNEL))
    );
    let spawned = lock(&ctx.control.spawned);
    assert_eq!(spawned.len(), 1);
    assert_eq!(spawned[0].1, PtySize { rows: 24, cols: 80 });
    drop(spawned);

    // 状態の写しは running になり、変化はイベントでも届く。
    assert_eq!(
        snapshot(handle, window).terminals[0].status,
        TerminalStatus::Running
    );
    assert!(
        wait_until(|| lock(&ctx.events)
            .iter()
            .any(|s| s.terminals[0].status == TerminalStatus::Running)),
        "状態の写しのイベントが届かなかった"
    );
}

fn 入力と寸法の変更と処理済みの通知が届く(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let id = terminal_id(handle, window);

    call(
        handle,
        window,
        "write_terminal",
        json!({ "terminalId": id, "data": "echo あ\r" }),
    )
    .unwrap_or_else(|e| panic!("write_terminal が失敗した: {e}"));
    call(
        handle,
        window,
        "resize_terminal",
        json!({ "terminalId": id, "rows": 40, "cols": 120 }),
    )
    .unwrap_or_else(|e| panic!("resize_terminal が失敗した: {e}"));
    call(
        handle,
        window,
        "ack_terminal_output",
        json!({ "terminalId": id, "bytes": 9 }),
    )
    .unwrap_or_else(|e| panic!("ack_terminal_output が失敗した: {e}"))
    .deserialize::<()>()
    .unwrap_or_else(|e| panic!("戻り値が null ではなかった: {e}"));

    assert_eq!(
        lock(&ctx.control.written).as_slice(),
        "echo あ\r".as_bytes()
    );
    assert_eq!(
        lock(&ctx.control.sizes).as_slice(),
        &[PtySize {
            rows: 40,
            cols: 120
        }]
    );

    let zero = error_text(call(
        handle,
        window,
        "resize_terminal",
        json!({ "terminalId": id, "rows": 40, "cols": 0 }),
    ));
    assert!(zero.contains("寸法"), "{zero}");
}

fn バイト列の入力がそのまま_pty_に届く(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let id = terminal_id(handle, window);
    let before = lock(&ctx.control.written).len();
    // xterm.js の `onBinary`(古い形式のマウス報告)。UTF-8 にならないバイトを含む。
    let bytes: Vec<u8> = vec![0x1b, b'[', b'M', b' ', 0xff, 0x80, 0x00];

    call(
        handle,
        window,
        "write_terminal_bytes",
        json!({ "terminalId": id, "data": bytes }),
    )
    .unwrap_or_else(|e| panic!("write_terminal_bytes が失敗した: {e}"))
    .deserialize::<()>()
    .unwrap_or_else(|e| panic!("戻り値が null ではなかった: {e}"));

    assert_eq!(&lock(&ctx.control.written)[before..], bytes.as_slice());

    let out_of_range = error_text(call(
        handle,
        window,
        "write_terminal_bytes",
        json!({ "terminalId": id, "data": [256] }),
    ));
    assert!(!out_of_range.is_empty());
    assert_eq!(lock(&ctx.control.written).len(), before + bytes.len());
}

fn 検証画面ではタブを選べて閉じられ_新しいタブは開けない(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let before = snapshot(handle, window);
    let last = before.terminals[8].id.as_str().to_owned();
    let second = before.terminals[1].id.as_str().to_owned();

    call(
        handle,
        window,
        "select_terminal",
        json!({ "terminalId": second }),
    )
    .unwrap_or_else(|e| panic!("select_terminal が失敗した: {e}"));
    call(
        handle,
        window,
        "close_terminal",
        json!({ "terminalId": last }),
    )
    .unwrap_or_else(|e| panic!("close_terminal が失敗した: {e}"));

    let after = snapshot(handle, window);
    assert_eq!(after.terminals.len(), 8);
    assert!(after.terminals.iter().all(|t| t.id.as_str() != last));
    let missing = error_text(call(
        handle,
        window,
        "select_terminal",
        json!({ "terminalId": last }),
    ));
    assert!(missing.contains(&last), "{missing}");
    let no_project = error_text(call(handle, window, "new_terminal", json!({})));
    assert!(no_project.contains("選ばれていない"), "{no_project}");
    assert_eq!(snapshot(handle, window).terminals.len(), 8);
    assert_eq!(
        lock(&ctx.control.spawned).len(),
        1,
        "閉じた・選んだだけで起動した"
    );
}

fn 二重の接続は起動し直さず新しいチャネルへ流す(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let id = terminal_id(handle, window);
    let second = CHANNEL + 1;

    call(
        handle,
        window,
        "connect_terminal",
        json!({ "terminalId": id, "rows": 24, "cols": 80, "onOutput": format!("__CHANNEL__:{second}") }),
    )
    .unwrap_or_else(|e| panic!("2 回目の connect_terminal が失敗した: {e}"));
    ctx.control.feed(b"again");

    assert!(wait_until(|| ctx.received(second) == b"again"));
    assert_eq!(lock(&ctx.control.spawned).len(), 1, "起動し直した");
}

fn 終了の処理でシェルを終わらせる(
    handle: &AppHandle<MockRuntime>,
    _window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    arin_lib::lifecycle::on_run_event(handle, RunEvent::Exit);

    assert!(
        ctx.control.killed.load(Ordering::SeqCst),
        "シェルを終わらせなかった"
    );
}

const CHECKS: &[(&str, Check)] = &[
    (
        "写しにまだ起動していないターミナルが載る",
        写しにまだ起動していないターミナルが載る,
    ),
    (
        "存在しない_id_や寸法が_0_の接続は失敗して起動しない",
        存在しない_id_や寸法が_0_の接続は失敗して起動しない,
    ),
    (
        "接続でシェルを起動し_接続の前の出力からチャネルに届く",
        接続でシェルを起動し_接続の前の出力からチャネルに届く,
    ),
    (
        "入力と寸法の変更と処理済みの通知が届く",
        入力と寸法の変更と処理済みの通知が届く,
    ),
    (
        "バイト列の入力がそのまま_pty_に届く",
        バイト列の入力がそのまま_pty_に届く,
    ),
    (
        "二重の接続は起動し直さず新しいチャネルへ流す",
        二重の接続は起動し直さず新しいチャネルへ流す,
    ),
    (
        "検証画面ではタブを選べて閉じられ_新しいタブは開けない",
        検証画面ではタブを選べて閉じられ_新しいタブは開けない,
    ),
    (
        "終了の処理でシェルを終わらせる",
        終了の処理でシェルを終わらせる,
    ),
];

// ---------------------------------------------------------------------------
// プロジェクトのタブ(通常の起動。T33b)
// ---------------------------------------------------------------------------

/// 状態ファイルに書くプロジェクトの ID。
const PROJECT_ID: &str = "3f2b8c1e-4d5a-4b6c-8d7e-9f0a1b2c3d4e";

/// `bookmark` が null のプロジェクト(ルート `root`)を 1 件持ち、それを選んだ状態ファイルを `state_dir` に置く。
fn write_project_state(state_dir: &Path, root: &Path) {
    let text = serde_json::to_string_pretty(&json!({
        "schemaVersion": 1,
        "projects": [{
            "id": PROJECT_ID,
            "bookmark": null,
            "lastKnownPath": root,
            "customName": null,
            "colorId": "green"
        }],
        "ui": { "selectedProjectId": PROJECT_ID }
    }))
    .expect("JSON にできなかった");
    fs::write(state_dir.join("state.json"), text).expect("状態ファイルを置けなかった");
}

fn project_tabs(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
) -> Vec<String> {
    snapshot(handle, window).projects[0]
        .terminal_ids
        .iter()
        .map(|id| id.as_str().to_owned())
        .collect()
}

fn 新しいタブを開いて接続するとルートでシェルが起動し_閉じるとシェルが終わる(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let first = project_tabs(handle, window);
    assert_eq!(first.len(), 1, "起動時のタブは 1 枚");

    let opened: String = call(handle, window, "new_terminal", json!({}))
        .unwrap_or_else(|e| panic!("new_terminal が失敗した: {e}"))
        .deserialize()
        .unwrap_or_else(|e| panic!("戻り値がターミナルの ID ではなかった: {e}"));

    assert_eq!(
        project_tabs(handle, window),
        vec![first[0].clone(), opened.clone()]
    );
    let s = snapshot(handle, window);
    assert_eq!(
        s.projects[0]
            .active_terminal_id
            .as_ref()
            .map(|id| id.as_str()),
        Some(opened.as_str())
    );
    assert!(
        wait_until(|| lock(&ctx.events)
            .iter()
            .any(|s| s.terminals.iter().any(|t| t.id.as_str() == opened))),
        "開いたことが写しのイベントで届かなかった"
    );
    assert!(
        lock(&ctx.control.spawned).is_empty(),
        "開いただけで起動した"
    );

    call(
        handle,
        window,
        "connect_terminal",
        json!({ "terminalId": opened, "rows": 24, "cols": 80, "onOutput": format!("__CHANNEL__:{CHANNEL}") }),
    )
    .unwrap_or_else(|e| panic!("connect_terminal が失敗した: {e}"));
    let root = ctx.root.as_deref().expect("プロジェクトのルート");
    let canonical = fs::canonicalize(root).expect("ルートを正規化できなかった");
    {
        let spawned = lock(&ctx.control.spawned);
        assert_eq!(spawned.len(), 1);
        assert_eq!(
            spawned[0].0.cwd(),
            canonical.as_path(),
            "cwd がプロジェクトのルートでない"
        );
    }

    call(
        handle,
        window,
        "select_terminal",
        json!({ "terminalId": first[0] }),
    )
    .unwrap_or_else(|e| panic!("select_terminal が失敗した: {e}"));
    assert_eq!(
        snapshot(handle, window).projects[0]
            .active_terminal_id
            .as_ref()
            .map(|id| id.as_str().to_owned()),
        Some(first[0].clone())
    );

    call(
        handle,
        window,
        "close_terminal",
        json!({ "terminalId": opened }),
    )
    .unwrap_or_else(|e| panic!("close_terminal が失敗した: {e}"))
    .deserialize::<()>()
    .unwrap_or_else(|e| panic!("戻り値が null ではなかった: {e}"));

    assert!(
        ctx.control.killed.load(Ordering::SeqCst),
        "シェルを終わらせなかった"
    );
    assert_eq!(project_tabs(handle, window), first);
    let again = error_text(call(
        handle,
        window,
        "connect_terminal",
        json!({ "terminalId": opened, "rows": 24, "cols": 80, "onOutput": format!("__CHANNEL__:{}", CHANNEL + 9) }),
    ));
    assert!(again.contains(&opened), "{again}");
    assert_eq!(
        lock(&ctx.control.spawned).len(),
        1,
        "閉じたタブで起動し直した"
    );
}

fn 最後の_1_枚を閉じたプロジェクトにも新しいタブを開ける(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    _ctx: &Ctx,
) {
    for id in project_tabs(handle, window) {
        call(
            handle,
            window,
            "close_terminal",
            json!({ "terminalId": id }),
        )
        .unwrap_or_else(|e| panic!("close_terminal が失敗した: {e}"));
    }
    assert!(project_tabs(handle, window).is_empty());

    let opened: String = call(handle, window, "new_terminal", json!({}))
        .unwrap_or_else(|e| panic!("new_terminal が失敗した: {e}"))
        .deserialize()
        .unwrap_or_else(|e| panic!("戻り値がターミナルの ID ではなかった: {e}"));

    assert_eq!(project_tabs(handle, window), vec![opened]);
}

const PROJECT_CHECKS: &[(&str, Check)] = &[
    (
        "新しいタブを開いて接続するとルートでシェルが起動し_閉じるとシェルが終わる",
        新しいタブを開いて接続するとルートでシェルが起動し_閉じるとシェルが終わる,
    ),
    (
        "最後の_1_枚を閉じたプロジェクトにも新しいタブを開ける",
        最後の_1_枚を閉じたプロジェクトにも新しいタブを開ける,
    ),
];

/// 組み立てて 1 回だけ起動し、メインのウィンドウができたところで `checks` を順に行ってから閉じる。
/// 通った確認の数を返す。
fn run_checks(
    launch: arin_lib::launch::LaunchConfig,
    root: Option<PathBuf>,
    checks: &'static [(&'static str, Check)],
) -> usize {
    let control = Arc::new(Control::default());
    let received: Arc<Mutex<HashMap<u32, Vec<u8>>>> = Arc::default();
    let sink = Arc::clone(&received);

    let launch = launch.with_terminal_env(arin_lib::launch::TerminalEnv {
        home: Some(PathBuf::from("/Users/someone")),
        user: None,
        vars: vec![("HOME".into(), "/Users/someone".into())],
    });
    let app = arin_lib::builder_with_pty::<MockRuntime>(
        launch,
        Arc::new(FakePty(Arc::clone(&control))),
        Arc::new(|| ShellDefaults {
            shell: PathBuf::from("/bin/zsh"),
            locale: None,
        }),
    )
    .channel_interceptor(move |_webview, callback, _index, body| {
        if let InvokeResponseBody::Raw(bytes) = body {
            lock(&sink)
                .entry(callback.0)
                .or_default()
                .extend_from_slice(bytes);
        }
        true
    })
    .build(tauri::generate_context!())
    .unwrap_or_else(|e| panic!("組み立てに失敗した: {e}"));

    let ctx = Ctx {
        control,
        received,
        events: Arc::default(),
        root,
    };
    let passed = Rc::new(Cell::new(0_usize));
    let count = Rc::clone(&passed);

    app.run_return(move |handle, event| {
        if let RunEvent::Ready = event {
            let events = Arc::clone(&ctx.events);
            handle.listen(arin_lib::SNAPSHOT_EVENT, move |event| {
                let snapshot: AppSnapshot =
                    serde_json::from_str(event.payload()).expect("写しとして読めなかった");
                lock(&events).push(snapshot);
            });
            let window = handle
                .get_webview_window("main")
                .expect("設定にあるウィンドウ main が作られなかった");
            for (name, check) in checks {
                check(handle, &window, &ctx);
                println!("test {name} ... ok");
                count.set(count.get() + 1);
            }
            for window in handle.webview_windows().into_values() {
                window.destroy().expect("ウィンドウを閉じられなかった");
            }
        }
        // 終了の処理(セッションの後片付けと保存)は本物と同じ経路で行う(2 回目以降は何もしない)。
        arin_lib::lifecycle::on_run_event(handle, event);
    });
    passed.get()
}

fn main() {
    let grid_dir = support::TempDir::new("terminal");
    let passed = run_checks(
        support::launch_verify_grid_in(&grid_dir, false),
        None,
        CHECKS,
    );
    assert_eq!(passed, CHECKS.len(), "起動の完了まで進まなかった(検証画面)");

    let project_dir = support::TempDir::new("terminal-project");
    let state_dir = project_dir.path().join("state");
    let root = project_dir.path().join("project");
    fs::create_dir(&state_dir).expect("状態フォルダを作れなかった");
    fs::create_dir(&root).expect("プロジェクトのフォルダを作れなかった");
    write_project_state(&state_dir, &root);
    let launch =
        arin_lib::launch::LaunchConfig::from_inputs(None, Some(state_dir.as_os_str()), None)
            .unwrap_or_else(|e| panic!("組み立ての設定を作れなかった: {e}"));
    let passed = run_checks(launch, Some(root), PROJECT_CHECKS);
    assert_eq!(
        passed,
        PROJECT_CHECKS.len(),
        "起動の完了まで進まなかった(プロジェクト)"
    );
}
