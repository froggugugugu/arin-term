//! 終了の確認(FR-063、計画書 T33e、設計書 §11「終了時」)と、終了時の後片付け(M1 の見送り)を確かめる。
//!
//! - 終了の要求(ウィンドウの閉じるボタン=模擬ランタイムの `close`、メニューの「終了」)で、動作中のターミナル
//!   (前面がシェル以外)が無ければ止めずに終わり、あれば終了を止めて `quit-requested` を送る
//! - 画面の `confirm_quit` で終わる。終わるときは状態を書いてから全セッションを終わらせる(`lifecycle.rs` の順)
//! - 後片付けは全セッションへ先に終了を送り、待ちをまとめる(9 枚で 1 枚ぶんほど)
//!
//! PTY は偽物。状態ファイルは一時フォルダの本物(`arin-platform`)で、時計だけを止める(まとめの待ちでは書かれず、
//! 終了の契機でだけ書かれる)。模擬ランタイムのループは 1 回ごとに 1 秒休むので、手順は回数で進める。
//!
//! macOS ではメニューの部品をメインスレッドでしか作れないため、`harness = false` で走らせる(`app_builder` と同じ)。

mod support;

use std::cell::Cell;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use arin_core::ports::clock::Clock;
use arin_core::ports::{ChildExit, Foreground, PtyError, PtyPort, PtySession, PtySize, SpawnedPty};
use arin_core::terminal::{ShellLaunchSpec, TerminalId};
use arin_core::workspace::QuitRequest;
use arin_lib::events::QUIT_REQUESTED_EVENT;
use arin_lib::launch::LaunchConfig;
use arin_lib::menu::{self, QUIT_ID};
use arin_lib::persistence::PersistencePorts;
use arin_lib::sessions::{OutputSink, SessionConfig, SessionRegistry, ShellDefaults};
use serde_json::json;
use support::request;
use tauri::test::{MockRuntime, get_ipc_response};
use tauri::{AppHandle, Listener, Manager, RunEvent};

const PROJECT_ID: &str = "3f2b8c1e-4d5a-4b6c-8d7e-9f0a1b2c3d4e";

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// 止まった時計。まとめの待ちが終わらないので、待たずに書く契機(終了など)だけで書かれる。
struct FrozenClock;

impl Clock for FrozenClock {
    fn monotonic(&self) -> Duration {
        Duration::ZERO
    }

    fn since_unix_epoch(&self) -> Duration {
        Duration::from_secs(1_794_735_000)
    }
}

/// 偽の PTY の記録と操作(1 枚ぶん)。
struct Control {
    feed: Mutex<Option<Sender<Vec<u8>>>>,
    foreground: Mutex<Foreground>,
    kills: AtomicUsize,
    /// 終わらせたときに、状態ファイルにブックマークが書かれていたか(保存 → 後片付けの順の確認)。
    saved_before_kill: Mutex<Option<bool>>,
    state_file: PathBuf,
}

impl Control {
    fn new(state_file: PathBuf, foreground: Foreground) -> Arc<Self> {
        Arc::new(Self {
            feed: Mutex::new(None),
            foreground: Mutex::new(foreground),
            kills: AtomicUsize::new(0),
            saved_before_kill: Mutex::new(None),
            state_file,
        })
    }
}

struct FakePty(Arc<Control>);

impl PtyPort for FakePty {
    fn spawn(&self, _spec: &ShellLaunchSpec, _size: PtySize) -> Result<SpawnedPty, PtyError> {
        let (tx, rx) = mpsc::channel();
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
        *lock(&self.0.saved_before_kill) = Some(has_bookmark(&self.0.state_file));
        self.0.kills.fetch_add(1, Ordering::SeqCst);
        lock(&self.0.feed).take();
        Ok(())
    }

    fn try_wait(&mut self) -> Result<Option<ChildExit>, PtyError> {
        Ok(None)
    }

    fn foreground(&mut self) -> Foreground {
        *lock(&self.0.foreground)
    }
}

/// 出力を捨てる送り先。
struct NullSink;

impl OutputSink for NullSink {
    fn send(&self, _data: Vec<u8>) -> Result<(), String> {
        Ok(())
    }
}

