//! `portable-pty` 0.9.0 による `PtyPort` の実装(ADR-004)。
//!
//! `portable-pty` の起動処理(`CommandBuilder` を `std::process::Command` にする部分)には、次の癖がある。
//! 仕様のとおりに起動するため、ここで先回りして確かめる。
//!
//! - `CommandBuilder::new(prog)` は `prog` をパスの解決と `argv[0]` の両方に使うので、`argv[0]` を `-zsh` にできない。
//!   既定のプログラム(`new_default_prog`)で作ると、`SHELL` のシェルを `-` とファイル名の `argv[0]` で起動する。
//!   ただし `SHELL` が実行できないと、ユーザー情報のシェル(なければ `/bin/sh`)に黙って切り替える
//! - 作った時点で親の環境変数を丸ごと取り込む(`env_clear` で捨てる)
//! - cwd がフォルダでないと、エラーにせずホームに切り替える

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::thread;
use std::time::{Duration, Instant};

use arin_core::ports::{ChildExit, Foreground, PtyError, PtyPort, PtySession, PtySize, SpawnedPty};
use arin_core::terminal::ShellLaunchSpec;
use portable_pty::{Child, CommandBuilder, MasterPty, native_pty_system};

/// OS の PTY でシェルを起動する。状態を持たないので、1 つを全ターミナルで使い回せる。
#[derive(Debug, Default, Clone, Copy)]
pub struct NativePty;

impl PtyPort for NativePty {
    fn spawn(&self, spec: &ShellLaunchSpec, size: PtySize) -> Result<SpawnedPty, PtyError> {
        let command = command_for(spec)?;
        let pair = native_pty_system()
            .openpty(pty_size(size))
            .map_err(spawn_error)?;
        // 読み書きの口は、シェルを起動する前に用意する(起動後に失敗してシェルが残るのを避ける)。
        let output = pair.master.try_clone_reader().map_err(spawn_error)?;
        let writer = pair.master.take_writer().map_err(spawn_error)?;
        let child = pair.slave.spawn_command(command).map_err(spawn_error)?;
        // 端末の側をこちらで開いたままにすると、シェルが終わっても出力が終わらない。
        drop(pair.slave);

        Ok(SpawnedPty {
            session: Box::new(NativePtySession {
                master: Some(pair.master),
                writer: Some(writer),
                child,
            }),
            output,
        })
    }
}

/// 仕様を `CommandBuilder` にする。`portable-pty` が黙って別の値に切り替える条件は、ここで失敗にする。
fn command_for(spec: &ShellLaunchSpec) -> Result<CommandBuilder, PtyError> {
    let program = spec.program();
    let program_str = program.to_str().ok_or_else(|| {
        PtyError::Spawn(format!(
            "シェルのパス {} が UTF-8 でない",
            program.display()
        ))
    })?;
    let executable = program
        .metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0);
    if !executable {
        return Err(PtyError::Spawn(format!(
            "シェル {} が無いか、実行できない",
            program.display()
        )));
    }
    // `portable-pty` は `argv[0]` を `-` と、パスの最後の `/` より後ろから作る。仕様の値と食い違えば起動しない。
    let basename = program_str.rsplit('/').next().unwrap_or(program_str);
    if spec.argv0() != format!("-{basename}").as_str() {
        return Err(PtyError::Spawn(format!(
            "argv[0] {} を、シェル {} の起動で再現できない",
            spec.argv0().display(),
            program.display()
        )));
    }
    let cwd = spec.cwd();
    if !cwd.is_dir() {
        return Err(PtyError::Spawn(format!(
            "作業フォルダ {} が無いか、フォルダでない",
            cwd.display()
        )));
    }

    let mut command = CommandBuilder::new_default_prog();
    command.env_clear();
    for (name, value) in spec.env() {
        command.env(name, value);
    }
    // 起動するシェルは `SHELL` で決まる。仕様の `SHELL` と同じ値だが、起動するものを明示する。
    command.env("SHELL", program);
    command.cwd(cwd);
    Ok(command)
}

fn pty_size(size: PtySize) -> portable_pty::PtySize {
    portable_pty::PtySize {
        rows: size.rows,
        cols: size.cols,
        pixel_width: 0,
        pixel_height: 0,
    }
}

fn spawn_error(error: impl std::fmt::Display) -> PtyError {
    PtyError::Spawn(format!("{error:#}"))
}

