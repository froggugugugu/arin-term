//! 透過の規則(ADR-014 の 1)。
//!
//! ウィンドウ(サイドバーと帯)とターミナルの背景の 2 つの「透過の度合い(%)」を整数で持つ。`state.json` の
//! `ui.windowTranslucency`・`ui.terminalTranslucency` に保存する(読み書きは `persistence::codec`)。入出力はしない。
//!
//! - 範囲はウィンドウ 0〜100・ターミナル 0〜80、刻みは 5(上限 80 は文字が読めなくなるのを避けるため)
//! - 既定はウィンドウ 45(M4.6 までの見た目と同じ)・ターミナル 20
//! - 値はいつも 5 の倍数。5 の倍数でない値は近い 5 の倍数に丸め(ちょうど中間は大きいほう)、範囲の外は端に留める
//! - 保存値を読むときは、無い・`null`・数でない・有限でないなら既定(どれも退避しない)

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

/// ウィンドウの透過の最小(不透明)。
pub const MIN_WINDOW_TRANSLUCENCY: u8 = 0;
/// ウィンドウの透過の最大(色を重ねない。すりガラスだけ)。
pub const MAX_WINDOW_TRANSLUCENCY: u8 = 100;
/// ウィンドウの透過の既定(M4.6 までの見た目と同じ)。
pub const DEFAULT_WINDOW_TRANSLUCENCY: u8 = 45;
/// ターミナルの背景の透過の最小(不透明の黒。M4.6 までの見た目)。
pub const MIN_TERMINAL_TRANSLUCENCY: u8 = 0;
/// ターミナルの背景の透過の最大(文字が読めなくなるのを避ける)。
pub const MAX_TERMINAL_TRANSLUCENCY: u8 = 80;
/// ターミナルの背景の透過の既定。
pub const DEFAULT_TERMINAL_TRANSLUCENCY: u8 = 20;
/// 刻み(両方の対象で同じ)。値はいつもこの倍数。
pub const TRANSLUCENCY_STEP: u8 = 5;

/// 透過を変える対象(コマンド `step_translucency` の引数)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum TranslucencyTarget {
    /// ウィンドウ(サイドバーと帯)。
    Window,
    /// ターミナルの背景(すべてのターミナル)。
    Terminal,
}

impl TranslucencyTarget {
    /// 範囲の最小。
    #[must_use]
    pub const fn min(self) -> u8 {
        match self {
            Self::Window => MIN_WINDOW_TRANSLUCENCY,
            Self::Terminal => MIN_TERMINAL_TRANSLUCENCY,
        }
    }

    /// 範囲の最大。
    #[must_use]
    pub const fn max(self) -> u8 {
        match self {
            Self::Window => MAX_WINDOW_TRANSLUCENCY,
            Self::Terminal => MAX_TERMINAL_TRANSLUCENCY,
        }
    }

    /// 既定の値。
    #[must_use]
    pub const fn default_value(self) -> u8 {
        match self {
            Self::Window => DEFAULT_WINDOW_TRANSLUCENCY,
            Self::Terminal => DEFAULT_TERMINAL_TRANSLUCENCY,
        }
    }

    /// 刻み。
    #[must_use]
    pub const fn step(self) -> u8 {
        TRANSLUCENCY_STEP
    }

    /// 保存値(`state.json` の JSON の値。キーが無ければ `None`)を、この対象の値にする。
    /// 無い・`null`・数でない・有限でないなら既定、それ以外は近い 5 の倍数に丸めて端に留める。
    #[must_use]
    pub fn from_persisted(self, raw: Option<&Value>) -> u8 {
        let Some(number) = raw.and_then(Value::as_f64).filter(|n| n.is_finite()) else {
            return self.default_value();
        };
        let step = f64::from(TRANSLUCENCY_STEP);
        // f64 → i64 の `as` は範囲の外を i64 の端に留める(有限の値だけが来る)ので、そのあと範囲に留めればよい
        self.clamp(((number / step).round() * step) as i64)
    }

    /// 整数を近い 5 の倍数に丸めて範囲に留める(ちょうど中間は大きいほう。整数ではちょうど中間は起きない)。
    fn normalize(self, value: i64) -> u8 {
        let step = i64::from(TRANSLUCENCY_STEP);
        let rounded = value.saturating_add(step / 2).div_euclid(step) * step;
        self.clamp(rounded)
    }

    /// 整数を範囲に留める。
    fn clamp(self, value: i64) -> u8 {
        let clamped = value.clamp(i64::from(self.min()), i64::from(self.max()));
        // 範囲の端は u8 なので失敗しない
        u8::try_from(clamped).unwrap_or(self.default_value())
    }
}

/// ウィンドウとターミナルの背景の透過(%)。値はいつも範囲の中の 5 の倍数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Translucency {
    /// ウィンドウ(サイドバーと帯)。
    pub window: u8,
    /// ターミナルの背景。
    pub terminal: u8,
}

impl Default for Translucency {
    fn default() -> Self {
        Self {
            window: DEFAULT_WINDOW_TRANSLUCENCY,
            terminal: DEFAULT_TERMINAL_TRANSLUCENCY,
        }
    }
}

impl Translucency {
    /// 対象の値。
    #[must_use]
    pub const fn get(self, target: TranslucencyTarget) -> u8 {
        match target {
            TranslucencyTarget::Window => self.window,
            TranslucencyTarget::Terminal => self.terminal,
        }
    }

    /// 対象の値を `value` にしたもの(近い 5 の倍数に丸めて端に留める)。
    #[must_use]
    pub fn with(self, target: TranslucencyTarget, value: u8) -> Self {
        self.with_raw(target, i64::from(value))
    }

    /// 対象の値を `delta`(% の差分。刻みの数ではない)だけ動かしたもの(近い 5 の倍数に丸めて端に留める)。
    #[must_use]
    pub fn stepped(self, target: TranslucencyTarget, delta: i32) -> Self {
        self.with_raw(target, i64::from(self.get(target)) + i64::from(delta))
    }

    /// 両方を既定に戻したもの。
    #[must_use]
    pub fn reset(self) -> Self {
        Self::default()
    }

    /// 両方が既定か(既定のあいだは `state.json` に書かない)。
    #[must_use]
    pub fn is_default(self) -> bool {
        self == Self::default()
    }

    fn with_raw(self, target: TranslucencyTarget, value: i64) -> Self {
        let value = target.normalize(value);
        match target {
            TranslucencyTarget::Window => Self {
                window: value,
                ..self
            },
            TranslucencyTarget::Terminal => Self {
                terminal: value,
                ..self
            },
        }
    }
}
