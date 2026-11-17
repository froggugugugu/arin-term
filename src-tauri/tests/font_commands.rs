//! 見た目のコマンド(`src-tauri/src/commands/layout.rs` の `step_font_size`・`step_translucency`・`reset_appearance`)。
//! 文字の大きさは計画書 T68c・ADR-013 の 2(`delta` で動かす・変わったときだけ写しを送る・保存は既存の保存のスレッド)、
//! AC-F02(Rust 側)・F06・F07・F08。透過と「既定に戻す」は ADR-014 の 1(AC-G01・G04・G05 の Rust 側)。
//!
//! - 状態フォルダに 1 プロジェクトの `state.json` を置いて起動する(起動時にタブが 1 枚できる)
//! - PTY は偽物(起動・寸法・終了を記録する)。シェルが続くことは、偽の PTY が終了を受けていないことと写しの状態で確かめる
//! - 写しは `app-snapshot` の受け口で数え、届いた写しを覚える(変わったときだけ送ることを確かめる)
//! - 1 回の起動の中で、確認を順に行う(前の確認の状態を引き継ぐ。各確認の頭に前提を書く)
//!
//! macOS ではメニューの部品をメインスレッドでしか作れないため、`harness = false` で走らせる(`app_builder` と同じ)。

mod support;

use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use arin_core::layout::font_sizes::{
    DEFAULT_INTERFACE_FONT_SIZE, DEFAULT_TERMINAL_FONT_SIZE, MAX_TERMINAL_FONT_SIZE,
    MIN_INTERFACE_FONT_SIZE,
};
use arin_core::layout::translucency::{
    DEFAULT_TERMINAL_TRANSLUCENCY, DEFAULT_WINDOW_TRANSLUCENCY, MAX_TERMINAL_TRANSLUCENCY,
    MIN_WINDOW_TRANSLUCENCY,
};
use arin_core::ports::{ChildExit, PtyError, PtyPort, PtySession, PtySize, SpawnedPty};
use arin_core::terminal::{ShellLaunchSpec, TerminalStatus};
use arin_core::workspace::AppSnapshot;
use arin_lib::SNAPSHOT_EVENT;
use arin_lib::sessions::ShellDefaults;
use serde_json::json;
use support::{request, request_with};
use tauri::ipc::{InvokeBody, InvokeResponseBody};
use tauri::test::{MockRuntime, get_ipc_response};
use tauri::{AppHandle, Listener, Manager, RunEvent, WebviewWindow};

const WAIT: Duration = Duration::from_secs(10);
/// 「来ない」ことを確かめるときに待つ時間。
const QUIET: Duration = Duration::from_millis(200);
const CHANNEL: u32 = 4343;

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn wait_until(mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        thread::sleep(Duration::from_millis(5));
    }
    condition()
}

// ---------------------------------------------------------------------------
// 偽の PTY
// ---------------------------------------------------------------------------

/// 偽の PTY の記録と操作。
#[derive(Default)]
struct Control {
    spawned: AtomicUsize,
    feed: Mutex<Option<Sender<Vec<u8>>>>,
    killed: AtomicBool,
}

struct FakePty(Arc<Control>);

impl PtyPort for FakePty {
    fn spawn(&self, _spec: &ShellLaunchSpec, _size: PtySize) -> Result<SpawnedPty, PtyError> {
        self.0.spawned.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel();
        tx.send(b"% ".to_vec()).expect("読み取り側がいない");
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
    fn write(&mut self, _data: &[u8]) -> Result<(), PtyError> {
        Ok(())
    }

    fn resize(&mut self, _size: PtySize) -> Result<(), PtyError> {
        Ok(())
    }

    fn kill(&mut self) -> Result<(), PtyError> {
        self.0.killed.store(true, Ordering::SeqCst);
        lock(&self.0.feed).take();
        Ok(())
    }

    fn try_wait(&mut self) -> Result<Option<ChildExit>, PtyError> {
        Ok(None)
    }
}

// ---------------------------------------------------------------------------
// 組み立て
// ---------------------------------------------------------------------------

/// 確かめるときに使うもの。
struct Ctx<'a> {
    handle: &'a AppHandle<MockRuntime>,
    window: &'a WebviewWindow<MockRuntime>,
    state_dir: &'a std::path::Path,
    control: &'a Control,
    /// `app-snapshot` で届いた写し(届いた順)。
    events: &'a Mutex<Vec<AppSnapshot>>,
}

