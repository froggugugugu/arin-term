//! `Workspace` のターミナルの規則: 接続・入力・寸法・シェルの終了の判定と、タブの追加・閉じる・切替
//! (設計書 §7.1「ターミナルの起動」、FR-039・FR-040・FR-044)。
//!
//! 起動の仕様は cwd(プロジェクトのルート、検証画面ではホーム)と親の環境変数から組み立てる。
//! シェルが終わったタブは、通常は閉じ、検証画面では残す(`ExitPolicy`。計画書 M2 の【仮定】5)。

use std::path::Path;

use super::outcome::WorkspaceError;
use super::state::{TerminalEntry, Workspace};
use crate::ports::ChildExit;
use crate::project::{ProjectError, ProjectId};
use crate::terminal::{
    ExitAction, ShellLaunchSpec, TerminalError, TerminalId, TerminalStatus, TerminalTab,
};

/// 接続のコマンドで行うこと。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectPlan {
    /// この仕様でシェルを起動する(最初の接続、または前回の起動の失敗のあと)。
    Spawn(ShellLaunchSpec),
    /// 起動済み。出力の送り先だけを付け替える(画面の再読み込みなど)。
    Reattach,
}

impl Workspace {
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
    /// `Running` のときだけ `Exited` にし、`ExitPolicy` に従ってタブを閉じる(`ExitAction::Closed`。
    /// セッションの後片付けは呼び出し側)か残す(`ExitAction::Kept`)。それ以外(ID が無い、既に終わっている)は
    /// 何もせず `None` を返す。
    pub fn mark_exited(&mut self, id: &TerminalId, exit: Option<ChildExit>) -> Option<ExitAction> {
        let entry = self.terminals.get_mut(id)?;
        if entry.status != TerminalStatus::Running {
            return None;
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
        let action = self.tabs.on_shell_exited(id).ok()?;
        if matches!(action, ExitAction::Closed(_)) {
            self.terminals.remove(id);
        }
        Some(action)
    }

    /// 入力を書き込めるか(シェルが動いているときだけ)。
    ///
    /// # Errors
    ///
    /// ID が無いとき、まだ起動していないとき、シェルが終わっているとき。
    pub fn check_input(&self, id: &TerminalId) -> Result<(), TerminalError> {
        match &find(self, id)?.status {
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
        match &find(self, id)?.status {
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
        find(self, id).map(|_| ())
    }

    /// プロジェクトに新しいタブを開く(FR-039・FR-044)。末尾に足して表示中にし、cwd はルート。0 枚からも開ける。
    ///
    /// # Errors
    ///
    /// プロジェクトが無いとき(`WorkspaceError::Project`)、ルートが見つからないとき(`WorkspaceError::RootMissing`)。
    pub fn open_terminal(&mut self, project: &ProjectId) -> Result<TerminalId, WorkspaceError> {
        let found = self
            .projects
            .get(project)
            .ok_or_else(|| ProjectError::NotFound(project.clone()))?;
        if found.is_missing() {
            return Err(WorkspaceError::RootMissing(project.clone()));
        }
        self.add_tab(project)
            .ok_or_else(|| ProjectError::NotFound(project.clone()).into())
    }

    /// タブを閉じ、閉じたタブを返す(FR-044)。表示中だったなら右隣(無ければ左)が表示中になる。最後の 1 枚なら 0 枚。
    /// セッションの後片付けは呼び出し側。
    ///
    /// # Errors
    ///
    /// その ID のタブが無いとき(`TerminalError::NotFound`)。
    pub fn close_terminal(&mut self, id: &TerminalId) -> Result<TerminalTab, TerminalError> {
        let tab = self.tabs.close(id)?;
        self.terminals.remove(id);
        Ok(tab)
    }

    /// そのタブを表示中にする。
    ///
    /// # Errors
    ///
    /// その ID のタブが無いとき(`TerminalError::NotFound`)。
    pub fn activate_terminal(&mut self, id: &TerminalId) -> Result<(), TerminalError> {
        self.tabs.activate(id)
    }

    /// 選択中のプロジェクト(無ければ持ち主の無いタブ)の次のタブを表示中にし、その ID を返す。末尾の次は先頭。0 枚なら `None`。
    pub fn select_next_terminal(&mut self) -> Option<TerminalId> {
        let project = self.projects.selected().cloned();
        self.tabs.select_next(project.as_ref())
    }

    /// 選択中のプロジェクト(無ければ持ち主の無いタブ)の前のタブを表示中にし、その ID を返す。先頭の前は末尾。0 枚なら `None`。
    pub fn select_previous_terminal(&mut self) -> Option<TerminalId> {
        let project = self.projects.selected().cloned();
        self.tabs.select_previous(project.as_ref())
    }
}

fn find<'a>(ws: &'a Workspace, id: &TerminalId) -> Result<&'a TerminalEntry, TerminalError> {
    ws.terminals
        .get(id)
        .ok_or_else(|| TerminalError::NotFound(id.clone()))
}

fn find_mut<'a>(
    terminals: &'a mut std::collections::HashMap<TerminalId, TerminalEntry>,
    id: &TerminalId,
) -> Result<&'a mut TerminalEntry, TerminalError> {
    terminals
        .get_mut(id)
        .ok_or_else(|| TerminalError::NotFound(id.clone()))
}
