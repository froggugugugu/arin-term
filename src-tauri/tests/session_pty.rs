//! セッション管理(`arin_lib::sessions`)を実 PTY で動かす結合テスト。設計書 §7.1、ADR-004、NFR-006。
//!
//! `crates/arin-platform/tests/pty.rs` にならう。
//! - テスト同士が並行しないよう、全テストが同じロックを取ってから始める
//! - どの待ちにも上限時間(`WAIT`)を付け、上限を過ぎたらそれまでの出力を添えて失敗させる
//! - シェルは `/bin/zsh` に固定し、`HOME` を一時フォルダに向けて利用者の設定ファイルを読ませない
//! - シェルの出力は、目印の文字列で囲ませて取り出す(入力のエコーには目印がそのままの形では現れない)

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use arin_core::persistence::codec::PersistedState;
use arin_core::project::{Project, ProjectColor, ProjectId, RootReference};
use arin_core::terminal::{TerminalId, TerminalStatus};
use arin_core::workspace::{RootResolution, Startup, Workspace};
use arin_lib::sessions::{OutputSink, SessionConfig, SessionRegistry, ShellDefaults};
use arin_platform::pty::NativePty;

const WAIT: Duration = Duration::from_secs(20);
const ZSH: &str = "/bin/zsh";

static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(PoisonError::into_inner)
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// このテストだけが使う一時フォルダ(`HOME` と cwd)。抜けるときに自分が作ったフォルダだけを消す。
struct TempHome(PathBuf);

impl TempHome {
    fn new(name: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("時計が 1970 年より前")
            .as_nanos();
        let base = std::env::temp_dir();
        let dir = base.join(format!(
            "arin-session-pty-{name}-{}-{nanos}",
            std::process::id()
        ));
        assert!(dir.starts_with(&base), "一時領域の外を使おうとした");
        fs::create_dir(&dir).expect("一時フォルダを作れなかった");
        // 設定ファイルが 1 つも無いと、zsh が初回の設定の案内を出すことがある。
        fs::write(dir.join(".zshrc"), "").expect(".zshrc を作れなかった");
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempHome {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Default)]
struct Collect(Mutex<Vec<u8>>);

impl OutputSink for Collect {
    fn send(&self, data: Vec<u8>) -> Result<(), String> {
        lock(&self.0).extend_from_slice(&data);
        Ok(())
    }
}

struct Shell {
    registry: SessionRegistry,
    sink: Arc<Collect>,
    id: TerminalId,
    _home: TempHome,
}

impl Shell {
    fn start(name: &str) -> Self {
        let home = TempHome::new(name);
        let mut vars: Vec<(OsString, OsString)> = ["USER", "LOGNAME", "TMPDIR"]
            .into_iter()
            .filter_map(|n| std::env::var_os(n).map(|v| (n.into(), v)))
            .collect();
        vars.push(("HOME".into(), home.path().as_os_str().to_owned()));
        // シェルが終わってもタブを残す 9 枚の検証画面の形で作り、1 枚目だけを使う(タブを閉じる形のテストは下の T33b の節)。
        let workspace = Workspace::for_verify_grid("0.0.0-test", home.path().to_path_buf(), vars);
        let registry = SessionRegistry::new(
            workspace,
            Arc::new(NativePty),
            Arc::new(|| ShellDefaults {
                shell: PathBuf::from(ZSH),
                locale: Some("ja_JP".to_owned()),
            }),
            Arc::new(|_| {}),
            SessionConfig {
                exit_poll: Duration::from_millis(50),
                ..SessionConfig::default()
            },
        );
        let id = registry.snapshot().terminals[0].id.clone();
        let sink = Arc::new(Collect::default());
        registry
            .connect(&id, 24, 80, Arc::clone(&sink) as Arc<dyn OutputSink>)
            .expect("シェルを起動できなかった");
        Self {
            registry,
            sink,
            id,
            _home: home,
        }
    }

    fn output(&self) -> String {
        String::from_utf8_lossy(&lock(&self.sink.0)).into_owned()
    }

