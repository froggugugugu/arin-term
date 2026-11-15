//! 起動まわり(設計書 §4 の `lifecycle.rs`)。
//!
//! 画面からの準備完了の通知を受けたときの処理と、ウィンドウが前面でなくなったときの保存、Finder からのドロップでの登録、
//! アプリの終了時の後片付けを置く。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, DragDropEvent, Manager, RunEvent, Runtime, WindowEvent};

use crate::persistence::Persistence;
use crate::projects::Projects;
use crate::sessions::SessionRegistry;

/// アプリのイベントループのイベントを受ける(`run` に渡す)。
///
/// - ウィンドウが前面でなくなったら、保存できていない変更を待たずに書く(設計書 §6.2「書き込みの契機」)
/// - ウィンドウに Finder から落とされたら、フォルダだけを登録する(FR-001、計画書 M2 の【仮定】7)。
///   フォルダを調べるのに待つことがあるので、メインスレッドの外で行う
/// - 終了するときは、状態を書き終えてから、全ターミナルのシェルを終わらせる(PTY を残さない)
pub fn on_run_event<R: Runtime>(app: &AppHandle<R>, event: RunEvent) {
    match event {
        RunEvent::WindowEvent {
            event: WindowEvent::Focused(focused),
            ..
        } => on_window_focus_changed(app, focused),
        RunEvent::WindowEvent {
            event: WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }),
            ..
        } => {
            let app = app.clone();
            tauri::async_runtime::spawn_blocking(move || on_paths_dropped(&app, paths));
        }
        RunEvent::Exit => {
            if let Some(persistence) = app.try_state::<Persistence>() {
                persistence.shutdown();
            }
            if let Some(sessions) = app.try_state::<SessionRegistry>() {
                sessions.shutdown();
            }
        }
        _ => {}
    }
}

/// ウィンドウが前面になった(`focused` が真)・でなくなったとき。でなくなったときだけ、待たずに保存する。
pub fn on_window_focus_changed<R: Runtime>(app: &AppHandle<R>, focused: bool) {
    if !focused && let Some(persistence) = app.try_state::<Persistence>() {
        persistence.flush();
    }
}

/// ウィンドウに落とされたパスのうち、フォルダだけを順に登録する(ファイル・無いものは無視する。FR-001、【仮定】7)。
/// 最後に登録した(または既存を選んだ)ものが選択中になる。フォルダを調べるのに待つことがあるので、メインスレッドから呼ばない。
pub fn on_paths_dropped<R: Runtime>(app: &AppHandle<R>, paths: Vec<PathBuf>) {
    if let Some(projects) = app.try_state::<Projects>() {
        projects.add_dropped(&paths);
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