impl Ctx<'_> {
    fn snapshot(&self) -> AppSnapshot {
        get_ipc_response(self.window, request(self.handle, "get_app_snapshot"))
            .unwrap_or_else(|e| panic!("get_app_snapshot が失敗した: {e}"))
            .deserialize()
            .unwrap_or_else(|e| panic!("AppSnapshot として読めなかった: {e}"))
    }

    fn call(&self, cmd: &str, args: serde_json::Value) -> Result<InvokeResponseBody, String> {
        get_ipc_response(
            self.window,
            request_with(self.handle, cmd, InvokeBody::Json(args)),
        )
        .map_err(|e| e.to_string())
    }

    fn ok(&self, cmd: &str, args: serde_json::Value) {
        self.call(cmd, args)
            .unwrap_or_else(|e| panic!("{cmd} が失敗した: {e}"));
    }

    fn step(&self, target: &str, delta: i32) {
        self.ok(
            "step_font_size",
            json!({ "target": target, "delta": delta }),
        );
    }

    fn step_translucency(&self, target: &str, delta: i32) {
        self.ok(
            "step_translucency",
            json!({ "target": target, "delta": delta }),
        );
    }

    fn sent(&self) -> usize {
        lock(self.events).len()
    }

    /// 最後に届いた写し。
    fn last_event(&self) -> Option<AppSnapshot> {
        lock(self.events).last().cloned()
    }

    /// `f` のあいだに写しが送られなかったこと。
    fn assert_not_sent(&self, what: &str, f: impl FnOnce()) {
        let before = self.sent();
        f();
        thread::sleep(QUIET);
        assert_eq!(self.sent(), before, "{what}で写しを送った");
    }

    /// `f` のあいだに写しが送られ、最後に届いた写しが `expected` を満たすこと。
    fn assert_sent(&self, what: &str, f: impl FnOnce(), expected: impl Fn(&AppSnapshot) -> bool) {
        let before = self.sent();
        f();
        assert!(
            wait_until(|| self.sent() > before && self.last_event().is_some_and(|s| expected(&s))),
            "{what}で期待した写しが届かない: {:?}",
            self.last_event().map(|s| (s.font_sizes, s.translucency))
        );
    }

    /// 状態ファイルの `ui`。
    fn saved_ui(&self) -> Option<serde_json::Value> {
        let text = fs::read_to_string(self.state_dir.join(support::STATE_FILE_NAME)).ok()?;
        let value: serde_json::Value = serde_json::from_str(&text).ok()?;
        value.get("ui").cloned()
    }

    fn wait_saved(&self, what: &str, condition: impl Fn(&serde_json::Value) -> bool) {
        assert!(
            wait_until(|| self.saved_ui().is_some_and(|ui| condition(&ui))),
            "{what}が状態ファイルに書かれない: {:?}",
            self.saved_ui()
        );
    }
}

// ---------------------------------------------------------------------------
// 確認(順に行う)
// ---------------------------------------------------------------------------

/// 前提: 起動直後(どちらも既定)。ターミナルに接続してシェルを起動しておく(最後の確認で、続いていることを見る)。
fn 刻むと写しが届き保存される(c: &Ctx) {
    let id = c.snapshot().terminals[0].id.as_str().to_owned();
    c.ok(
        "connect_terminal",
        json!({ "terminalId": id, "rows": 24, "cols": 80, "onOutput": format!("__CHANNEL__:{CHANNEL}") }),
    );
    assert!(
        wait_until(|| c.snapshot().terminals[0].status == TerminalStatus::Running),
        "シェルが起動しない"
    );
    let before = c.snapshot().font_sizes;
    assert_eq!(before.terminal.value, DEFAULT_TERMINAL_FONT_SIZE);
    assert_eq!(before.interface.value, DEFAULT_INTERFACE_FONT_SIZE);
    // 既定のあいだは書かない。
    assert!(
        c.saved_ui()
            .is_none_or(|ui| ui.get("terminalFontSize").is_none()),
        "{:?}",
        c.saved_ui()
    );

    c.assert_sent(
        "ターミナルの文字を 1 つ大きくする",
        || c.step("terminal", 1),
        |s| s.font_sizes.terminal.value == DEFAULT_TERMINAL_FONT_SIZE + 1,
    );
    let s = c.snapshot().font_sizes;
    assert_eq!(s.terminal.value, DEFAULT_TERMINAL_FONT_SIZE + 1);
    assert_eq!(
        s.interface.value, DEFAULT_INTERFACE_FONT_SIZE,
        "対象でない大きさが変わった"
    );
    c.wait_saved("ターミナルの文字の大きさ", |ui| {
        ui["terminalFontSize"] == json!(DEFAULT_TERMINAL_FONT_SIZE + 1)
            && ui.get("interfaceFontSize").is_none()
    });
}

