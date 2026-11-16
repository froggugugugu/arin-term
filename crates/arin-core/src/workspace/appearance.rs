//! `Workspace` の見た目の口: 文字の大きさ(計画書 T68b、ADR-013 の 2)と透過(ADR-014 の 1)。
//! 規則(範囲・既定・刻み・丸め)は `layout::font_sizes` と `layout::translucency`。
//!
//! コマンドは「いくつ動かすか」で受ける(画面が「今の値 + 1」を送ると、写しが届く前に続けて押したとき 1 つしか上がらない)。
//! 変わったときだけ `true` を返し、呼び出し側が写しを送る(送ると保存が予約される)。

use super::state::Workspace;
use crate::layout::font_sizes::{FontSizeTarget, FontSizes};
use crate::layout::translucency::{Translucency, TranslucencyTarget};

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

    /// いまの透過。
    #[must_use]
    pub fn translucency(&self) -> Translucency {
        self.translucency
    }

    /// 対象の透過を `delta`(% の差分。▲ は +5、▼ は −5、Home・End は範囲の幅ぶん)だけ動かす。
    /// 近い 5 の倍数に丸めて範囲の端で留める。変わったら `true`。
    pub fn step_translucency(&mut self, target: TranslucencyTarget, delta: i32) -> bool {
        let before = self.translucency;
        self.translucency = before.stepped(target, delta);
        self.translucency != before
    }

    /// 文字の大きさ 2 つと透過 2 つを既定に戻す(設定画面の「既定に戻す」。ADR-014 の 1・AC-G05)。
    /// 変わったら `true`(4 つとも既定のときは `false`)。
    pub fn reset_appearance(&mut self) -> bool {
        let before = (self.font_sizes, self.translucency);
        self.font_sizes = self.font_sizes.reset();
        self.translucency = self.translucency.reset();
        (self.font_sizes, self.translucency) != before
    }
}