/// プロジェクトを 1 件持つ状態ファイルを置いた一時フォルダ。
struct Fixture {
    root: support::TempDir,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let root = support::TempDir::new(name);
        let state = root.path().join("state");
        let project = root.path().join("project");
        fs::create_dir(&state).expect("状態フォルダを作れなかった");
        fs::create_dir(&project).expect("プロジェクトのフォルダを作れなかった");
        let text = serde_json::to_string_pretty(&json!({
            "schemaVersion": 1,
            "projects": [{
                "id": PROJECT_ID,
                "bookmark": null,
                "lastKnownPath": project,
                "customName": null,
                "colorId": "green"
            }],
            "ui": { "selectedProjectId": PROJECT_ID }
        }))
        .expect("JSON にできなかった");
        fs::write(state.join("state.json"), text).expect("状態ファイルを置けなかった");
        Self { root }
    }

    fn state_dir(&self) -> PathBuf {
        self.root.path().join("state")
    }

    fn state_file(&self) -> PathBuf {
        self.state_dir().join("state.json")
    }

    fn launch(&self) -> LaunchConfig {
        LaunchConfig::from_inputs(None, Some(self.state_dir().as_os_str()), None)
            .unwrap_or_else(|e| panic!("組み立ての設定を作れなかった: {e}"))
    }

    fn ports(&self) -> PersistencePorts {
        PersistencePorts {
            clock: Arc::new(FrozenClock),
            ..PersistencePorts::native(self.state_dir())
        }
    }
}

/// 状態ファイルの 1 件目のプロジェクトにブックマークが入っているか(起動時は null。書かれると入る)。
fn has_bookmark(path: &Path) -> bool {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|v| v["projects"][0]["bookmark"].as_str().map(|s| !s.is_empty()))
        .unwrap_or(false)
}

/// 1 つの起動で起きたこと。
#[derive(Default)]
struct Outcome {
    exited: Cell<bool>,
    /// 届いた `quit-requested` の中身。
    requests: Arc<Mutex<Vec<String>>>,
}

/// 手順の 1 歩。`step` はループの何回目か(0 は準備完了のとき)。真を返すと、それ以上の手順を行わない。
type Step = Box<dyn FnMut(&AppHandle<MockRuntime>, usize) -> bool>;

/// 組み立てて起動し、準備完了のときに表示中のタブを偽の PTY でつなぎ、手順を進める。
/// 終了(`RunEvent::Exit`)まで進んだかなどを返す。手順が長すぎたら失敗させる。
fn run_app(fx: &Fixture, control: &Arc<Control>, mut step: Step) -> Rc<Outcome> {
    let app = arin_lib::builder_with_ports::<MockRuntime>(
        fx.launch(),
        Arc::new(FakePty(Arc::clone(control))),
        Arc::new(|| ShellDefaults {
            shell: PathBuf::from("/bin/zsh"),
            locale: None,
        }),
        fx.ports(),
    )
    .build(tauri::generate_context!())
    .unwrap_or_else(|e| panic!("組み立てに失敗した: {e}"));

    let outcome = Rc::new(Outcome::default());
    let seen = Rc::clone(&outcome);
    let mut count = 0_usize;
    let mut done = false;
    app.run_return(move |handle, event| {
        let is_exit = matches!(event, RunEvent::Exit);
        let tick = match &event {
            RunEvent::Ready => {
                let requests = Arc::clone(&seen.requests);
                handle.listen(QUIT_REQUESTED_EVENT, move |event| {
                    lock(&requests).push(event.payload().to_owned());
                });
                let sessions = handle.state::<SessionRegistry>();
                let id = sessions.snapshot().terminals[0].id.clone();
                sessions
                    .connect(&id, 24, 80, Arc::new(NullSink))
                    .unwrap_or_else(|e| panic!("接続できなかった: {e}"));
                true
            }
            RunEvent::MainEventsCleared => {
                count += 1;
                assert!(count < 10, "手順が終わらない(終了しない)");
                true
            }
            _ => false,
        };
        if tick && !done {
            done = step(handle, count);
        }
        arin_lib::lifecycle::on_run_event(handle, event);
        if is_exit {
            seen.exited.set(true);
        }
    });
    outcome
}

fn main_window(handle: &AppHandle<MockRuntime>) -> tauri::WebviewWindow<MockRuntime> {
    handle
        .get_webview_window("main")
        .expect("ウィンドウ main が無い")
}

