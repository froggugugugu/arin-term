//! テーマ(ライト・ダーク)の選び方(ADR-016)。
//!
//! 「システム(OS の設定に追従)・ライト・ダーク」の 3 つから選び、`state.json` の `ui.theme` に保存する
//! (読み書きは `persistence::codec`)。入出力はしない。選んだ値を macOS のアプリの外観に当てるのは `src-tauri`。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// テーマの選び方(コマンド `set_theme` の引数。写しの `theme`)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    /// OS の設定に追従する。既定。
    #[default]
    System,
    /// いつもライト。
    Light,
    /// いつもダーク。
    Dark,
}
