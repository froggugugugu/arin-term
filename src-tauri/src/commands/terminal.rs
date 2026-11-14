//! ターミナルのコマンド(設計書 §5.2 の「ターミナル」のうち M1 で使う 4 つ)。
//!
//! 引数を受けて `SessionRegistry` を呼ぶだけにする。規則は `arin-core`、スレッドは `sessions.rs` が持つ。
//! 待ちうる操作(起動、書き込み、寸法の変更)は Tauri の待ってよいスレッドで行い、非同期の実行環境とメインスレッドを塞がない。
//! 失敗は日本語の文言にして画面へ返す。
//!
//! コマンドどうしは並行に動く(呼ばれるたびに別の待ってよいスレッドで行う)ので、同じターミナルへの `write_terminal`・
//! `resize_terminal` の順序は、画面がターミナルごとに前の応答を待ってから次を送ることで保つ(`SessionRegistry::write`)。

use std::sync::Arc;

use arin_core::terminal::TerminalId;
use tauri::State;
use tauri::ipc::{Channel, InvokeResponseBody};

use crate::lifecycle::FirstOutputNotice;
use crate::sessions::{OutputSink, SessionError, SessionRegistry};

/// Tauri のチャネルへ生のバイト列で送る(画面には `ArrayBuffer` で届く。ADR-004)。
struct ChannelSink(Channel<InvokeResponseBody>);

impl OutputSink for ChannelSink {
    fn send(&self, data: Vec<u8>) -> Result<(), String> {
        self.0
            .send(InvokeResponseBody::Raw(data))
            .map_err(|e| e.to_string())
    }
}

/// 待ってよいスレッドで `f` を行い、失敗を文言にする。
async fn blocking<F>(f: F) -> Result<(), String>
where
    F: FnOnce() -> Result<(), SessionError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| format!("ターミナルの操作を完了できなかった: {e}"))?
        .map_err(|e| e.to_string())
}

/// 出力のチャネルをつなぐ。最初の接続で `rows` × `cols` の PTY を開いてログインシェルを起動する。
/// 2 回目以降(画面の再読み込みなど)はチャネルを付け替え、寸法を伝える。
///
/// チャネルには、まとめた出力を生のバイト列で 1 回ずつ流す。画面は取り込みが済んだバイト数を
/// `ack_terminal_output` で返す。
#[tauri::command]
pub async fn connect_terminal(
    sessions: State<'_, SessionRegistry>,
    terminal_id: TerminalId,
    rows: u16,
    cols: u16,
    on_output: Channel<InvokeResponseBody>,
) -> Result<(), String> {
    let sessions = sessions.inner().clone();
    blocking(move || sessions.connect(&terminal_id, rows, cols, Arc::new(ChannelSink(on_output))))
        .await
}

/// キー入力・貼り付けの文字列を、そのまま PTY に書き込む。
#[tauri::command]
pub async fn write_terminal(
    sessions: State<'_, SessionRegistry>,
    terminal_id: TerminalId,
    data: String,
) -> Result<(), String> {
    let sessions = sessions.inner().clone();
    blocking(move || sessions.write(&terminal_id, data.as_bytes())).await
}

/// 画面が出力を `bytes` バイト取り込んだことを知らせる(流量制御)。
///
/// 起動スモークが有効なら、最初の通知(0 より大きい)でターミナルの印を書く。
#[tauri::command]
pub async fn ack_terminal_output(
    sessions: State<'_, SessionRegistry>,
    first_output: State<'_, FirstOutputNotice>,
    terminal_id: TerminalId,
    bytes: u64,
) -> Result<(), String> {
    sessions
        .ack(&terminal_id, bytes)
        .map_err(|e| e.to_string())?;
    first_output.on_output_acked(bytes)
}

/// 寸法(行・桁)を変える。
#[tauri::command]
pub async fn resize_terminal(
    sessions: State<'_, SessionRegistry>,
    terminal_id: TerminalId,
    rows: u16,
    cols: u16,
) -> Result<(), String> {
    let sessions = sessions.inner().clone();
    blocking(move || sessions.resize(&terminal_id, rows, cols)).await
}