fn 動作中が無ければ閉じるボタンの終了は止めず保存してから終わる() {
    let fx = Fixture::new("quit-close-idle");
    let control = Control::new(fx.state_file(), Foreground::Shell);
    let state_file = fx.state_file();
    let outcome = run_app(
        &fx,
        &control,
        Box::new(move |handle, _| {
            assert!(!has_bookmark(&state_file), "終了の前に書いた");
            main_window(handle)
                .close()
                .expect("閉じる要求を送れなかった");
            true
        }),
    );

    assert!(outcome.exited.get(), "終了しなかった");
    assert!(lock(&outcome.requests).is_empty(), "確認を求めた");
    assert!(has_bookmark(&fx.state_file()), "終了のときに書かれていない");
    assert_eq!(
        control.kills.load(Ordering::SeqCst),
        1,
        "シェルを終わらせていない"
    );
    assert_eq!(
        *lock(&control.saved_before_kill),
        Some(true),
        "状態を書く前にシェルを終わらせた"
    );
}

fn 動作中が無ければメニューの終了はそのまま終わる() {
    let fx = Fixture::new("quit-menu-idle");
    let control = Control::new(fx.state_file(), Foreground::Shell);
    let outcome = run_app(
        &fx,
        &control,
        Box::new(|handle, _| {
            menu::on_menu_event(handle, QUIT_ID);
            true
        }),
    );

    assert!(outcome.exited.get(), "終了しなかった");
    assert!(lock(&outcome.requests).is_empty(), "確認を求めた");
    assert!(has_bookmark(&fx.state_file()), "終了のときに書かれていない");
    assert_eq!(control.kills.load(Ordering::SeqCst), 1);
}

fn 動作中があれば終了を止め確認を求め_取り消すと何も変わらず_確認すると保存してから終わる() {
    let fx = Fixture::new("quit-running");
    let control = Control::new(fx.state_file(), Foreground::Other);
    let state_file = fx.state_file();
    let watched = Arc::clone(&control);
    let outcome = run_app(
        &fx,
        &control,
        Box::new(move |handle, step| {
            match step {
                0 => {
                    main_window(handle)
                        .close()
                        .expect("閉じる要求を送れなかった");
                    false
                }
                1 => {
                    // 閉じる要求は止められ、ウィンドウもセッションも残る。
                    assert!(
                        handle.get_webview_window("main").is_some(),
                        "ウィンドウが閉じた"
                    );
                    assert_eq!(
                        watched.kills.load(Ordering::SeqCst),
                        0,
                        "シェルを終わらせた"
                    );
                    assert!(!has_bookmark(&state_file), "確認の前に書いた");
                    // メニューの「終了」でも同じく止めて確認を求める。
                    menu::on_menu_event(handle, QUIT_ID);
                    false
                }
                2 => {
                    // 取り消し(画面は何も呼ばない)のあとも何も変わっていない。
                    assert!(
                        handle.get_webview_window("main").is_some(),
                        "ウィンドウが閉じた"
                    );
                    assert_eq!(
                        watched.kills.load(Ordering::SeqCst),
                        0,
                        "シェルを終わらせた"
                    );
                    let window = main_window(handle);
                    get_ipc_response(&window, request(handle, "confirm_quit"))
                        .unwrap_or_else(|e| panic!("confirm_quit が失敗した: {e}"))
                        .deserialize::<()>()
                        .unwrap_or_else(|e| panic!("戻り値が null ではなかった: {e}"));
                    true
                }
                _ => true,
            }
        }),
    );

    assert!(outcome.exited.get(), "確認したのに終了しなかった");
    let payloads = lock(&outcome.requests).clone();
    assert_eq!(
        payloads.len(),
        2,
        "閉じるボタンとメニューで 1 回ずつ確認を求める: {payloads:?}"
    );
    for payload in &payloads {
        let request: QuitRequest =
            serde_json::from_str(payload).expect("QuitRequest として読めなかった");
        assert_eq!(request.running.len(), 1, "{request:?}");
        let running = &request.running[0];
        assert_eq!(running.project_name.as_deref(), Some("project"));
        assert_eq!(running.label, "ターミナル 1");
        assert_eq!(
            running.project_id.as_ref().map(|id| id.as_str().to_owned()),
            Some(PROJECT_ID.to_owned())
        );
    }
    assert_eq!(QUIT_REQUESTED_EVENT, "quit-requested");
    assert!(has_bookmark(&fx.state_file()), "終了のときに書かれていない");
    assert_eq!(control.kills.load(Ordering::SeqCst), 1);
    assert_eq!(
        *lock(&control.saved_before_kill),
        Some(true),
        "状態を書く前にシェルを終わらせた"
    );
}

