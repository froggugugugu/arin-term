//! 実 PTY の結合テスト(`arin_platform::pty`)。設計書 §7.1「ターミナルの起動」、FR-039・FR-040・FR-043、NFR-006。
//!
//! - テスト同士が並行しないよう、全テストが同じロックを取ってから始める
//! - どの待ちにも上限時間(`WAIT`)を付ける。上限を過ぎたら、それまでの出力を添えて失敗させる
//! - `HOME` を一時フォルダに向け、利用者の `~/.zshrc` などを読ませない。読まれるのは `/etc` の設定だけ
//! - シェルの出力は、目印の文字列で囲ませて取り出す。入力のエコーやプロンプトには目印がそのままの形では現れない

use std::ffi::OsString;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use arin_core::ports::{Foreground, PtyError, PtyPort, PtySession, PtySize};
use arin_core::terminal::ShellLaunchSpec;
use arin_platform::pty::{FALLBACK_SHELL, NativePty, default_shell, system_locale};

/// 1 回の待ちの上限。CI の遅いランナーでも足りる長さにする。
const WAIT: Duration = Duration::from_secs(20);
/// テストで起動するシェル。利用者の既定のシェルに左右されないよう固定する。
const ZSH: &str = "/bin/zsh";
const SIZE: PtySize = PtySize { rows: 24, cols: 80 };

static SERIAL: Mutex<()> = Mutex::new(());

/// テストを直列にする。前のテストが失敗してロックが壊れていても続ける。
fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(PoisonError::into_inner)
}

/// このテストだけが使う一時フォルダ。抜けるときに自分が作ったフォルダだけを消す。
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("時計が 1970 年より前")
            .as_nanos();
        let base = std::env::temp_dir();
        let dir = base.join(format!(
            "arin-platform-pty-{name}-{}-{nanos}",
            std::process::id()
        ));
        assert!(dir.starts_with(&base), "一時領域の外を使おうとした");
        fs::create_dir(&dir).expect("一時フォルダを作れなかった");
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// シェルの起動に使うフォルダ一式。`HOME` と、それとは別の作業フォルダを持つ。
struct Sandbox {
    root: TempDir,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        let root = TempDir::new(name);
        let home = root.path().join("home");
        fs::create_dir(&home).expect("HOME を作れなかった");
        // 設定ファイルが 1 つも無いと、zsh が初回の設定の案内(zsh-newuser-install)を出すことがある。
        fs::write(home.join(".zshrc"), "").expect(".zshrc を作れなかった");
        fs::create_dir(root.path().join("project")).expect("作業フォルダを作れなかった");
        Self { root }
    }

    fn home(&self) -> PathBuf {
        self.root.path().join("home")
    }

    fn project(&self) -> PathBuf {
        self.root.path().join("project")
    }

    /// 親の環境変数(許可リストの名前だけ)と、一時の `HOME` から仕様を組み立てる。
    fn spec_in(&self, cwd: &Path) -> ShellLaunchSpec {
        let mut parent_env: Vec<(OsString, OsString)> = ["USER", "LOGNAME", "TMPDIR"]
            .into_iter()
            .filter_map(|name| std::env::var_os(name).map(|value| (name.into(), value)))
            .collect();
        parent_env.push(("HOME".into(), self.home().into_os_string()));
        ShellLaunchSpec::new(Path::new(ZSH), cwd, parent_env, Some("ja_JP"))
            .expect("起動の仕様を組み立てられなかった")
    }

    fn spec(&self) -> ShellLaunchSpec {
        self.spec_in(&self.project())
    }
}

/// 起動したシェル 1 つ。出力は別スレッドで読み、チャネルで受け取る。
struct Shell {
    session: Box<dyn PtySession>,
    output: Receiver<Vec<u8>>,
    received: Vec<u8>,
    queries: u32,
}

impl Shell {
    fn spawn(spec: &ShellLaunchSpec, size: PtySize) -> Self {
        let spawned = NativePty
            .spawn(spec, size)
            .expect("シェルを起動できなかった");
        let (sender, output) = mpsc::channel();
        let mut reader = spawned.output;
        // 出力の終わりかエラーで抜ける。抜けるとチャネルが閉じ、受け取り側が終わりを知る。
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if sender.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        Self {
            session: spawned.session,
            output,
            received: Vec::new(),
            queries: 0,
        }
    }

    fn send(&mut self, input: &str) {
        self.session
            .write(input.as_bytes())
            .expect("入力を書き込めなかった");
    }

