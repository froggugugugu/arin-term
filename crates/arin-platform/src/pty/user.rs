//! ユーザー情報とシステムの設定から、ターミナルの起動に要る値を引く(設計書 §7.1「ターミナルの起動」の 1)。
//!
//! どちらも macOS 標準のコマンドを絶対パスで起動して読む(`unsafe` と新しい依存を増やさないため)。
//! 環境変数は読まない(読むのは `src-tauri/src/launch.rs` だけ)。ユーザー名は引数で受け取る。
//!
//! どちらのコマンドにも上限時間([`PROBE_LIMIT`])を付ける。呼び出し側(`src-tauri/src/sessions.rs`)は
//! 全ターミナルの接続を直列にするロックの中で呼ぶので、コマンドが戻らないと全部の接続が止まるため。
//! 超えたら子プロセスを kill して回収し、代わりの値を使う。

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const DSCL: &str = "/usr/bin/dscl";
const DEFAULTS: &str = "/usr/bin/defaults";

/// コマンドの終了を待つ上限。ふだんは数十ミリ秒で終わる。負荷の高い起動直後でも足りるよう余裕を取りつつ、
/// 戻らないときに接続を長く止めない値にする。
const PROBE_LIMIT: Duration = Duration::from_secs(2);
/// 終了を見回る間隔。
const POLL: Duration = Duration::from_millis(10);
/// 終了のあと、標準出力の読み取りの終わりを待つ最小の時間(上限時間ぎりぎりで終わったときに出力を取りこぼさない)。
const DRAIN_GRACE: Duration = Duration::from_millis(100);

/// 起動するコマンドと上限時間。テストでは差し替える。
#[derive(Debug, Clone, Copy)]
struct Probe<'a> {
    program: &'a str,
    /// 呼び出しごとの引数の前に置く引数。
    leading: &'a [&'a str],
    limit: Duration,
}

const DSCL_PROBE: Probe<'static> = Probe {
    program: DSCL,
    leading: &[],
    limit: PROBE_LIMIT,
};

const DEFAULTS_PROBE: Probe<'static> = Probe {
    program: DEFAULTS,
    leading: &[],
    limit: PROBE_LIMIT,
};

/// コマンドの結果。
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    /// 成功で終わった。標準出力(UTF-8)。
    Output(String),
    /// 起動できない、失敗で終わった、出力が UTF-8 でない。
    Failed,
    /// 上限時間を超えたので kill して回収した。
    TimedOut { pid: u32 },
}

impl Probe<'_> {
    /// コマンドを起動し、上限時間まで終了を見回る。超えたら kill して回収する。
    fn run(&self, args: &[&str]) -> Outcome {
        let Ok(mut child) = Command::new(self.program)
            .args(self.leading)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        else {
            return Outcome::Failed;
        };
        let pid = child.id();
        let deadline = Instant::now() + self.limit;
        // 標準出力は別のスレッドで読み切る。読まずに終了を待つと、パイプが満ちたところで子が書き込みで止まる。
        // 打ち切ったときは、kill でパイプの書き手が閉じるので、このスレッドも読み終えて抜ける。
        let stdout = child.stdout.take().and_then(|mut out| {
            let (tx, rx) = mpsc::channel();
            thread::Builder::new()
                .name("arin-probe-stdout".to_owned())
                .spawn(move || {
                    let mut bytes = Vec::new();
                    let _ = tx.send(out.read_to_end(&mut bytes).map(|_| bytes));
                })
                .ok()
                .map(|_| rx)
        });
        let Some(stdout) = stdout else {
            let _ = child.kill();
            let _ = child.wait();
            return Outcome::Failed;
        };

        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) if Instant::now() < deadline => thread::sleep(POLL),
                Ok(None) => break Err(true),
                Err(_) => break Err(false),
            }
        };
        let status = match status {
            Ok(status) => status,
            Err(timed_out) => {
                let _ = child.kill();
                let _ = child.wait();
                return if timed_out {
                    Outcome::TimedOut { pid }
                } else {
                    Outcome::Failed
                };
            }
        };
        if !status.success() {
            return Outcome::Failed;
        }
        let left = deadline
            .saturating_duration_since(Instant::now())
            .max(DRAIN_GRACE);
        match stdout.recv_timeout(left) {
            Ok(Ok(bytes)) => String::from_utf8(bytes).map_or(Outcome::Failed, Outcome::Output),
            _ => Outcome::Failed,
        }
    }

    /// 成功したら標準出力を返す。
    fn stdout(&self, args: &[&str]) -> Option<String> {
        match self.run(args) {
            Outcome::Output(stdout) => Some(stdout),
            Outcome::Failed | Outcome::TimedOut { .. } => None,
        }
    }
}