    /// 出力に `start`〜`end` で囲まれた値が現れるまで待って返す。受け取った分は処理済みとして返す。
    fn wait_marked(&self, start: &str, end: &str) -> String {
        let deadline = Instant::now() + WAIT;
        loop {
            let text = self.output();
            let _ = self.registry.ack(&self.id, u64::MAX);
            if let Some(i) = text.find(start)
                && let Some(j) = text[i + start.len()..].find(end)
            {
                return text[i + start.len()..i + start.len() + j].to_owned();
            }
            assert!(
                Instant::now() < deadline,
                "目印が現れなかった。出力: {text:?}"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn status(&self) -> TerminalStatus {
        self.registry.snapshot().terminals[0].status.clone()
    }
}

/// プロセスが動いているか(`kill -0`)。
fn is_alive(pid: &str) -> bool {
    Command::new("/bin/kill")
        .args(["-0", pid])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[test]
fn 入力がシェルに届き_出力が送り先に届き_exit_で_exited_になる() {
    let _serial = serial();
    let shell = Shell::start("echo");

    // 目印は引用符で分けて送るので、入力のエコーには目印がそのままの形では現れない。
    shell
        .registry
        .write(&shell.id, b"echo AR''IN-$((6*7))-MA''RK\r")
        .unwrap();
    assert_eq!(shell.wait_marked("ARIN-", "-MARK"), "42");

    shell.registry.write(&shell.id, b"exit 7\r").unwrap();
    let deadline = Instant::now() + WAIT;
    while shell.status() == TerminalStatus::Running {
        let _ = shell.registry.ack(&shell.id, u64::MAX);
        assert!(Instant::now() < deadline, "終了を反映しなかった");
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        shell.status(),
        TerminalStatus::Exited {
            code: Some(7),
            signal: None
        }
    );
    assert!(shell.registry.write(&shell.id, b"ls\r").is_err());
}

#[test]
fn 終了の処理で動いているシェルを残さない() {
    let _serial = serial();
    let shell = Shell::start("shutdown");
    shell
        .registry
        .write(&shell.id, b"echo PI''D-$$-E''ND\r")
        .unwrap();
    let pid = shell.wait_marked("PID-", "-END");
    assert!(
        pid.chars().all(|c| c.is_ascii_digit()),
        "PID が数字でない: {pid}"
    );
    assert!(is_alive(&pid), "終了の処理の前にシェルが終わっていた");

    shell.registry.shutdown();

    let deadline = Instant::now() + WAIT;
    while is_alive(&pid) {
        assert!(Instant::now() < deadline, "シェル {pid} が残っている");
        thread::sleep(Duration::from_millis(10));
    }
}

// ---------------------------------------------------------------------------
// 2 つのプロジェクト(T33b。NFR-006)
// ---------------------------------------------------------------------------

/// シェルの出力に `start`〜`end` で囲まれた値が現れるまで待って返す。受け取った分は処理済みとして返す。
fn wait_marked_in(
    registry: &SessionRegistry,
    id: &TerminalId,
    sink: &Collect,
    start: &str,
    end: &str,
) -> String {
    let deadline = Instant::now() + WAIT;
    loop {
        let text = String::from_utf8_lossy(&lock(&sink.0)).into_owned();
        let _ = registry.ack(id, u64::MAX);
        if let Some(i) = text.find(start)
            && let Some(j) = text[i + start.len()..].find(end)
        {
            return text[i + start.len()..i + start.len() + j].to_owned();
        }
        assert!(
            Instant::now() < deadline,
            "目印が現れなかった。出力: {text:?}"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn _2_つのプロジェクトで_1_枚ずつ動かし片方を閉じてももう片方が入出力を続ける() {
    let _serial = serial();
    let home = TempHome::new("two-projects");
    let roots: Vec<PathBuf> = ["alpha", "beta"]
        .iter()
        .map(|name| {
            let dir = home.path().join(name);
            fs::create_dir(&dir).expect("プロジェクトのフォルダを作れなかった");
            dir
        })
        .collect();
    let projects: Vec<(ProjectId, RootReference)> = roots
        .iter()
        .zip(1u8..)
        .map(|(root, n)| {
            (
                ProjectId::from_random_bytes([n; 16]),
                RootReference::new(root.clone(), None),
            )
        })
        .collect();
    let mut vars: Vec<(OsString, OsString)> = ["USER", "LOGNAME", "TMPDIR"]
        .into_iter()
        .filter_map(|n| std::env::var_os(n).map(|v| (n.into(), v)))
        .collect();
    vars.push(("HOME".into(), home.path().as_os_str().to_owned()));
    let workspace = Workspace::restore(
        "0.0.0-test",
        home.path().to_path_buf(),
        vars,
        Startup {
            state: PersistedState {
                projects: projects
                    .iter()
                    .map(|(id, root)| {
                        Project::restore(id.clone(), root.clone(), None, ProjectColor::Blue)
                    })
                    .collect(),
                selected_project_id: Some(projects[0].0.clone()),
                ..PersistedState::default()
            },
            roots: projects
                .iter()
                .map(|(id, root)| (id.clone(), RootResolution::Found(root.clone())))
                .collect(),
            notice: None,
        },
    );
    let registry = SessionRegistry::new(
        workspace,
        Arc::new(NativePty),
        Arc::new(|| ShellDefaults {
            shell: PathBuf::from(ZSH),
            locale: Some("ja_JP".to_owned()),
        }),
        Arc::new(|_| {}),
        SessionConfig {
            exit_poll: Duration::from_millis(50),
            ..SessionConfig::default()
        },
    );
    let snapshot = registry.snapshot();
    let a = snapshot.projects[0].terminal_ids[0].clone();
    let b = snapshot.projects[1].terminal_ids[0].clone();
    let (sink_a, sink_b) = (Arc::new(Collect::default()), Arc::new(Collect::default()));
    for (id, sink) in [(&a, &sink_a), (&b, &sink_b)] {
        registry
            .connect(id, 24, 80, Arc::clone(sink) as Arc<dyn OutputSink>)
            .expect("シェルを起動できなかった");
    }
    registry.write(&a, b"echo PI''D-$$-E''ND\r").unwrap();
    let pid_a = wait_marked_in(&registry, &a, &sink_a, "PID-", "-END");
    assert!(
        pid_a.chars().all(|c| c.is_ascii_digit()),
        "PID が数字でない: {pid_a}"
    );
    // cwd はそれぞれのプロジェクトのルート。
    registry.write(&b, b"echo CW''D-$PWD-E''ND\r").unwrap();
    let cwd_b = wait_marked_in(&registry, &b, &sink_b, "CWD-", "-END");
    assert_eq!(
        fs::canonicalize(&cwd_b).unwrap(),
        fs::canonicalize(&roots[1]).unwrap()
    );

    registry.close_terminal(&a).expect("閉じられなかった");

    let deadline = Instant::now() + WAIT;
    while is_alive(&pid_a) {
        assert!(
            Instant::now() < deadline,
            "閉じたタブのシェル {pid_a} が残っている"
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert!(registry.snapshot().terminals.iter().all(|t| t.id != a));
    assert_eq!(registry.backlog(&a), None);
    // 閉じたタブの資源(送り先を持つセッションとスレッド)が返る。
    let deadline = Instant::now() + WAIT;
    while Arc::strong_count(&sink_a) > 1 {
        assert!(Instant::now() < deadline, "閉じたタブのスレッドが抜けない");
        thread::sleep(Duration::from_millis(10));
    }
    // もう片方は入出力を続ける。
    registry
        .write(&b, b"echo AR''IN-$((6*7))-MA''RK\r")
        .unwrap();
    assert_eq!(
        wait_marked_in(&registry, &b, &sink_b, "ARIN-", "-MARK"),
        "42"
    );
    assert_eq!(
        registry.snapshot().terminals[0].status,
        TerminalStatus::Running
    );

    registry.shutdown();
}
