//! 起動まわり(設計書 §4 の `lifecycle.rs`)。
//!
//! いまは画面からの準備完了の通知を受けたときの処理だけを置く。

use std::path::PathBuf;

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
        let contents = format!("{}\n", std::process::id());
        arin_platform::storage::write_ready_marker(marker, &contents)
            .map_err(|e| format!("起動スモークの印 {} を書けなかった: {e}", marker.display()))
    }
}