    /// シェルに `expr` を評価させ、その値を返す。値は改行と `>` を含まないものに限る。
    ///
    /// 出力は `ARIN<番号><値>NIRA` の形。入力のエコーには `ARIN%s<%s>NIRA` と出るので取り違えない。
    fn query(&mut self, expr: &str) -> String {
        self.queries += 1;
        let tag = self.queries;
        self.send(&format!("printf 'ARIN%s<%s>NIRA\\n' {tag} {expr}\n"));
        let start = format!("ARIN{tag}<");
        let deadline = Instant::now() + WAIT;
        loop {
            let text = String::from_utf8_lossy(&self.received).into_owned();
            if let Some(begin) = text.find(&start) {
                let rest = &text[begin + start.len()..];
                if let Some(end) = rest.find(">NIRA") {
                    return rest[..end].to_owned();
                }
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.output.recv_timeout(remaining) {
                Ok(chunk) => self.received.extend_from_slice(&chunk),
                Err(RecvTimeoutError::Timeout) => {
                    panic!("{expr} の値が {WAIT:?} 以内に返らなかった。出力: {text:?}")
                }
                Err(RecvTimeoutError::Disconnected) => {
                    panic!("{expr} の値が返る前に出力が終わった。出力: {text:?}")
                }
            }
        }
    }

    /// シェルが終わるのを待ち、終了コードを返す。
    fn wait_exit(&mut self) -> u32 {
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(exit) = self.session.try_wait().expect("終了を確かめられなかった")
            {
                return exit.exit_code;
            }
            assert!(
                Instant::now() < deadline,
                "シェルが {WAIT:?} 以内に終わらなかった"
            );
            thread::sleep(Duration::from_millis(20));
        }
    }