/// 前提: ターミナル 16・画面 13(既定)。写しを待たずに続けて 3 回送る(画面が「今の値 + 1」を送る作りだと 1 つしか上がらない)。
fn 続けて_3_回刻むと_3_上がる(c: &Ctx) {
    c.assert_sent(
        "画面の文字を続けて 3 回大きくする",
        || {
            for _ in 0..3 {
                c.step("interface", 1);
            }
        },
        |s| s.font_sizes.interface.value == DEFAULT_INTERFACE_FONT_SIZE + 3,
    );
    assert_eq!(
        c.snapshot().font_sizes.interface.value,
        DEFAULT_INTERFACE_FONT_SIZE + 3
    );
    c.wait_saved("画面の文字の大きさ", |ui| {
        ui["interfaceFontSize"] == json!(DEFAULT_INTERFACE_FONT_SIZE + 3)
    });
}

/// 前提: ターミナル 16・画面 16。大きく動かして端に留めてから、さらに端の外へ刻む。
fn 端では写しを送らない(c: &Ctx) {
    c.assert_sent(
        "ターミナルを最大の外まで動かす",
        || c.step("terminal", 100),
        |s| s.font_sizes.terminal.value == MAX_TERMINAL_FONT_SIZE,
    );
    c.assert_sent(
        "画面を最小の外まで動かす",
        || c.step("interface", -100),
        |s| s.font_sizes.interface.value == MIN_INTERFACE_FONT_SIZE,
    );
    let before = c.snapshot();
    c.assert_not_sent("範囲の端の外への刻みと 0 の刻み", || {
        c.step("terminal", 1);
        c.step("interface", -1);
        c.step("terminal", 0);
    });
    assert_eq!(c.snapshot().font_sizes, before.font_sizes);
    c.wait_saved("端の値", |ui| {
        ui["terminalFontSize"] == json!(MAX_TERMINAL_FONT_SIZE)
            && ui["interfaceFontSize"] == json!(MIN_INTERFACE_FONT_SIZE)
    });
}

/// 前提: ターミナル 28・画面 11。知らない対象・引数の無い呼び出しは、引数の読み取りで失敗する。
fn 知らない対象は失敗を返し何も変えない(c: &Ctx) {
    let before = c.snapshot();
    c.assert_not_sent("知らない対象", || {
        assert!(
            c.call("step_font_size", json!({ "target": "editor", "delta": 1 }))
                .is_err(),
            "知らない対象を受け付けた"
        );
        assert!(
            c.call("step_font_size", json!({ "target": "terminal" }))
                .is_err(),
            "delta の無い呼び出しを受け付けた"
        );
    });
    assert_eq!(c.snapshot().font_sizes, before.font_sizes);
}