/// 捨てるときに、シェルの回収を待つ上限。過ぎたら待つのをやめる(呼び出し側=タブを閉じる処理やアプリの終了を止めない)。
const REAP_TIMEOUT: Duration = Duration::from_secs(1);
/// 回収を確かめる間隔。
const REAP_POLL: Duration = Duration::from_millis(10);

/// 起動した PTY 1 組。捨てると(`Drop`)、シェルがまだ動いていれば終了させて回収する。
/// `master` と `writer` は捨てるときにだけ `None` になる(回収を待つ前に親側を閉じるため)。
struct NativePtySession {
    master: Option<Box<dyn MasterPty + Send>>,
    writer: Option<Box<dyn Write + Send>>,
    child: Box<dyn Child + Send + Sync>,
}

impl NativePtySession {
    /// まだ動いているか。終わっていればここで回収される。
    fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

impl PtySession for NativePtySession {
    fn write(&mut self, data: &[u8]) -> Result<(), PtyError> {
        let writer = self
            .writer
            .as_mut()
            .ok_or_else(|| PtyError::Write("PTY は閉じている".to_owned()))?;
        writer
            .write_all(data)
            .and_then(|()| writer.flush())
            .map_err(|error| PtyError::Write(error.to_string()))
    }

    fn resize(&mut self, size: PtySize) -> Result<(), PtyError> {
        self.master
            .as_ref()
            .ok_or_else(|| PtyError::Resize("PTY は閉じている".to_owned()))?
            .resize(pty_size(size))
            .map_err(|error| PtyError::Resize(format!("{error:#}")))
    }

    /// `SIGHUP` を送り、最大でおよそ 200 ミリ秒のあいだに終わらなければ `SIGKILL` を送る(`portable-pty` の動作)。
    /// 既に終わったシェルには何も送らない(回収済みのプロセス番号が別のプロセスに使い回されている恐れがある)。
    fn kill(&mut self) -> Result<(), PtyError> {
        if !self.is_running() {
            return Ok(());
        }
        self.child
            .kill()
            .map_err(|error| PtyError::Kill(error.to_string()))
    }

    fn try_wait(&mut self) -> Result<Option<ChildExit>, PtyError> {
        self.child
            .try_wait()
            .map(|status| {
                status.map(|status| ChildExit {
                    exit_code: status.exit_code(),
                    signal: status.signal().map(str::to_owned),
                })
            })
            .map_err(|error| PtyError::Wait(error.to_string()))
    }

    /// 端末の前面のプロセスグループ(`tcgetpgrp`)の番号をシェルの pid と比べる。
    /// シェルは新しいセッションの先頭として起動するので、コマンドを待つあいだは前面のグループの番号が自分の pid になる。
    /// 終わったシェルには `Unknown` を返す(回収済みの pid が別のプロセスに使い回されている恐れがある)。
    fn foreground(&mut self) -> Foreground {
        if !self.is_running() {
            return Foreground::Unknown;
        }
        let shell = self
            .child
            .process_id()
            .and_then(|pid| i32::try_from(pid).ok());
        let leader = self
            .master
            .as_ref()
            .and_then(|master| master.process_group_leader());
        match (leader, shell) {
            (Some(leader), Some(shell)) if leader == shell => Foreground::Shell,
            (Some(_), Some(_)) => Foreground::Other,
            _ => Foreground::Unknown,
        }
    }
}

impl Drop for NativePtySession {
    fn drop(&mut self) {
        // 先に親側(書き込みの口と master)を閉じる。開いたままだと、前面でコマンドが動いていたシェルは、終了の途中で
        // 端末の出力が読み出されるのを待って止まり(この待ちには `SIGKILL` も効かない)、回収の待ちと互いに待ち合う。
        // 読み取りの口(`SpawnedPty::output`)は読み取りの側が持つ。読み続けるか手放せば、同じく終了が進む。
        self.writer = None;
        self.master = None;
        if !self.is_running() {
            return;
        }
        // `kill` は最後に `SIGKILL` を送る。ゾンビを残さないよう回収するが、上限を過ぎたら待たない。
        let _ = self.child.kill();
        let deadline = Instant::now() + REAP_TIMEOUT;
        while self.is_running() && Instant::now() < deadline {
            thread::sleep(REAP_POLL);
        }
    }
}
