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

use arin_core::terminal::{TerminalId, TerminalStatus};
use arin_core::workspace::Workspace;
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
        let workspace = Workspace::new("0.0.0-test", home.path().to_path_buf(), vars);
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