/// 前提: 文字の大きさはターミナル 28・画面 11、透過は既定。透過を動かすと写しが届き、キーごとに保存される。
fn 透過を動かすと写しが届き保存される(c: &Ctx) {
    let before = c.snapshot().translucency;
    assert_eq!(before.window.value, DEFAULT_WINDOW_TRANSLUCENCY);
    assert_eq!(before.terminal.value, DEFAULT_TERMINAL_TRANSLUCENCY);
    assert_eq!(before.window.step, 5);
    assert!(
        c.saved_ui()
            .is_none_or(|ui| ui.get("windowTranslucency").is_none()
                && ui.get("terminalTranslucency").is_none()),
        "{:?}",
        c.saved_ui()
    );

    c.assert_sent(
        "ターミナルの背景の透過を 5 上げる",
        || c.step_translucency("terminal", 5),
        |s| s.translucency.terminal.value == DEFAULT_TERMINAL_TRANSLUCENCY + 5,
    );
    assert_eq!(
        c.snapshot().translucency.window.value,
        DEFAULT_WINDOW_TRANSLUCENCY,
        "対象でない透過が変わった"
    );
    c.wait_saved("ターミナルの背景の透過", |ui| {
        ui["terminalTranslucency"] == json!(DEFAULT_TERMINAL_TRANSLUCENCY + 5)
            && ui.get("windowTranslucency").is_none()
    });

    // 写しを待たずに続けて 3 回下げる(回数どおり下がる)
    c.assert_sent(
        "ウィンドウの透過を続けて 3 回下げる",
        || {
            for _ in 0..3 {
                c.step_translucency("window", -5);
            }
        },
        |s| s.translucency.window.value == DEFAULT_WINDOW_TRANSLUCENCY - 15,
    );
    c.wait_saved("ウィンドウの透過", |ui| {
        ui["windowTranslucency"] == json!(DEFAULT_WINDOW_TRANSLUCENCY - 15)
    });
}

/// 前提: ウィンドウ 30・ターミナル 25。端に留めてから端の外へ動かしても、5 の倍数に丸めて同じ値になっても写しを送らない。
fn 透過は端と丸めで変わらなければ写しを送らない(c: &Ctx) {
    c.assert_sent(
        "ターミナルの背景を最大の外まで動かす",
        || c.step_translucency("terminal", 1000),
        |s| s.translucency.terminal.value == MAX_TERMINAL_TRANSLUCENCY,
    );
    c.assert_sent(
        "ウィンドウを最小の外まで動かす",
        || c.step_translucency("window", -1000),
        |s| s.translucency.window.value == MIN_WINDOW_TRANSLUCENCY,
    );
    let before = c.snapshot();
    c.assert_not_sent(
        "端の外・0・丸めると同じ値になる差分",
        || {
            c.step_translucency("terminal", 5);
            c.step_translucency("window", -5);
            c.step_translucency("window", 0);
            c.step_translucency("window", 2);
        },
    );
    assert_eq!(c.snapshot().translucency, before.translucency);
    assert!(
        c.call(
            "step_translucency",
            json!({ "target": "sidebar", "delta": 5 })
        )
        .is_err(),
        "知らない対象を受け付けた"
    );
    c.wait_saved("端の値", |ui| {
        ui["terminalTranslucency"] == json!(MAX_TERMINAL_TRANSLUCENCY)
            && ui["windowTranslucency"] == json!(MIN_WINDOW_TRANSLUCENCY)
    });
}

/// 前提: 文字の大きさはターミナル 28・画面 11、透過はウィンドウ 0・ターミナル 80。既定に戻すと 4 つとも既定になり、
/// 状態ファイルから 4 つのキーが消える。
fn 既定に戻すと文字の大きさと透過が既定の写しが届く(c: &Ctx) {
    c.assert_sent(
        "既定に戻す",
        || c.ok("reset_appearance", json!({})),
        |s| {
            s.font_sizes.terminal.value == DEFAULT_TERMINAL_FONT_SIZE
                && s.font_sizes.interface.value == DEFAULT_INTERFACE_FONT_SIZE
                && s.translucency.window.value == DEFAULT_WINDOW_TRANSLUCENCY
                && s.translucency.terminal.value == DEFAULT_TERMINAL_TRANSLUCENCY
        },
    );
    c.wait_saved("既定に戻したこと", |ui| {
        ui.get("terminalFontSize").is_none()
            && ui.get("interfaceFontSize").is_none()
            && ui.get("windowTranslucency").is_none()
            && ui.get("terminalTranslucency").is_none()
    });
    c.assert_not_sent("既定のときの既定に戻す", || {
        c.ok("reset_appearance", json!({}));
    });
    assert!(
        c.call("reset_font_sizes", json!({})).is_err(),
        "古い名前のコマンドが残っている"
    );
}