    /// 前面のプロセスが `expected` になるのを待つ。上限を過ぎたら最後に見た値を添えて失敗させる。
    fn wait_foreground(&mut self, expected: Foreground) {
        let deadline = Instant::now() + WAIT;
        loop {
            let actual = self.session.foreground();
            if actual == expected {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "前面が {WAIT:?} 以内に {expected:?} にならなかった(最後は {actual:?})"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    /// 出力が終わる(PTY の相手側がすべて閉じる)のを待つ。
    fn wait_output_end(&mut self) {
        let deadline = Instant::now() + WAIT;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.output.recv_timeout(remaining) {
                Ok(chunk) => self.received.extend_from_slice(&chunk),
                Err(RecvTimeoutError::Disconnected) => return,
                Err(RecvTimeoutError::Timeout) => {
                    panic!("出力が {WAIT:?} 以内に終わらなかった")
                }
            }
        }
    }
}

/// プロセスが動いているか(`ps` で探す)。
fn process_exists(pid: &str) -> bool {
    Command::new("/bin/ps")
        .args(["-o", "pid=", "-p", pid])
        .output()
        .expect("ps を起動できなかった")
        .status
        .success()
}

#[test]
fn ログインかつ対話のシェルとして_argv0_が_zsh_の形で起動する() {
    let _serial = serial();
    let sandbox = Sandbox::new("login");
    let mut shell = Shell::spawn(&sandbox.spec(), SIZE);

    assert_eq!(shell.query("\"$0\""), "-zsh");
    assert_eq!(
        shell.query("\"$(/bin/ps -o command= -p $$)\"").trim(),
        "-zsh"
    );
    assert_eq!(
        shell.query("\"${options[login]}:${options[interactive]}\""),
        "on:on"
    );
}

#[test]
fn 指定の作業フォルダで始まる() {
    let _serial = serial();
    let sandbox = Sandbox::new("cwd");
    let mut shell = Shell::spawn(&sandbox.spec(), SIZE);

    // 一時領域の `/var` は `/private/var` へのリンクなので、実体のパスどうしで比べる。
    let expected = fs::canonicalize(sandbox.project()).expect("作業フォルダの実体を引けなかった");
    assert_eq!(
        PathBuf::from(shell.query("\"$(pwd -P)\"")),
        expected,
        "HOME ではなく指定のフォルダで始まる"
    );
}

#[test]
fn path_は同じ最小の環境から_zsh_l_で得た値と同じになる() {
    let _serial = serial();
    let sandbox = Sandbox::new("path");
    let spec = sandbox.spec();
    let mut shell = Shell::spawn(&spec, SIZE);
    let in_terminal = shell.query("\"$PATH\"");

    let login = Command::new(ZSH)
        .args(["-l", "-c", "printf %s \"$PATH\""])
        .env_clear()
        .envs(spec.env())
        .current_dir(spec.cwd())
        .output()
        .expect("zsh -l を起動できなかった");
    assert!(login.status.success(), "zsh -l が失敗した: {login:?}");
    let expected = String::from_utf8(login.stdout).expect("PATH が UTF-8 でない");

    assert_eq!(in_terminal, expected);
    assert!(
        in_terminal.split(':').count() > 4,
        "ログインシェルが最小の PATH から組み立てていない: {in_terminal}"
    );
}

#[test]
fn 行と桁数の変更がシェルに届く() {
    let _serial = serial();
    let sandbox = Sandbox::new("resize");
    let mut shell = Shell::spawn(&sandbox.spec(), SIZE);

    assert_eq!(shell.query("\"$(stty size)\""), "24 80");
    shell
        .session
        .resize(PtySize {
            rows: 40,
            cols: 132,
        })
        .expect("寸法を変えられなかった");
    assert_eq!(shell.query("\"$(stty size)\""), "40 132");
}

#[test]
fn ターミナル_2_枚のうち_1_枚が終わってももう_1_枚は入出力を続ける() {
    let _serial = serial();
    let sandbox = Sandbox::new("two");
    let spec = sandbox.spec();
    let mut first = Shell::spawn(&spec, SIZE);
    let mut second = Shell::spawn(&spec, SIZE);
    assert_eq!(first.query("one"), "one");
    assert_eq!(second.query("two"), "two");

    first.send("exit\n");
    assert_eq!(first.wait_exit(), 0);
    first.wait_output_end();

    assert_eq!(second.query("still"), "still");
    assert_eq!(second.query("\"$((6 * 7))\""), "42");
    assert!(
        second
            .session
            .try_wait()
            .expect("終了を確かめられなかった")
            .is_none()
    );
}

#[test]
fn 仕様の環境変数だけを渡し親の余計な変数を渡さない() {
    let _serial = serial();
    let sandbox = Sandbox::new("env");
    let mut shell = Shell::spawn(&sandbox.spec(), SIZE);

    // cargo がテストの実行時に渡す変数。テストのプロセスにはあり、シェルには渡らないはず。
    assert!(std::env::var_os("CARGO_MANIFEST_DIR").is_some());
    assert_eq!(shell.query("\"${CARGO_MANIFEST_DIR-unset}\""), "unset");
    assert_eq!(shell.query("\"${CARGO-unset}\""), "unset");

    assert_eq!(shell.query("\"$TERM\""), "xterm-256color");
    assert_eq!(shell.query("\"$COLORTERM\""), "truecolor");
    assert_eq!(shell.query("\"$TERM_PROGRAM\""), "Arin");
    assert_eq!(shell.query("\"$SHELL\""), ZSH);
    assert_eq!(
        PathBuf::from(shell.query("\"$HOME\"")),
        sandbox.home(),
        "利用者の本物の HOME ではなく仕様の HOME"
    );
}

#[test]
fn 作業フォルダが無いかフォルダでなければホームにせず失敗する() {
    let _serial = serial();
    let sandbox = Sandbox::new("bad-cwd");

    let missing = sandbox.project().join("no-such-folder");
    let result = NativePty.spawn(&sandbox.spec_in(&missing), SIZE);
    assert!(
        matches!(result, Err(PtyError::Spawn(_))),
        "存在しない cwd でホームに切り替えずに失敗する"
    );

    let file = sandbox.project().join("file.txt");
    fs::write(&file, "").expect("ファイルを作れなかった");
    let result = NativePty.spawn(&sandbox.spec_in(&file), SIZE);
    assert!(
        matches!(result, Err(PtyError::Spawn(_))),
        "フォルダでない cwd は失敗する"
    );
}

#[test]
fn シェルが無ければ別のシェルにせず失敗する() {
    let _serial = serial();
    let sandbox = Sandbox::new("bad-shell");
    let spec = ShellLaunchSpec::new(
        &sandbox.project().join("no-such-shell"),
        &sandbox.project(),
        [("HOME", sandbox.home())],
        None,
    )
    .expect("起動の仕様を組み立てられなかった");

    let result = NativePty.spawn(&spec, SIZE);
    assert!(
        matches!(result, Err(PtyError::Spawn(_))),
        "別のシェルに切り替えずに失敗する"
    );
}

#[test]
fn kill_でシェルと出力が終わる() {
    let _serial = serial();
    let sandbox = Sandbox::new("kill");
    let mut shell = Shell::spawn(&sandbox.spec(), SIZE);
    let pid = shell.query("\"$$\"");

    shell.session.kill().expect("シェルを終了させられなかった");
    let exit = shell.wait_exit();
    assert_ne!(exit, 0, "シグナルで終わったので 0 以外");
    shell.wait_output_end();
    assert!(!process_exists(&pid), "シェルのプロセス {pid} が残っている");

    // 終わったあとの kill は何もしない(番号が使い回されたプロセスに送らない)。
    shell
        .session
        .kill()
        .expect("終わったあとの kill が失敗した");
}

#[test]
fn セッションを捨てるとシェルが終わり回収される() {
    let _serial = serial();
    let sandbox = Sandbox::new("drop");
    let mut shell = Shell::spawn(&sandbox.spec(), SIZE);
    let pid = shell.query("\"$$\"");
    assert!(process_exists(&pid));

    let Shell {
        session, output, ..
    } = shell;
    drop(session);

    assert!(
        !process_exists(&pid),
        "捨てたあともシェルのプロセス {pid} が残っている"
    );
    let deadline = Instant::now() + WAIT;
    loop {
        match output.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(_) => {}
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => panic!("出力が {WAIT:?} 以内に終わらなかった"),
        }
    }
}

/// シェル `shell_pid` の子の `sleep` を終わらせ、残っていないことを確かめる。
/// テストのあとに `sleep` を残さないため、失敗の前にも呼べるよう結果は見ない(`pkill` は該当なしで 1 を返す)。
fn end_child_sleep(shell_pid: &str) {
    let _ = Command::new("/usr/bin/pkill")
        .args(["-TERM", "-x", "-P", shell_pid, "sleep"])
        .status()
        .expect("pkill を起動できなかった");
}

/// シェル `shell_pid` の子に `sleep` があるか。
fn child_sleep_exists(shell_pid: &str) -> bool {
    Command::new("/usr/bin/pgrep")
        .args(["-x", "-P", shell_pid, "sleep"])
        .output()
        .expect("pgrep を起動できなかった")
        .status
        .success()
}

/// 落ちても子の `sleep` を残さないよう、抜けるときに終わらせる。
struct SleepGuard(String);

impl Drop for SleepGuard {
    fn drop(&mut self) {
        end_child_sleep(&self.0);
    }
}

#[test]
fn 起動直後の前面はシェル() {
    let _serial = serial();
    let sandbox = Sandbox::new("fg-shell");
    let mut shell = Shell::spawn(&sandbox.spec(), SIZE);

    // プロンプトを出すまで待ってから調べる(組み込みの printf は別のプロセスを作らない)。
    assert_eq!(shell.query("ready"), "ready");
    assert_eq!(shell.session.foreground(), Foreground::Shell);
}

#[test]
fn sleep_を前面で動かすとシェル以外になる() {
    let _serial = serial();
    let sandbox = Sandbox::new("fg-sleep");
    let mut shell = Shell::spawn(&sandbox.spec(), SIZE);
    let pid = shell.query("\"$$\"");
    let _guard = SleepGuard(pid.clone());

    shell.send("/bin/sleep 30\n");
    shell.wait_foreground(Foreground::Other);
    assert!(child_sleep_exists(&pid), "前面の sleep が見つからない");
}

#[test]
fn sleep_が終わるとシェルに戻る() {
    let _serial = serial();
    let sandbox = Sandbox::new("fg-back");
    let mut shell = Shell::spawn(&sandbox.spec(), SIZE);
    let pid = shell.query("\"$$\"");
    let _guard = SleepGuard(pid.clone());

    shell.send("/bin/sleep 30\n");
    shell.wait_foreground(Foreground::Other);
    end_child_sleep(&pid);

    shell.wait_foreground(Foreground::Shell);
    assert!(!child_sleep_exists(&pid), "sleep が残っている");
    assert_eq!(shell.query("after"), "after", "シェルが入力を受け付ける");
}

#[test]
fn シェルが終わったあとは不明を返す() {
    let _serial = serial();
    let sandbox = Sandbox::new("fg-exit");
    let mut shell = Shell::spawn(&sandbox.spec(), SIZE);
    assert_eq!(shell.query("ready"), "ready");

    shell.send("exit\n");
    assert_eq!(shell.wait_exit(), 0);
    shell.wait_output_end();

    assert_eq!(shell.session.foreground(), Foreground::Unknown);
}

#[test]
fn 既定のシェルはユーザー情報から引き引けなければ_bin_zsh_にする() {
    let _serial = serial();
    let user = std::env::var("USER").expect("USER が無い");
    let shell = default_shell(&user);
    assert!(shell.is_absolute(), "{shell:?}");
    assert!(shell.is_file(), "{shell:?} が無い");

    assert_eq!(
        default_shell("arin-no-such-user"),
        PathBuf::from(FALLBACK_SHELL)
    );
    assert_eq!(default_shell("../../etc"), PathBuf::from(FALLBACK_SHELL));
    assert_eq!(default_shell(""), PathBuf::from(FALLBACK_SHELL));
}

#[test]
fn 言語設定はあれば空白を含まない_1_語() {
    let _serial = serial();
    // CI のランナーには言語設定が無いことがあるので、あるときだけ形を確かめる。
    if let Some(locale) = system_locale() {
        assert!(!locale.is_empty());
        assert!(!locale.contains(char::is_whitespace), "{locale:?}");
    }
}