/// 終了に 200ms かかる偽の PTY(9 枚ぶん)。
struct SlowPty {
    killed: Arc<AtomicUsize>,
    feeds: Arc<Mutex<Vec<Sender<Vec<u8>>>>>,
}

impl PtyPort for SlowPty {
    fn spawn(&self, _spec: &ShellLaunchSpec, _size: PtySize) -> Result<SpawnedPty, PtyError> {
        let (tx, rx) = mpsc::channel();
        lock(&self.feeds).push(tx);
        Ok(SpawnedPty {
            session: Box::new(SlowSession {
                killed: Arc::clone(&self.killed),
            }),
            output: Box::new(FakeOutput(rx)),
        })
    }
}

struct SlowSession {
    killed: Arc<AtomicUsize>,
}

impl PtySession for SlowSession {
    fn write(&mut self, _data: &[u8]) -> Result<(), PtyError> {
        Ok(())
    }

    fn resize(&mut self, _size: PtySize) -> Result<(), PtyError> {
        Ok(())
    }

    fn kill(&mut self) -> Result<(), PtyError> {
        // 本物(portable-pty)は SIGHUP のあと最大でおよそ 200ms 待ってから SIGKILL を送る。
        thread::sleep(Duration::from_millis(200));
        self.killed.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn try_wait(&mut self) -> Result<Option<ChildExit>, PtyError> {
        Ok(None)
    }
}

fn 九枚の後片付けがおよそ_1_枚ぶんの時間で終わる() {
    let killed = Arc::new(AtomicUsize::new(0));
    let feeds: Arc<Mutex<Vec<Sender<Vec<u8>>>>> = Arc::default();
    // 9 プロジェクト・各 1 タブ(タイル表示の 9 枚と同じ形。M1 の 9 枚の検証画面の代わり。T49d)。
    let roots: Vec<PathBuf> = (1..=9)
        .map(|n| PathBuf::from(format!("/Users/someone/dev/p{n}")))
        .collect();
    let registry = SessionRegistry::new(
        support::workspace_with_projects(
            "0.0.0-test",
            PathBuf::from("/Users/someone"),
            [("HOME", "/Users/someone")],
            &roots,
        ),
        Arc::new(SlowPty {
            killed: Arc::clone(&killed),
            feeds: Arc::clone(&feeds),
        }),
        Arc::new(|| ShellDefaults {
            shell: PathBuf::from("/bin/zsh"),
            locale: None,
        }),
        Arc::new(|_| {}),
        SessionConfig::default(),
    );
    let ids: Vec<TerminalId> = registry
        .snapshot()
        .terminals
        .iter()
        .map(|t| t.id.clone())
        .collect();
    assert_eq!(ids.len(), 9);
    for id in &ids {
        registry
            .connect(id, 24, 80, Arc::new(NullSink))
            .unwrap_or_else(|e| panic!("{id} に接続できなかった: {e}"));
    }

    let started = Instant::now();
    registry.shutdown();
    let elapsed = started.elapsed();

    assert_eq!(killed.load(Ordering::SeqCst), 9, "全部を終わらせていない");
    assert!(
        elapsed < Duration::from_millis(600),
        "後片付けに {elapsed:?} かかった(1 枚ずつ待っている)"
    );
    drop(feeds);
}

fn main() {
    let tests: [(&str, fn()); 4] = [
        (
            "動作中が無ければ閉じるボタンの終了は止めず保存してから終わる",
            動作中が無ければ閉じるボタンの終了は止めず保存してから終わる,
        ),
        (
            "動作中が無ければメニューの終了はそのまま終わる",
            動作中が無ければメニューの終了はそのまま終わる,
        ),
        (
            "動作中があれば終了を止め確認を求め_取り消すと何も変わらず_確認すると保存してから終わる",
            動作中があれば終了を止め確認を求め_取り消すと何も変わらず_確認すると保存してから終わる,
        ),
        (
            "9_枚の後片付けがおよそ_1_枚ぶんの時間で終わる",
            九枚の後片付けがおよそ_1_枚ぶんの時間で終わる,
        ),
    ];
    for (name, test) in tests {
        test();
        println!("test {name} ... ok");
    }
}
