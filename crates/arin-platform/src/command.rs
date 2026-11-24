//! macOS 標準のコマンドを絶対パスで起動し、上限時間まで待って標準出力を読む(`pty/user.rs` と `release/` が使う)。
//!
//! 超えたら子プロセスを kill して回収する。標準出力は別のスレッドで読み切る。

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// 終了を見回る間隔。
const POLL: Duration = Duration::from_millis(10);
/// 終了のあと、標準出力の読み取りの終わりを待つ最小の時間(上限時間ぎりぎりで終わったときに出力を取りこぼさない)。
const DRAIN_GRACE: Duration = Duration::from_millis(100);

/// 起動するコマンドと上限時間。テストでは差し替える。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Probe<'a> {
    pub(crate) program: &'a str,
    /// 呼び出しごとの引数の前に置く引数。
    pub(crate) leading: &'a [&'a str],
    pub(crate) limit: Duration,
}

/// コマンドの結果。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// 成功で終わった。標準出力(UTF-8)。
    Output(String),
    /// 起動できない、失敗で終わった、出力が UTF-8 でない。
    Failed,
    /// 上限時間を超えたので kill して回収した。
    TimedOut { pid: u32 },
}

impl Probe<'_> {
    /// コマンドを起動し、上限時間まで終了を見回る。超えたら kill して回収する。
    pub(crate) fn run(&self, args: &[&str]) -> Outcome {
        self.run_capped(args, None)
    }

    /// `run` に、標準出力の大きさの上限(バイト)を付けたもの。超えたら残りは読み捨てて失敗にする
    /// (curl の `--max-filesize` は大きさの分からない応答では効かないため)。
    pub(crate) fn run_capped(&self, args: &[&str], max_output: Option<u64>) -> Outcome {
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
                    let read = match max_output {
                        None => out.read_to_end(&mut bytes).map(|_| Some(bytes)),
                        Some(max) => {
                            (&mut out)
                                .take(max + 1)
                                .read_to_end(&mut bytes)
                                .and_then(|_| {
                                    if bytes.len() as u64 > max {
                                        // 子が書き込みで止まらないよう、残りは読み捨てる
                                        std::io::copy(&mut out, &mut std::io::sink()).map(|_| None)
                                    } else {
                                        Ok(Some(bytes))
                                    }
                                })
                        }
                    };
                    let _ = tx.send(read);
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
            Ok(Ok(Some(bytes))) => {
                String::from_utf8(bytes).map_or(Outcome::Failed, Outcome::Output)
            }
            _ => Outcome::Failed,
        }
    }

    /// 成功したら標準出力を返す。
    pub(crate) fn stdout(&self, args: &[&str]) -> Option<String> {
        match self.run(args) {
            Outcome::Output(stdout) => Some(stdout),
            Outcome::Failed | Outcome::TimedOut { .. } => None,
        }
    }
}
