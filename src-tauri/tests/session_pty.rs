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
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, mpsc};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use arin_core::persistence::codec::PersistedState;
use arin_core::project::{Project, ProjectColor, ProjectId, RootReference};
use arin_core::terminal::{TerminalId, TerminalStatus};
use arin_core::workspace::{AppSnapshot, RootResolution, Startup, Workspace};
use arin_lib::sessions::{
    OutputSink, SessionConfig, SessionRegistry, ShellDefaults, SnapshotListener,
};
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

/// プロジェクトをいくつか持つセッション管理。プロジェクトごとに 1 枚のタブを開き、すべてのシェルを起動する。
struct Projects {
    registry: SessionRegistry,
    roots: Vec<PathBuf>,
    project_ids: Vec<ProjectId>,
    terminals: Vec<TerminalId>,
    sinks: Vec<Arc<Collect>>,
    _home: TempHome,
}

impl Projects {
    /// `names` のフォルダを一時フォルダの中に作り、その順にプロジェクトにする。先頭を選択中にする。
    fn start(name: &str, names: &[&str]) -> Self {
        Self::start_with_listener(name, names, Arc::new(|_| {}))
    }

    /// `start` に、写しの受け取りの関数(`listener`)を足したもの。
    fn start_with_listener(name: &str, names: &[&str], listener: SnapshotListener) -> Self {
        let home = TempHome::new(name);
        let roots: Vec<PathBuf> = names
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
            listener,
            SessionConfig {
                exit_poll: Duration::from_millis(50),
                ..SessionConfig::default()
            },
        );
        let snapshot = registry.snapshot();
        let project_ids: Vec<ProjectId> = snapshot.projects.iter().map(|p| p.id.clone()).collect();
        let terminals: Vec<TerminalId> = snapshot
            .projects
            .iter()
            .map(|p| p.terminal_ids[0].clone())
            .collect();
        let sinks: Vec<Arc<Collect>> = terminals
            .iter()
            .map(|_| Arc::new(Collect::default()))
            .collect();
        for (id, sink) in terminals.iter().zip(&sinks) {
            registry
                .connect(id, 24, 80, Arc::clone(sink) as Arc<dyn OutputSink>)
                .expect("シェルを起動できなかった");
        }
        Self {
            registry,
            roots,
            project_ids,
            terminals,
            sinks,
            _home: home,
        }
    }

    fn write(&self, i: usize, data: &[u8]) {
        self.registry
            .write(&self.terminals[i], data)
            .expect("入力を書き込めなかった");
    }

    fn wait_marked(&self, i: usize, start: &str, end: &str) -> String {
        wait_marked_in(
            &self.registry,
            &self.terminals[i],
            &self.sinks[i],
            start,
            end,
        )
    }

    fn output(&self, i: usize) -> String {
        String::from_utf8_lossy(&lock(&self.sinks[i].0)).into_owned()
    }

    /// `i` 番目のシェルの pid。
    fn shell_pid(&self, i: usize) -> String {
        self.write(i, b"echo PI''D-$$-E''ND\r");
        let pid = self.wait_marked(i, "PID-", "-END");
        assert!(
            pid.chars().all(|c| c.is_ascii_digit()),
            "PID が数字でない: {pid}"
        );
        pid
    }

    fn status(&self, i: usize) -> Option<TerminalStatus> {
        self.registry
            .snapshot()
            .terminals
            .into_iter()
            .find(|t| t.id == self.terminals[i])
            .map(|t| t.status)
    }

    /// `i` 番目のシェルが入出力を続けていることを確かめる(`tag` は数字と英字だけ)。
    fn assert_responsive(&self, i: usize, tag: &str) {
        self.write(i, format!("echo AR''IN-{tag}-$((6*7))-MA''RK\r").as_bytes());
        assert_eq!(
            self.wait_marked(i, &format!("ARIN-{tag}-"), "-MARK"),
            "42",
            "{i} 番目のシェルが応えない"
        );
        assert_eq!(self.status(i), Some(TerminalStatus::Running));
    }
}

