//! PTY のポート(設計書 §1.3 #2、§5.1、§7.1、ADR-004)。
//!
//! ターミナル 1 枚につき PTY 1 組を持つ。実装は `arin-platform/src/pty/` の `NativePty`(`portable-pty` 0.9.0)で、
//! ここにはクレートの型を出さない。クレートを替えても影響は実装側に閉じる。

use std::io::Read;

use thiserror::Error;

use crate::terminal::ShellLaunchSpec;

/// 端末の寸法(行・桁)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PtySize {
    /// 行数。
    pub rows: u16,
    /// 桁数。
    pub cols: u16,
}

/// 子プロセス(シェル)の終わり方。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildExit {
    /// 終了コード。シグナルで終わったときは実装が決めた 0 以外の値。
    pub exit_code: u32,
    /// シグナルで終わったときのシグナルの名前。
    pub signal: Option<String>,
}

/// PTY の操作の失敗。原因は実装側の文言をそのまま持つ。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PtyError {
    /// PTY を開くか、シェルを起動できなかった。
    #[error("シェルを起動できなかった: {0}")]
    Spawn(String),
    /// 入力を書き込めなかった。
    #[error("ターミナルへの書き込みに失敗した: {0}")]
    Write(String),
    /// 寸法を変えられなかった。
    #[error("ターミナルの寸法を変えられなかった: {0}")]
    Resize(String),
    /// シェルを終了させられなかった。
    #[error("シェルを終了させられなかった: {0}")]
    Kill(String),
    /// シェルが終わったかどうかを確かめられなかった。
    #[error("シェルの終了の確認に失敗した: {0}")]
    Wait(String),
}

/// PTY を開いてログインシェルを起動する口。
pub trait PtyPort: Send + Sync {
    /// `spec` のとおりにシェルを起動し、`size` の寸法の PTY につなぐ。
    ///
    /// # Errors
    ///
    /// PTY を開けないとき、シェルを起動できないとき(`PtyError::Spawn`)。
    fn spawn(&self, spec: &ShellLaunchSpec, size: PtySize) -> Result<SpawnedPty, PtyError>;
}

/// 起動した PTY。出力の読み取りと、それ以外の操作を分けて持つ。
pub struct SpawnedPty {
    /// 入力・寸法・終了の操作。
    pub session: Box<dyn PtySession>,
    /// PTY の出力。読み取りは呼び出し側の専用スレッドが回す(T17)。読み取りは出力が来るまで待つ。
    /// シェルと、端末を開いたままの子孫がすべて終わると、終わり(0 バイト)かエラーを返す。
    pub output: Box<dyn Read + Send>,
}

/// 起動した PTY 1 組への操作。
pub trait PtySession: Send {
    /// 入力をすべて書き込む。
    ///
    /// # Errors
    ///
    /// 書き込めなかったとき(`PtyError::Write`)。
    fn write(&mut self, data: &[u8]) -> Result<(), PtyError>;

    /// 寸法を変える。シェルには端末の寸法変更として届く。
    ///
    /// # Errors
    ///
    /// 変えられなかったとき(`PtyError::Resize`)。
    fn resize(&mut self, size: PtySize) -> Result<(), PtyError>;

    /// シェルを終了させる。終わるのは待たない(終わったかは `try_wait` で見る)。
    ///
    /// # Errors
    ///
    /// 終了させられなかったとき(`PtyError::Kill`)。
    fn kill(&mut self) -> Result<(), PtyError>;

    /// シェルが終わっていればその終わり方を、まだ動いていれば `None` を返す。待たない。
    ///
    /// 出力の終わりだけでは終了を検知できない(端末を開いたままの子孫が残ると出力が終わらない)ので、
    /// 呼び出し側は出力の終わりのあとと、一定の間隔でこれを呼ぶ。
    ///
    /// # Errors
    ///
    /// 確かめられなかったとき(`PtyError::Wait`)。
    fn try_wait(&mut self) -> Result<Option<ChildExit>, PtyError>;
}
