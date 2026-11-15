//! ペインの寸法のコマンド(FR-009、計画書 T33d)と、右ペインの表示のコマンド(FR-048・FR-052〜054、計画書 T47b)。
//!
//! 引数を受けて `SessionRegistry` を呼ぶだけにする。不正な値(最小を下回る・負の値)は `arin-core` の
//! `PaneSizes` の規則で直す。変わったら写しを送り、保存が予約される(0.5 秒まとめ)。
//!
//! 表示の規則(グリッド・ページ・フォーカス・拡大)は `arin-core` の `Workspace` が持つ。どのコマンドも、状態が変わったときだけ
//! 写しを送る。知らない表示モードの文字列は引数の読み取りで失敗し(`DisplayMode` に無い値)、何も変えない。
//! どれも待たない(ロックを短く取り、送るのはロックの外)ので、ほかの寸法のコマンドと同じく同期のコマンドにする。

use arin_core::layout::tiles::DisplayMode;
use arin_core::terminal::TerminalId;
use tauri::State;

use crate::sessions::SessionRegistry;

/// 左ペインの幅とカード領域(左ペインの上段)の高さを受け取る(単位は CSS ピクセル)。
/// 画面は境界のドラッグを終えたときに送る。直したあとの値は写しで届く。
#[tauri::command]
pub fn set_pane_sizes(
    sessions: State<'_, SessionRegistry>,
    sidebar_width: f64,
    card_area_height: f64,
) {
    sessions.set_pane_sizes(sidebar_width, card_area_height);
}

/// 左ペインを折りたたむ・戻す。幅は覚えておく(戻すと同じ幅)。
#[tauri::command]
pub fn set_sidebar_collapsed(sessions: State<'_, SessionRegistry>, collapsed: bool) {
    sessions.set_sidebar_collapsed(collapsed);
}

/// 右ペインの表示モード(`tabs` / `tiles`)を変える(FR-048)。変わったら写しを送り、保存が予約される
/// (状態ファイルの `ui.displayMode`)。タブ表示にすると拡大は解ける。
#[tauri::command]
pub fn set_display_mode(sessions: State<'_, SessionRegistry>, mode: DisplayMode) {
    sessions.set_display_mode(mode);
}

/// タイルにフォーカスする(タイルのクリック。FR-053、AC-M15)。そのタブのプロジェクトを選び、そのタブを表示中にする。
/// プロジェクトが替わると、ツリーの監視は写しで張り替わる。
#[tauri::command]
pub fn focus_terminal(
    sessions: State<'_, SessionRegistry>,
    terminal_id: TerminalId,
) -> Result<(), String> {
    sessions
        .focus_terminal(&terminal_id)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// 次のページへ送り、その先頭のタイルにフォーカスする(FR-052)。タブ表示・拡大中・最後のページでは何もしない。
#[tauri::command]
pub fn next_tile_page(sessions: State<'_, SessionRegistry>) {
    sessions.next_tile_page();
}

/// 前のページへ送り、その先頭のタイルにフォーカスする(FR-052)。タブ表示・拡大中・最初のページでは何もしない。
#[tauri::command]
pub fn previous_tile_page(sessions: State<'_, SessionRegistry>) {
    sessions.previous_tile_page();
}

/// フォーカス中のタイルを拡大する、または拡大を解く(FR-054)。タブ表示では何もしない。
#[tauri::command]
pub fn toggle_tile_maximize(sessions: State<'_, SessionRegistry>) {
    sessions.toggle_tile_maximize();
}