/// `text` の中の `start`〜`end` で囲まれた値を、現れた順にすべて返す。
fn all_marked(text: &str, start: &str, end: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find(start) {
        rest = &rest[i + start.len()..];
        let Some(j) = rest.find(end) else { break };
        found.push(rest[..j].to_owned());
        rest = &rest[j + end.len()..];
    }
    found
}

/// プロセスが動いているか(`ps`。回収待ちのゾンビは終わったものとみなす)。
fn is_running_process(pid: &str) -> bool {
    let out = Command::new("/bin/ps")
        .args(["-o", "stat=", "-p", pid])
        .output()
        .expect("ps を起動できなかった");
    let stat = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    out.status.success() && !stat.is_empty() && !stat.starts_with('Z')
}

/// プロセスの状態(`ps -o stat=`。例 `S+`、終わりかけは `E` を含む)。無ければ「無い」。失敗の報告に使う。
fn process_state(pid: &str) -> String {
    let out = Command::new("/bin/ps")
        .args(["-o", "stat=", "-p", pid])
        .output()
        .expect("ps を起動できなかった");
    let stat = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    if stat.is_empty() {
        "無い".to_owned()
    } else {
        stat
    }
}

/// プロセスの実行ファイルの名前(`ps -o comm=`)。無ければ空。
fn command_name(pid: &str) -> String {
    let out = Command::new("/bin/ps")
        .args(["-o", "comm=", "-p", pid])
        .output()
        .expect("ps を起動できなかった");
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// 落ちても `sleep` を残さないよう、抜けるときにまだ動いている `sleep` だけを終わらせる。
struct KillSleepOnDrop(String);

impl Drop for KillSleepOnDrop {
    fn drop(&mut self) {
        if is_running_process(&self.0) && command_name(&self.0).ends_with("sleep") {
            let _ = Command::new("/bin/kill")
                .args(["-KILL", &self.0])
                .stderr(std::process::Stdio::null())
                .status();
        }
    }
}

#[test]
fn _2_つのプロジェクトで_1_枚ずつ動かし片方を閉じてももう片方が入出力を続ける() {
    let _serial = serial();
    let p = Projects::start("two-projects", &["alpha", "beta"]);
    let pid_a = p.shell_pid(0);
    // cwd はそれぞれのプロジェクトのルート。
    p.write(1, b"echo CW''D-$PWD-E''ND\r");
    let cwd_b = p.wait_marked(1, "CWD-", "-END");
    assert_eq!(
        fs::canonicalize(&cwd_b).unwrap(),
        fs::canonicalize(&p.roots[1]).unwrap()
    );

    let a = p.terminals[0].clone();
    p.registry.close_terminal(&a).expect("閉じられなかった");

    let deadline = Instant::now() + WAIT;
    while is_alive(&pid_a) {
        assert!(
            Instant::now() < deadline,
            "閉じたタブのシェル {pid_a} が残っている"
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert!(p.registry.snapshot().terminals.iter().all(|t| t.id != a));
    assert_eq!(p.registry.backlog(&a), None);
    // 閉じたタブの資源(送り先を持つセッションとスレッド)が返る。
    let deadline = Instant::now() + WAIT;
    while Arc::strong_count(&p.sinks[0]) > 1 {
        assert!(Instant::now() < deadline, "閉じたタブのスレッドが抜けない");
        thread::sleep(Duration::from_millis(10));
    }
    // もう片方は入出力を続ける。
    p.assert_responsive(1, "B");

    p.registry.shutdown();
}

// ---------------------------------------------------------------------------
// 実物の結合テスト(T35b。FR-042 の AC-M04 の Rust 側、NFR-006、M1 の見送り)
//
// 計画書の `A_のタブを閉じても_B_が入出力を続ける` は、上の
// `_2_つのプロジェクトで_1_枚ずつ動かし片方を閉じてももう片方が入出力を続ける`(T33b)が同じ内容を確かめる。
// ---------------------------------------------------------------------------

#[test]
#[allow(non_snake_case)] // 計画書のテスト名(A・B はプロジェクトの名前)
fn A_で_for_i_in_seq_1_30_を流しながら_B_を操作しても_A_の出力が続く() {
    let _serial = serial();
    let p = Projects::start("seq-while-b", &["alpha", "beta"]);
    // 待ちは短くしてよい(計画書 T35a・T35b の【仮定】)。1 行 0.1 秒で 30 行、およそ 3 秒流す。
    p.write(
        0,
        b"for i in $(seq 1 30); do echo SE''Q-$i-E''ND; sleep 0.1; done; echo DO''NE-A-E''ND\r",
    );
    // A が流し始めてから B を操作する。
    p.wait_marked(0, "SEQ-", "-END");

    // B のプロジェクトに切り替え、B のタブを表示中にして、入出力と寸法の変更を何度か行う。
    p.registry
        .select_project(&p.project_ids[1])
        .expect("B を選べなかった");
    p.registry
        .select_terminal(&p.terminals[1])
        .expect("B のタブを表示中にできなかった");
    for round in 0..5u16 {
        p.registry
            .resize(&p.terminals[1], 24 + round, 80 + round)
            .expect("B の寸法を変えられなかった");
        p.assert_responsive(1, &format!("B{round}"));
    }
    assert!(
        !p.output(0).contains("DONE-A-END"),
        "B を操作し終える前に A が流し終わった(重ねて動かせていない)。A の出力: {:?}",
        p.output(0)
    );
    // A に戻る。
    p.registry
        .select_project(&p.project_ids[0])
        .expect("A を選べなかった");

    // A の 1〜30 が欠けず、順に届いている。
    p.wait_marked(0, "DONE-", "-END");
    let seen: Vec<String> = all_marked(&p.output(0), "SEQ-", "-END");
    let expected: Vec<String> = (1..=30).map(|n| n.to_string()).collect();
    assert_eq!(seen, expected, "A の出力が欠けたか順が乱れた");
    assert_eq!(p.status(0), Some(TerminalStatus::Running));

    p.registry.shutdown();
}

#[test]
#[allow(non_snake_case)] // 計画書のテスト名(A・B はプロジェクトの名前)
fn A_のシェルを外から_kill_しても_B_が続く() {
    let _serial = serial();
    let p = Projects::start("kill-a", &["alpha", "beta"]);
    let pid_a = p.shell_pid(0);
    p.assert_responsive(1, "before");

    let killed = Command::new("/bin/kill")
        .args(["-KILL", &pid_a])
        .status()
        .expect("kill を起動できなかった");
    assert!(
        killed.success(),
        "A のシェル {pid_a} に SIGKILL を送れなかった"
    );

    // A は終わったと反映され(プロジェクトのタブは既定の `ExitPolicy::CloseTab` で閉じる)、B は入出力を続ける。
    let deadline = Instant::now() + WAIT;
    while p.status(0).is_some() {
        let _ = p.registry.ack(&p.terminals[0], u64::MAX);
        assert!(
            Instant::now() < deadline,
            "A の終了を反映しなかった(状態: {:?})",
            p.status(0)
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        !is_running_process(&pid_a),
        "A のシェル {pid_a} が残っている"
    );
    assert_eq!(p.registry.backlog(&p.terminals[0]), None);
    assert_eq!(p.registry.snapshot().terminals.len(), 1);
    for round in 0..3 {
        p.assert_responsive(1, &format!("after{round}"));
    }

    p.registry.shutdown();
}

#[test]
fn 前面で_sleep_を動かしたタブを閉じると_sleep_も終わる() {
    let _serial = serial();
    let p = Projects::start("close-sleep", &["alpha", "beta"]);
    let pid_shell = p.shell_pid(0);
    // `sh` が自分の pid を目印で囲んで出し、その pid のまま `sleep` に置き換わる(前面のジョブ)。
    // 引用符で分けるので、入力のエコーには目印がそのままの形では現れない。
    p.write(
        0,
        b"/bin/sh -c 'echo SL''P-$$-E''ND; exec /bin/sleep 300'\r",
    );
    let pid_sleep = p.wait_marked(0, "SLP-", "-END");
    assert!(
        pid_sleep.chars().all(|c| c.is_ascii_digit()),
        "PID が数字でない: {pid_sleep}"
    );
    let _guard = KillSleepOnDrop(pid_sleep.clone());
    // exec が済んで sleep になるまで待つ。
    let deadline = Instant::now() + WAIT;
    while !command_name(&pid_sleep).ends_with("sleep") {
        assert!(
            Instant::now() < deadline,
            "{pid_sleep} が sleep にならなかった({:?})",
            command_name(&pid_sleep)
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert!(is_running_process(&pid_sleep));

    // 閉じる処理そのものが戻らないこともあるので、別のスレッドで呼んで上限時間まで待つ
    // (戻らなければ、そのスレッドはテストのプロセスの終了で消える)。
    let (done, finished) = mpsc::channel();
    let registry = p.registry.clone();
    let a = p.terminals[0].clone();
    thread::spawn(move || {
        let _ = done.send(registry.close_terminal(&a));
    });
    match finished.recv_timeout(WAIT) {
        Ok(result) => result.expect("タブを閉じられなかった"),
        Err(_) => panic!(
            "タブを閉じる処理が {WAIT:?} 以内に戻らなかった(シェル {pid_shell}: {}、sleep {pid_sleep}: {})",
            process_state(&pid_shell),
            process_state(&pid_sleep)
        ),
    }

    let deadline = Instant::now() + WAIT;
    while is_running_process(&pid_shell) || is_running_process(&pid_sleep) {
        assert!(
            Instant::now() < deadline,
            "閉じたタブのプロセスが残っている(シェル {pid_shell}: {}、sleep {pid_sleep}: {})",
            process_state(&pid_shell),
            process_state(&pid_sleep)
        );
        thread::sleep(Duration::from_millis(10));
    }
    p.assert_responsive(1, "after");

    p.registry.shutdown();
}

// ---------------------------------------------------------------------------
// アテンション(T48c。FR-055)
// ---------------------------------------------------------------------------

/// 写しのうち、`id` のタブにアテンションの印が立っているものの数。
fn sent_with_attention(snapshots: &Mutex<Vec<AppSnapshot>>, id: &TerminalId) -> usize {
    lock(snapshots)
        .iter()
        .filter(|s| s.terminals.iter().any(|t| &t.id == id && t.attention))
        .count()
}

fn has_attention(registry: &SessionRegistry, id: &TerminalId) -> bool {
    registry
        .snapshot()
        .terminals
        .iter()
        .any(|t| &t.id == id && t.attention)
}

#[test]
fn 隠れたタブで_printf_bel_を実行すると印が立つ() {
    // 計画書の名前は `隠れたタブで_printf_'\a'_を実行すると印が立つ`(Rust の名前に引用符と `\` を使えないので言い換えた)。
    let _serial = serial();
    let snapshots: Arc<Mutex<Vec<AppSnapshot>>> = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&snapshots);
    let p = Projects::start_with_listener(
        "attention",
        &["alpha", "beta"],
        Arc::new(move |snapshot| lock(&seen).push(snapshot)),
    );
    // A(選択中)が表示中で、B は隠れている。起動しただけでは印は立たない。
    p.assert_responsive(1, "ready");
    assert!(!has_attention(&p.registry, &p.terminals[1]));

    // 表示中の A のベルでは立たない(目印が届いた時点で、ベルのチャンクの処理は済んでいる)。
    p.write(0, b"printf '\\a'; echo BE''LL-A-E''ND\r");
    p.wait_marked(0, "BELL-", "-END");
    assert_eq!(sent_with_attention(&snapshots, &p.terminals[0]), 0);

    // 隠れた B のベルで印が立ち、印の載った写しが 1 回送られる。
    p.write(1, b"printf '\\a'; echo BE''LL-B-E''ND\r");
    p.wait_marked(1, "BELL-", "-END");
    let deadline = Instant::now() + WAIT;
    while sent_with_attention(&snapshots, &p.terminals[1]) == 0 {
        assert!(
            Instant::now() < deadline,
            "B のベルで印の載った写しが送られなかった。出力: {:?}",
            p.output(1)
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert!(has_attention(&p.registry, &p.terminals[1]));
    assert!(!has_attention(&p.registry, &p.terminals[0]));

    // B を表示すると消える。
    p.registry
        .select_project(&p.project_ids[1])
        .expect("B を選べなかった");
    assert!(!has_attention(&p.registry, &p.terminals[1]));

    p.registry.shutdown();
}
