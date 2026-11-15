//! 起動まわり(設計書 §4 の `lifecycle.rs`)。
//!
//! 画面からの準備完了の通知を受けたときの処理と、ウィンドウが前面でなくなったときの保存、Finder からのドロップでの登録、
//! 終了の確認(FR-063)、アプリの終了時の後片付けを置く。
//!
//! 終了の流れ(計画書 T33e、設計書 §11): 終了の要求(メニューの「終了」⌘Q=`request_quit`、ウィンドウの閉じるボタン)の
//! ときに動作中のターミナル(`SessionRegistry::running_terminals`)があれば、終了を止めて画面へ `quit-requested` を送る。
//! 画面が確認したら `confirm_quit` のコマンドで `quit_now` を呼ぶ。動作中が無ければ確認せずに `quit_now` と同じく終わる。
//! 終わるときはウィンドウを閉じ(最後のウィンドウが無くなると Tauri がループを抜ける)、`RunEvent::Exit` で
//! 見回りを止め → 状態を書き → 全セッションを終わらせる。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use arin_core::workspace::QuitRequest;
use tauri::{AppHandle, DragDropEvent, Emitter, Manager, RunEvent, Runtime, WindowEvent};

use crate::events::QUIT_REQUESTED_EVENT;
use crate::patrol::PatrolThread;
use crate::persistence::Persistence;
use crate::projects::Projects;
use crate::sessions::SessionRegistry;
use crate::tree_watch::TreeWatch;

/// アプリのイベントループのイベントを受ける(`run` に渡す)。
///
/// - ウィンドウが前面でなくなったら、保存できていない変更を待たずに書く(設計書 §6.2「書き込みの契機」)
/// - ウィンドウに Finder から落とされたら、フォルダだけを登録する(FR-001、計画書 M2 の【仮定】7)。
///   フォルダを調べるのに待つことがあるので、メインスレッドの外で行う
/// - ウィンドウの閉じるボタンは終了の要求として扱う。終わってよいと決まっていなければ、動作中のターミナルがあるときだけ
///   閉じるのを止めて確認を求める(FR-063)。無ければそのまま閉じて終わる
/// - 終了するときは、ツリーの監視と、ルートの見回りのスレッドを止め、状態を書き終えてから、全ターミナルのシェルを終わらせる(PTY を残さない)
pub fn on_run_event<R: Runtime>(app: &AppHandle<R>, event: RunEvent) {
    match event {
        RunEvent::WindowEvent {
            event: WindowEvent::CloseRequested { api, .. },
            ..
        } => {
            if !quit_or_ask(app) {
                api.prevent_close();
            }
        }
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
            // ツリーの監視の張り替えのスレッドを止め、監視を捨てる(FSEvents のスレッドを残さない。T42b)。
            if let Some(watch) = app.try_state::<TreeWatch>() {
                watch.stop();
            }
            // 見回りを先に止める(止めたあとに状態が変わらず、最後の追従も保存に入る)。
            if let Some(patrol) = app.try_state::<PatrolThread>() {
                patrol.stop();
            }
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

/// 終了してよいと決まったか。Tauri の管理状態に登録する(`lib.rs`)。決まったあとの閉じる要求は止めない。
#[derive(Debug, Default)]
pub struct QuitGate {
    allowed: AtomicBool,
}

impl QuitGate {
    fn allow(&self) {
        self.allowed.store(true, Ordering::SeqCst);
    }

    fn is_allowed(&self) -> bool {
        self.allowed.load(Ordering::SeqCst)
    }
}

/// 終了の要求(メニューの「終了」⌘Q)。動作中のターミナルがあれば、終わらずに画面へ確認を求める(`quit-requested`)。
/// 無ければそのまま終わる(`quit_now`)。待たない(前面のプロセスは待たずに調べる)。
pub fn request_quit<R: Runtime>(app: &AppHandle<R>) {
    if quit_or_ask(app) {
        quit_now(app);
    }
}

/// 終わってよいかを決める。終わってよければ真(以後の閉じる要求も止めない)。動作中のターミナルがあれば、
/// 画面へ `quit-requested`(動作中の一覧)を送って偽を返す(何も変えない。取り消しは画面が何も呼ばないこと)。
fn quit_or_ask<R: Runtime>(app: &AppHandle<R>) -> bool {
    let Some(gate) = app.try_state::<QuitGate>() else {
        return true;
    };
    if gate.is_allowed() {
        return true;
    }
    let running = app
        .try_state::<SessionRegistry>()
        .map(|sessions| sessions.running_terminals())
        .unwrap_or_default();
    if running.is_empty() {
        gate.allow();
        return true;
    }
    // 画面が無いなどで送れなければ確認は出ない。そのときは閉じるボタン・⌘Q を押し直しても同じで、
    // 終わらせるにはシェルを終えてから押す(動作中のプロセスを黙って終わらせない)。
    let _ = app.emit(QUIT_REQUESTED_EVENT, QuitRequest { running });
    false
}

/// 確認なしで終わる(画面が確認した=`confirm_quit`、または動作中が無いとき)。ウィンドウを閉じると、最後のウィンドウが
/// 無くなった時点で Tauri がループを抜け、`RunEvent::Exit` で状態を書いてから全セッションを終わらせる。
pub fn quit_now<R: Runtime>(app: &AppHandle<R>) {
    if let Some(gate) = app.try_state::<QuitGate>() {
        gate.allow();
    }
    let windows = app.webview_windows();
    if windows.is_empty() {
        // ウィンドウが無いとループを抜ける契機が無いので、終了を頼む。
        app.exit(0);
        return;
    }
    for window in windows.into_values() {
        // 閉じる要求(`close`)を経由せずに破棄する(画面の閉じる要求の購読などに止められない)。
        let _ = window.destroy();
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