/// ユーザー情報から既定のシェルを引けないときに使うシェル(macOS の既定)。
pub const FALLBACK_SHELL: &str = "/bin/zsh";

/// ユーザー情報(ローカルのディレクトリサービスの `UserShell`)に登録された既定のシェルを返す。
///
/// 引けないとき(ユーザーが無い、名前に `/` を含む、登録されたシェルが絶対パスでないかファイルが無い)は
/// [`FALLBACK_SHELL`] を返す。Finder から起動したアプリには `SHELL` が渡らないことがあるので、環境変数は見ない。
#[must_use]
pub fn default_shell(user_name: &str) -> PathBuf {
    shell_with(user_name, &DSCL_PROBE)
}

fn shell_with(user_name: &str, probe: &Probe<'_>) -> PathBuf {
    lookup_user_shell(user_name, probe).unwrap_or_else(|| PathBuf::from(FALLBACK_SHELL))
}

/// システムの言語設定の識別子(例 `ja_JP`)を返す。`ShellLaunchSpec::new` の `system_locale` に渡す。
///
/// 設定が無いとき、読めないときは `None`(`LANG` は `ShellLaunchSpec` が代わりの値にする)。
#[must_use]
pub fn system_locale() -> Option<String> {
    locale_with(&DEFAULTS_PROBE)
}

fn locale_with(probe: &Probe<'_>) -> Option<String> {
    let stdout = probe.stdout(&["read", "-g", "AppleLocale"])?;
    parse_locale(&stdout)
}

fn lookup_user_shell(user_name: &str, probe: &Probe<'_>) -> Option<PathBuf> {
    if !is_plain_user_name(user_name) {
        return None;
    }
    let record = format!("/Users/{user_name}");
    let stdout = probe.stdout(&[".", "-read", &record, "UserShell"])?;
    parse_user_shell(&stdout).filter(|shell| shell.is_file())
}

/// ユーザー名として受け付けるか。レコードのパスを別の場所へ向けられないよう、`/` と `.`・`..` を断る。
fn is_plain_user_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\0'])
}

/// `dscl` の出力(`UserShell: /bin/zsh`。値に空白があると次の行に字下げして出る)から値を取り出す。
fn parse_user_shell(stdout: &str) -> Option<PathBuf> {
    let value = stdout.trim().strip_prefix("UserShell:")?.trim();
    if value.is_empty() || value.contains('\n') {
        return None;
    }
    let shell = Path::new(value);
    shell.is_absolute().then(|| shell.to_path_buf())
}

