//! `Workspace` の文字の大きさの口(計画書 T68b、ADR-013 の 2)。規則(範囲・既定・刻み)は `layout::font_sizes`。
//!
//! コマンドは「いくつ動かすか」で受ける(画面が「今の値 + 1」を送ると、写しが届く前に続けて押したとき 1 つしか上がらない)。
//! 変わったときだけ `true` を返し、呼び出し側が写しを送る(送ると保存が予約される)。

use super::state::Workspace;
use crate::layout::font_sizes::{FontSizeTarget, FontSizes};

impl Workspace {
    /// いまの文字の大きさ。
    #[must_use]
    pub fn font_sizes(&self) -> FontSizes {
        self.font_sizes
    }

    /// 対象の文字の大きさを `delta` だけ動かす(範囲の端で留める)。変わったら `true`。
    pub fn step_font_size(&mut self, target: FontSizeTarget, delta: i32) -> bool {
        let before = self.font_sizes;
        self.font_sizes = before.stepped(target, delta);
        self.font_sizes != before
    }

    /// 両方の文字の大きさを既定に戻す。変わったら `true`(両方が既定のときは `false`)。
    pub fn reset_font_sizes(&mut self) -> bool {
        let before = self.font_sizes;
        self.font_sizes = before.reset();
        self.font_sizes != before
    }
}
