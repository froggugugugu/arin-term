//! ストアをまたぐ操作の入口(設計書 §5.1、§6.3)。
//!
//! M1 は単一プロジェクト・単一タブ。【仮定】ターミナルの cwd はホームフォルダ(計画書 M1 の冒頭)。
//! 入出力はしない。PTY の起動・読み書きは `src-tauri/src/sessions.rs` が、ここの判定に従って行う。
//! 呼び出し側は Mutex を取ってここを読み書きし、PTY の操作のあいだは Mutex を持たない(設計書 §7.1「スレッドの分担」)。

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use super::AppSnapshot;
use crate::ports::ChildExit;
use crate::terminal::{
    ShellLaunchSpec, TerminalError, TerminalId, TerminalSnapshot, TerminalStatus,
};

/// 接続のコマンドで行うこと。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectPlan {
    /// この仕様でシェルを起動する(最初の接続、または前回の起動の失敗のあと)。
    Spawn(ShellLaunchSpec),
    /// 起動済み。出力の送り先だけを付け替える(画面の再読み込みなど)。
    Reattach,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TerminalEntry {
    id: TerminalId,
    cwd: PathBuf,
    status: TerminalStatus,
}

/// アプリの状態の正本。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    app_version: String,
    parent_env: Vec<(OsString, OsString)>,
    terminals: Vec<TerminalEntry>,
}

impl Workspace {
    /// 起動時の状態を作る。ターミナルを 1 枚、まだ起動していない状態で登録する(PTY は最初の接続で起動する)。
    ///
    /// - `app_version`: アプリの版
    /// - `home`: ホームフォルダ(ターミナルの cwd)。分からなければ空のパスを渡す(接続のときに起動が失敗する)
    /// - `parent_env`: アプリが受け取った環境変数。シェルへは許可リストの名前だけが渡る(`ShellLaunchSpec`)
    pub fn new<I, K, V>(app_version: impl Into<String>, home: PathBuf, parent_env: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        Self {
            app_version: app_version.into(),
            parent_env: parent_env
                .into_iter()
                .map(|(k, v)| (k.as_ref().to_owned(), v.as_ref().to_owned()))
                .collect(),
            terminals: vec![TerminalEntry {
                id: terminal_id(1),
                cwd: home,
                status: TerminalStatus::NotStarted,
            }],
        }
    }

    /// ターミナルを `count` 枚(`terminal-1` 〜 `terminal-<count>`)持つようにする。0 は 1 として扱う。
    /// どれも `new` で渡したホームフォルダを cwd にし、まだ起動していない状態で登録する(PTY はそれぞれの最初の接続で起動する)。
    ///
    /// M1 の 9 枚の検証画面(`ARIN_VERIFY_GRID`。T22)のためだけに使う。M4(T49)のタイル表示で外す前提。
    #[must_use]
    pub fn with_terminal_count(mut self, count: usize) -> Self {
        let cwd = self
            .terminals
            .first()
            .map(|t| t.cwd.clone())
            .unwrap_or_default();
        self.terminals = (1..=count.max(1))
            .map(|n| TerminalEntry {
                id: terminal_id(n),
                cwd: cwd.clone(),
                status: TerminalStatus::NotStarted,
            })
            .collect();
        self
    }

    /// 画面へ渡す状態の写し。
    #[must_use]
    pub fn snapshot(&self) -> AppSnapshot {
        AppSnapshot::new(
            self.app_version.clone(),
            self.terminals
                .iter()
                .map(|t| TerminalSnapshot {
                    id: t.id.clone(),
                    status: t.status.clone(),
                })
                .collect(),
        )
    }

    /// 接続のコマンドで行うことを決める。
    ///
    /// 起動していない(または前回失敗した)なら、`shell` と `system_locale` で起動の仕様を組み立てる。
    /// 組み立てられなければ状態を `Failed` にして理由を返す。起動できたかの反映は `mark_running` / `mark_failed`。
    ///
    /// # Errors
    ///
    /// ID が無いとき、シェルが終わっているとき、起動の仕様を組み立てられないとき。
    pub fn plan_connect(
        &mut self,
        id: &TerminalId,
        shell: &Path,
        system_locale: Option<&str>,
    ) -> Result<ConnectPlan, TerminalError> {
        let parent_env = &self.parent_env;
        let entry = find_mut(&mut self.terminals, id)?;
        match entry.status {
            TerminalStatus::Running => Ok(ConnectPlan::Reattach),
            TerminalStatus::Exited { .. } => Err(TerminalError::Exited(id.clone())),
            TerminalStatus::NotStarted | TerminalStatus::Failed { .. } => {
                match ShellLaunchSpec::new(
                    shell,
                    &entry.cwd,
                    parent_env.iter().cloned(),
                    system_locale,
                ) {
                    Ok(spec) => Ok(ConnectPlan::Spawn(spec)),
                    Err(e) => {
                        entry.status = TerminalStatus::Failed {
                            message: e.to_string(),
                        };
                        Err(e.into())
                    }
                }
            }
        }
    }