/// `defaults` の出力(`ja_JP` と改行)から、空白を含まない 1 語だけを受け付ける。
fn parse_locale(stdout: &str) -> Option<String> {
    let value = stdout.trim();
    (!value.is_empty() && !value.contains(char::is_whitespace)).then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    /// 引数を無視して `script` を動かす偽のコマンド(`sh -c script sh 引数…`)。
    fn fake(script: &'static str, limit: Duration) -> Probe<'static> {
        Probe {
            program: "/bin/sh",
            leading: Box::leak(Box::new(["-c", script, "sh"])),
            limit,
        }
    }

    /// プロセスが残っているか(`kill -0`)。
    fn alive(pid: u32) -> bool {
        Command::new("/bin/kill")
            .args(["-0", &pid.to_string()])
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    #[test]
    fn 上限時間を超えたコマンドは打ち切り_子プロセスを残さない() {
        let probe = fake("exec /bin/sleep 30", Duration::from_millis(200));
        let started = Instant::now();

        let outcome = probe.run(&[]);

        assert!(
            started.elapsed() < Duration::from_secs(5),
            "打ち切らずに待った({:?})",
            started.elapsed()
        );
        let Outcome::TimedOut { pid } = outcome else {
            panic!("打ち切りにならなかった: {outcome:?}");
        };
        assert!(!alive(pid), "打ち切った子プロセス {pid} が残っている");
    }

    #[test]
    fn 上限時間を超えたら代わりの値を使う() {
        let slow = fake("exec /bin/sleep 30", Duration::from_millis(200));

        assert_eq!(shell_with("alice", &slow), PathBuf::from(FALLBACK_SHELL));
        assert_eq!(locale_with(&slow), None);
    }

    #[test]
    fn 上限時間内に終われば標準出力を読む_引数は後ろに渡る() {
        let probe = fake(
            "echo \"UserShell: /bin/sh\"; echo \"$2\" >&2",
            Duration::from_secs(5),
        );
        assert_eq!(shell_with("alice", &probe), PathBuf::from("/bin/sh"));

        let echo = fake("echo \"$@\"", Duration::from_secs(5));
        assert_eq!(
            echo.run(&["read", "-g", "AppleLocale"]),
            Outcome::Output("read -g AppleLocale\n".to_owned())
        );
        assert_eq!(
            locale_with(&fake("echo ja_JP", Duration::from_secs(5))),
            Some("ja_JP".to_owned())
        );
    }

    #[test]
    fn 大きな出力もパイプを詰まらせずに読み切る() {
        // パイプの容量(64 KiB)を超える出力。読まずに終わりを待つと、子が書き込みで止まって打ち切りになる。
        let probe = fake(
            "/usr/bin/yes x | /usr/bin/head -c 1000000",
            Duration::from_secs(10),
        );

        let Outcome::Output(stdout) = probe.run(&[]) else {
            panic!("読み切れなかった");
        };
        assert_eq!(stdout.len(), 1_000_000);
    }

    #[test]
    fn 失敗したコマンドと起動できないコマンドは失敗になる() {
        assert_eq!(
            fake("exit 3", Duration::from_secs(5)).run(&[]),
            Outcome::Failed
        );
        let missing = Probe {
            program: "/nonexistent/arin-probe",
            leading: &[],
            limit: Duration::from_secs(5),
        };
        assert_eq!(missing.run(&[]), Outcome::Failed);
    }

    #[test]
    fn 本物のコマンドの上限時間は_2_秒() {
        assert_eq!(DSCL_PROBE.limit, Duration::from_secs(2));
        assert_eq!(DEFAULTS_PROBE.limit, Duration::from_secs(2));
        assert_eq!(DSCL_PROBE.program, "/usr/bin/dscl");
        assert_eq!(DEFAULTS_PROBE.program, "/usr/bin/defaults");
    }

    #[test]
    fn dscl_の出力からシェルを取り出す() {
        assert_eq!(
            parse_user_shell("UserShell: /bin/zsh\n"),
            Some(PathBuf::from("/bin/zsh"))
        );
        assert_eq!(
            parse_user_shell("UserShell:\n /opt/my shell/bin/fish\n"),
            Some(PathBuf::from("/opt/my shell/bin/fish"))
        );
    }

    #[test]
    fn 想定外の_dscl_の出力は受け付けない() {
        assert_eq!(parse_user_shell(""), None);
        assert_eq!(parse_user_shell("UserShell:\n"), None);
        assert_eq!(parse_user_shell("UserShell: zsh\n"), None);
        assert_eq!(parse_user_shell("RealName: zsh\n"), None);
        assert_eq!(parse_user_shell("UserShell: /bin/zsh\n /bin/bash\n"), None);
    }

    #[test]
    fn ユーザー名は_スラッシュ_や_ドット_を含まないものだけ受け付ける() {
        assert!(is_plain_user_name("alice"));
        assert!(is_plain_user_name("_mbsetupuser"));
        assert!(!is_plain_user_name(""));
        assert!(!is_plain_user_name("."));
        assert!(!is_plain_user_name(".."));
        assert!(!is_plain_user_name("../root"));
        assert!(!is_plain_user_name("a\0b"));
    }

    #[test]
    fn defaults_の出力から言語設定の識別子を取り出す() {
        assert_eq!(parse_locale("ja_JP\n"), Some("ja_JP".to_owned()));
        assert_eq!(
            parse_locale("en_US@rg=jpzzzz\n"),
            Some("en_US@rg=jpzzzz".to_owned())
        );
        assert_eq!(parse_locale("\n"), None);
        assert_eq!(parse_locale("ja JP\n"), None);
    }
}