/// 前提: 最初の確認でシェルを起動し、その後に大きさを何度も変えた。
fn 文字の大きさと透過を変えてもシェルは続く(c: &Ctx) {
    c.step("terminal", -2);
    c.step_translucency("terminal", -10);
    c.ok("reset_appearance", json!({}));
    thread::sleep(QUIET);
    let s = c.snapshot();
    assert_eq!(s.terminals.len(), 1, "{s:?}");
    assert_eq!(s.terminals[0].status, TerminalStatus::Running);
    assert_eq!(
        c.control.spawned.load(Ordering::SeqCst),
        1,
        "シェルを起動し直した"
    );
    assert!(
        !c.control.killed.load(Ordering::SeqCst),
        "シェルを終わらせた"
    );
}

type Check = fn(&Ctx);

const CHECKS: &[(&str, Check)] = &[
    ("刻むと写しが届き保存される", 刻むと写しが届き保存される),
    ("続けて_3_回刻むと_3_上がる", 続けて_3_回刻むと_3_上がる),
    ("端では写しを送らない", 端では写しを送らない),
    (
        "知らない対象は失敗を返し何も変えない",
        知らない対象は失敗を返し何も変えない,
    ),
    (
        "透過を動かすと写しが届き保存される",
        透過を動かすと写しが届き保存される,
    ),
    (
        "透過は端と丸めで変わらなければ写しを送らない",
        透過は端と丸めで変わらなければ写しを送らない,
    ),
    (
        "既定に戻すと文字の大きさと透過が既定の写しが届く",
        既定に戻すと文字の大きさと透過が既定の写しが届く,
    ),
    (
        "文字の大きさと透過を変えてもシェルは続く",
        文字の大きさと透過を変えてもシェルは続く,
    ),
];

fn main() {
    let state = support::TempDir::new("font-commands");
    let work = support::TempDir::new("font-commands-work");
    let roots = support::make_project_roots(&work, 1);
    let launch = support::launch_with_projects_in(&state, &roots, false).with_terminal_env(
        arin_lib::launch::TerminalEnv {
            home: Some(PathBuf::from("/Users/someone")),
            user: None,
            vars: vec![("HOME".into(), "/Users/someone".into())],
        },
    );
    let control = Arc::new(Control::default());
    let app = arin_lib::builder_with_pty::<MockRuntime>(
        launch,
        Arc::new(FakePty(Arc::clone(&control))),
        Arc::new(|| ShellDefaults {
            shell: PathBuf::from("/bin/zsh"),
            locale: None,
        }),
    )
    // 出力のチャネルへの送信は受け取って捨てる(このテストは出力を見ない)。
    .channel_interceptor(|_webview, _callback, _index, _body| true)
    .build(tauri::generate_context!())
    .unwrap_or_else(|e| panic!("組み立てに失敗した: {e}"));

    let passed = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&passed);
    let exited = Arc::new(AtomicBool::new(false));
    let seen_exit = Arc::clone(&exited);
    let state_dir = state.path().to_path_buf();
    app.run_return(move |handle, event| {
        if let RunEvent::Ready = event {
            let window = handle
                .get_webview_window("main")
                .expect("設定にあるウィンドウ main が作られなかった");
            let events: Arc<Mutex<Vec<AppSnapshot>>> = Arc::default();
            let sink = Arc::clone(&events);
            handle.listen(SNAPSHOT_EVENT, move |event| {
                let snapshot: AppSnapshot =
                    serde_json::from_str(event.payload()).expect("写しとして読めなかった");
                lock(&sink).push(snapshot);
            });
            let ctx = Ctx {
                handle,
                window: &window,
                state_dir: &state_dir,
                control: &control,
                events: &events,
            };
            for (name, check) in CHECKS {
                check(&ctx);
                println!("test {name} ... ok");
                count.fetch_add(1, Ordering::SeqCst);
            }
            for window in handle.webview_windows().into_values() {
                window.destroy().expect("ウィンドウを閉じられなかった");
            }
        }
        let exiting = matches!(event, RunEvent::Exit);
        arin_lib::lifecycle::on_run_event(handle, event);
        if exiting {
            seen_exit.store(true, Ordering::SeqCst);
        }
    });
    assert!(exited.load(Ordering::SeqCst), "終了のイベントが来なかった");
    assert_eq!(
        passed.load(Ordering::SeqCst),
        CHECKS.len(),
        "起動の完了まで進まなかった"
    );
    let _ = io::Write::flush(&mut io::stdout());
    println!("test result: ok. {} passed", CHECKS.len());
}