    /// シェルを起動できた。
    ///
    /// # Errors
    ///
    /// ID が無いとき。
    pub fn mark_running(&mut self, id: &TerminalId) -> Result<(), TerminalError> {
        find_mut(&mut self.terminals, id)?.status = TerminalStatus::Running;
        Ok(())
    }

    /// シェルを起動できなかった。`message` は画面に出す理由。
    ///
    /// # Errors
    ///
    /// ID が無いとき。
    pub fn mark_failed(
        &mut self,
        id: &TerminalId,
        message: impl Into<String>,
    ) -> Result<(), TerminalError> {
        find_mut(&mut self.terminals, id)?.status = TerminalStatus::Failed {
            message: message.into(),
        };
        Ok(())
    }

    /// シェルが終わった。`exit` は終わり方(確かめられなかったときは `None`)。
    ///
    /// `Running` のときだけ `Exited` にして `true` を返す。それ以外(ID が無い、既に終わっている)は何もしない。
    pub fn mark_exited(&mut self, id: &TerminalId, exit: Option<ChildExit>) -> bool {
        let Ok(entry) = find_mut(&mut self.terminals, id) else {
            return false;
        };
        if entry.status != TerminalStatus::Running {
            return false;
        }
        entry.status = match exit {
            Some(exit) => TerminalStatus::Exited {
                code: Some(exit.exit_code),
                signal: exit.signal,
            },
            None => TerminalStatus::Exited {
                code: None,
                signal: None,
            },
        };
        true
    }

    /// 入力を書き込めるか(シェルが動いているときだけ)。
    ///
    /// # Errors
    ///
    /// ID が無いとき、まだ起動していないとき、シェルが終わっているとき。
    pub fn check_input(&self, id: &TerminalId) -> Result<(), TerminalError> {
        match &find(&self.terminals, id)?.status {
            TerminalStatus::Running => Ok(()),
            TerminalStatus::Exited { .. } => Err(TerminalError::Exited(id.clone())),
            TerminalStatus::NotStarted | TerminalStatus::Failed { .. } => {
                Err(TerminalError::NotStarted(id.clone()))
            }
        }
    }

    /// 寸法を変えるか。動いていれば `true`、終わっていれば `false`(何もしない。失敗にもしない)。
    ///
    /// # Errors
    ///
    /// ID が無いとき、まだ起動していないとき(初めの寸法は接続のコマンドで渡す)。
    pub fn check_resize(&self, id: &TerminalId) -> Result<bool, TerminalError> {
        match &find(&self.terminals, id)?.status {
            TerminalStatus::Running => Ok(true),
            TerminalStatus::Exited { .. } => Ok(false),
            TerminalStatus::NotStarted | TerminalStatus::Failed { .. } => {
                Err(TerminalError::NotStarted(id.clone()))
            }
        }
    }

    /// その ID のターミナルがあるか。
    ///
    /// # Errors
    ///
    /// ID が無いとき。
    pub fn check_exists(&self, id: &TerminalId) -> Result<(), TerminalError> {
        find(&self.terminals, id).map(|_| ())
    }
}

/// `n` 枚目(1 から)のターミナルの ID。
fn terminal_id(n: usize) -> TerminalId {
    TerminalId::new(format!("terminal-{n}"))
}

fn find<'a>(
    terminals: &'a [TerminalEntry],
    id: &TerminalId,
) -> Result<&'a TerminalEntry, TerminalError> {
    terminals
        .iter()
        .find(|t| &t.id == id)
        .ok_or_else(|| TerminalError::NotFound(id.clone()))
}

fn find_mut<'a>(
    terminals: &'a mut [TerminalEntry],
    id: &TerminalId,
) -> Result<&'a mut TerminalEntry, TerminalError> {
    terminals
        .iter_mut()
        .find(|t| &t.id == id)
        .ok_or_else(|| TerminalError::NotFound(id.clone()))
}
