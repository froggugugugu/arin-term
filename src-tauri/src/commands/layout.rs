//! ペインの寸法のコマンド(FR-009、計画書 T33d)と、右ペインの表示のコマンド(FR-048・FR-052〜054、計画書 T47b)と、
//! 文字の大きさのコマンド(ADR-013 の 2、計画書 T68c)と、透過と「既定に戻す」のコマンド(ADR-014 の 1)。
//!
//! 引数を受けて `SessionRegistry` を呼ぶだけにする。不正な値(最小を下回る・負の値)は `arin-core` の
//! `PaneSizes` の規則で直す。変わったら写しを送り、保存が予約される(0.5 秒まとめ)。
//!
//! 表示の規則(グリッド・ページ・フォーカス・拡大)は `arin-core` の `Workspace` が持つ。どのコマンドも、状態が変わったときだけ
//! 写しを送る。知らない表示モードの文字列は引数の読み取りで失敗し(`DisplayMode` に無い値)、何も変えない。
//! どれも待たない(ロックを短く取り、送るのはロックの外)ので、ほかの寸法のコマンドと同じく同期のコマンドにする。

use arin_core::layout::font_sizes::FontSizeTarget;
use arin_core::layout::theme::ThemeMode;
use arin_core::layout::tiles::DisplayMode;
use arin_core::layout::translucency::TranslucencyTarget;
use arin_core::terminal::TerminalId;
use tauri::State;

use crate::ThemeApplier;
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

/// 文字の大きさを `delta` だけ動かす(設定画面のステッパー。ADR-013 の 2。計画書 T68c)。`target` は `terminal` / `interface`。
/// 範囲の端で留め、変わったら写しを送り、保存が予約される(状態ファイルの `ui.terminalFontSize`・`ui.interfaceFontSize`)。
/// 知らない対象の文字列は引数の読み取りで失敗し、何も変えない。
#[tauri::command]
pub fn step_font_size(sessions: State<'_, SessionRegistry>, target: FontSizeTarget, delta: i32) {
    sessions.step_font_size(target, delta);
}

/// 透過を `delta`(% の差分。▲ は +5、▼ は −5、Home・End は範囲の幅ぶん)だけ動かす(設定画面のステッパー。ADR-014 の 1)。
/// `target` は `window` / `terminal`。近い 5 の倍数に丸めて範囲の端で留め、変わったら写しを送り、保存が予約される
/// (状態ファイルの `ui.windowTranslucency`・`ui.terminalTranslucency`)。知らない対象の文字列は引数の読み取りで失敗し、何も変えない。
#[tauri::command]
pub fn step_translucency(
    sessions: State<'_, SessionRegistry>,
    target: TranslucencyTarget,
    delta: i32,
) {
    sessions.step_translucency(target, delta);
}

/// テーマの選び方(システム・ライト・ダーク)を替える(設定画面の「外観」。ADR-016)。変わったら写しを送り、保存が予約され
/// (状態ファイルの `ui.theme`)、アプリの外観に当てる。知らない文字列は引数の読み取りで失敗し、何も変えない。
#[tauri::command]
pub fn set_theme(
    sessions: State<'_, SessionRegistry>,
    applier: State<'_, ThemeApplier>,
    theme: ThemeMode,
) {
    if sessions.set_theme(theme) {
        applier.apply(theme);
    }
}

/// 選択して真ん中クリックで貼り付けるかを切り替える(設定画面。ADR-020)。変わったら写しを送り、保存が予約される。
#[tauri::command]
pub fn set_middle_click_paste(sessions: State<'_, SessionRegistry>, enabled: bool) {
    sessions.set_middle_click_paste(enabled);
}

/// 文字の大きさ 2 つと透過 2 つを既定に戻す(設定画面の「既定に戻す」。ADR-014 の 1・AC-G05。ADR-013 の
/// `reset_font_sizes` を替えたもの)。変わったら写しを送る。
#[tauri::command]
pub fn reset_appearance(sessions: State<'_, SessionRegistry>) {
    sessions.reset_appearance();
}

/// タイルにフォーカスする(タイルのクリック。FR-053、AC-M15)。そのタブのプロジェクトを選び、そのタブを表示中にする。
/// プロジェクトが替わると、ツリーの監視は写しで張り替わる。閉じたタイルの ID なら何もしない(失敗にしない。
/// M4 のレビューの S6。`SessionRegistry::focus_terminal`)。
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

/// タイルを拡大する、または拡大を解く(FR-054)。タブ表示では何もしない。
///
/// `terminal_id` を渡したら(タイトルバーのダブルクリック)、そのタイルにフォーカスしてから拡大する(1 回のロックの中。
/// M4 のレビューの S3)。閉じたタイルの ID なら何もしない(失敗にしない。S6)。渡さなければフォーカス中のタイル
/// (拡大中の帯の「戻す」)。
#[tauri::command]
pub fn toggle_tile_maximize(
    sessions: State<'_, SessionRegistry>,
    terminal_id: Option<TerminalId>,
) -> Result<(), String> {
    match terminal_id {
        Some(id) => sessions
            .toggle_tile_maximize_of(&id)
            .map(|_| ())
            .map_err(|e| e.to_string()),
        None => {
            sessions.toggle_tile_maximize();
            Ok(())
        }
    }
}
