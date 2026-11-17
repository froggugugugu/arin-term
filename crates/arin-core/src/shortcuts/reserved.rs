//! 予約のキー(ADR-012 の 3 の #6。計画書 T64a)。
//!
//! OS やアプリの標準の項目が、メニューのアクセラレータより先に取るキー。設定ファイルで割り当てても効かないので、
//! その行を既定に戻す。一覧は手で持つ(本人が足し引きできる)。「システム設定」で足した OS のショートカットは検査できない。
//! `Cmd+Q`(終了)は予約にしない: `app.quit` の既定のキーで、表の重なりの検査で守られる。

use super::key::KeyChord;

/// 予約のキー(正規の書式)。
pub const RESERVED_KEYS: [&str; 18] = [
    // アプリ名のメニューの標準の項目(隠す・ほかを隠す)。
    "Cmd+H",
    "Cmd+Alt+H",
    // アプリの切り替えと入力。
    "Cmd+Tab",
    "Cmd+Shift+Tab",
    "Cmd+Space",
    "Ctrl+Space",
    "Cmd+Ctrl+Space",
    "Cmd+Alt+Space",
    // 画面の取り込み・ロック・強制終了。
    "Cmd+Shift+3",
    "Cmd+Shift+4",
    "Cmd+Shift+5",
    "Cmd+Ctrl+Q",
    "Cmd+Alt+Escape",
    // ウィンドウと操作スペース。
    "Cmd+`",
    "Ctrl+ArrowUp",
    "Ctrl+ArrowDown",
    "Ctrl+ArrowLeft",
    "Ctrl+ArrowRight",
];

/// 予約のキーか(書き方によらず、同じキーなら予約)。
#[must_use]
pub fn is_reserved(chord: KeyChord) -> bool {
    RESERVED_KEYS
        .iter()
        .any(|key| KeyChord::parse(key).is_ok_and(|reserved| reserved == chord))
}
