//! 文字の大きさの規則(ADR-013 の 1・2、計画書 T68a)。
//!
//! ターミナルと画面の 2 つの大きさを、CSS ピクセルの整数で持つ。`state.json` の `ui.terminalFontSize`・
//! `ui.interfaceFontSize` に保存する(読み書きは `persistence::codec`)。入出力はしない。
//!
//! - 範囲はターミナル 9〜28・画面 11〜18、1 刻み(【要確認】1 の推奨)。刻みは範囲の端で留める
//! - 既定はターミナル 15(xterm.js 6.0.0 の既定と同じ)・画面 13(`tokens.css` の `--font-size-l` と同じ)
//! - 保存値を読むときは、無い・`null`・数でない・有限でないなら既定、整数でない数は四捨五入(.5 は大きいほう)、
//!   範囲の外は近いほうの端に留める(どれも退避しない。ADR-013 の 1)

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

/// ターミナルの文字の大きさの最小。
pub const MIN_TERMINAL_FONT_SIZE: u16 = 9;
/// ターミナルの文字の大きさの最大。
pub const MAX_TERMINAL_FONT_SIZE: u16 = 28;
/// ターミナルの文字の大きさの既定(xterm.js 6.0.0 の既定と同じ)。
pub const DEFAULT_TERMINAL_FONT_SIZE: u16 = 15;
/// 画面の文字の大きさ(基準値)の最小。
pub const MIN_INTERFACE_FONT_SIZE: u16 = 11;
/// 画面の文字の大きさ(基準値)の最大。
pub const MAX_INTERFACE_FONT_SIZE: u16 = 18;
/// 画面の文字の大きさ(基準値)の既定(`tokens.css` の `--font-size-l` と同じ)。
pub const DEFAULT_INTERFACE_FONT_SIZE: u16 = 13;

/// 文字の大きさを変える対象(コマンド `step_font_size` の引数)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum FontSizeTarget {
    /// すべてのターミナル(プロジェクトごと・タブごとには持たない)。
    Terminal,
    /// 画面(サイドバー・タブ・ツリー・メニューなど)の文字の基準値。
    Interface,
}

impl FontSizeTarget {
    /// 範囲の最小。
    #[must_use]
    pub const fn min(self) -> u16 {
        match self {
            Self::Terminal => MIN_TERMINAL_FONT_SIZE,
            Self::Interface => MIN_INTERFACE_FONT_SIZE,
        }
    }

    /// 範囲の最大。
    #[must_use]
    pub const fn max(self) -> u16 {
        match self {
            Self::Terminal => MAX_TERMINAL_FONT_SIZE,
            Self::Interface => MAX_INTERFACE_FONT_SIZE,
        }
    }

    /// 既定の値。
    #[must_use]
    pub const fn default_value(self) -> u16 {
        match self {
            Self::Terminal => DEFAULT_TERMINAL_FONT_SIZE,
            Self::Interface => DEFAULT_INTERFACE_FONT_SIZE,
        }
    }

    /// 保存値(`state.json` の JSON の値。キーが無ければ `None`)を、この対象の大きさにする。
    /// 無い・`null`・数でない・有限でないなら既定、整数でない数は四捨五入、範囲の外は端に留める。
    #[must_use]
    pub fn from_persisted(self, raw: Option<&Value>) -> u16 {
        let Some(number) = raw.and_then(Value::as_f64).filter(|n| n.is_finite()) else {
            return self.default_value();
        };
        // f64 → i64 の `as` は範囲の外を i64 の端に留める(有限の値だけが来る)ので、そのあと範囲に留めればよい
        self.clamp(number.round() as i64)
    }

    /// 整数を範囲に留める。
    fn clamp(self, value: i64) -> u16 {
        let clamped = value.clamp(i64::from(self.min()), i64::from(self.max()));
        // 範囲の端は u16 なので失敗しない
        u16::try_from(clamped).unwrap_or(self.default_value())
    }
}

/// ターミナルと画面の文字の大きさ(CSS ピクセル)。値はいつも範囲の中にある。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontSizes {
    /// ターミナルの文字の大きさ。
    pub terminal: u16,
    /// 画面の文字の大きさの基準値。
    pub interface: u16,
}

impl Default for FontSizes {
    fn default() -> Self {
        Self {
            terminal: DEFAULT_TERMINAL_FONT_SIZE,
            interface: DEFAULT_INTERFACE_FONT_SIZE,
        }
    }
}

impl FontSizes {
    /// 対象の大きさ。
    #[must_use]
    pub const fn get(self, target: FontSizeTarget) -> u16 {
        match target {
            FontSizeTarget::Terminal => self.terminal,
            FontSizeTarget::Interface => self.interface,
        }
    }

    /// 対象の大きさを `value` にしたもの(範囲の外は端に留める)。
    #[must_use]
    pub fn with(self, target: FontSizeTarget, value: u16) -> Self {
        let value = target.clamp(i64::from(value));
        match target {
            FontSizeTarget::Terminal => Self {
                terminal: value,
                ..self
            },
            FontSizeTarget::Interface => Self {
                interface: value,
                ..self
            },
        }
    }

    /// 対象の大きさを `delta` だけ動かしたもの(範囲の端で留める)。
    #[must_use]
    pub fn stepped(self, target: FontSizeTarget, delta: i32) -> Self {
        let next = i64::from(self.get(target)) + i64::from(delta);
        self.with(target, target.clamp(next))
    }

    /// 両方を既定に戻したもの。
    #[must_use]
    pub fn reset(self) -> Self {
        Self::default()
    }

    /// 両方が既定か(既定のあいだは `state.json` に書かない。「既定に戻す」を無効にする)。
    #[must_use]
    pub fn is_default(self) -> bool {
        self == Self::default()
    }
}
