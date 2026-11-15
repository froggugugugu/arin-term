//! ペインの寸法のコマンド(FR-009、計画書 T33d)。
//!
//! 引数を受けて `SessionRegistry` を呼ぶだけにする。不正な値(最小を下回る・負の値)は `arin-core` の
//! `PaneSizes` の規則で直す。変わったら写しを送り、保存が予約される(0.5 秒まとめ)。

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
