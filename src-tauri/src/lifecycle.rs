//! 起動まわり(設計書 §4 の `lifecycle.rs`)。
//!
//! 画面からの準備完了の通知を受けたときの処理と、アプリの終了時の後片付けを置く。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Manager, RunEvent, Runtime};

use crate::sessions::SessionRegistry;

/// アプリのイベントループのイベントを受ける(`run` に渡す)。
///
/// 終了するときは、全ターミナルのシェルを終わらせる(PTY を残さない)。
pub fn on_run_event<R: Runtime>(app: &AppHandle<R>, event: RunEvent) {
    if let RunEvent::Exit = event
        && let Some(sessions) = app.try_state::<SessionRegistry>()
    {
        sessions.shutdown();
    }
}

/// 準備完了の通知を受けたときの処理。Tauri の管理状態に登録する。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadyNotice {
    marker: Option<PathBuf>,
}

impl ReadyNotice {
    /// `marker` が `Some` のときだけ、通知を受けたらそこに印を書く(起動スモーク用)。
    #[must_use]
    pub fn new(marker: Option<PathBuf>) -> Self {
        Self { marker }
    }

    /// 画面の準備ができたときに呼ぶ。
    ///
    /// 印が有効なら、中身にこのプロセスの ID を書く(スモークが自分の起こしたプロセスの印か確かめるため)。
    /// 印が無効なら何もしない。
    ///
    /// # Errors
    ///
    /// 印を書けなかったとき。理由を日本語の文字列で返す。
    pub fn on_app_ready(&self) -> Result<(), String> {
        let Some(marker) = &self.marker else {
            return Ok(());
        };
        write_pid_marker(marker)
    }
}

/// 印のファイルに、このプロセスの ID を書く(スモークが自分の起こしたプロセスの印か確かめるため)。
fn write_pid_marker(marker: &Path) -> Result<(), String> {
    let contents = format!("{}\n", std::process::id());
    arin_platform::storage::write_ready_marker(marker, &contents)
        .map_err(|e| format!("起動スモークの印 {} を書けなかった: {e}", marker.display()))
}

/// 画面が PTY の出力を初めて取り込んだことを受け取る処理。Tauri の管理状態に登録する。
///
/// 起動スモークが「実アプリで PTY の出力が画面に届き、エンジンが取り込み、処理済みの量が Rust に戻った」ことを
/// 確かめるための印を書く(スモークが有効なときだけ)。
#[derive(Debug)]
pub struct FirstOutputNotice {
    marker: Option<PathBuf>,
    written: AtomicBool,
}

impl FirstOutputNotice {
    /// `marker` が `Some` のときだけ、最初の処理済みの通知でそこに印を書く(起動スモーク用)。
    #[must_use]
    pub fn new(marker: Option<PathBuf>) -> Self {
        Self {
            marker,
            written: AtomicBool::new(false),
        }
    }

    /// 処理済みの通知(`bytes` バイト)を受けたときに呼ぶ。
    ///
    /// 印が有効で、`bytes` が 0 より大きい最初の通知のときだけ印を書く。2 回目以降は何もしない
    /// (書けなかったときも試し直さない。失敗は 1 回だけ返す)。
    ///
    /// # Errors
    ///
    /// 印を書けなかったとき。理由を日本語の文字列で返す。
    pub fn on_output_acked(&self, bytes: u64) -> Result<(), String> {
        let Some(marker) = &self.marker else {
            return Ok(());
        };
        if bytes == 0 || self.written.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        write_pid_marker(marker)
    }
}
