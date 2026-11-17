//! プロジェクト色の識別子と割当(FR-059、設計書 §9.2、計画書 M2 の【仮定】12)。
//!
//! Rust が持つのは色の識別子だけ。実際の色の値(ライト・ダーク)は画面の `src/ui/design/palette.ts` が持つ。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// プロジェクト色の識別子。並びがパレットの順。JSON では小文字の名前(`"green"` など)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum ProjectColor {
    /// 赤。
    Red,
    /// 橙。
    Orange,
    /// 黄。
    Yellow,
    /// 緑。
    Green,
    /// 青緑。
    Teal,
    /// 青。
    Blue,
    /// 紫。
    Purple,
    /// 桃。
    Pink,
}

impl ProjectColor {
    /// パレット(8 色)。この順で割り当てる。
    pub const PALETTE: [Self; 8] = [
        Self::Red,
        Self::Orange,
        Self::Yellow,
        Self::Green,
        Self::Teal,
        Self::Blue,
        Self::Purple,
        Self::Pink,
    ];

    /// 識別子の文字列(JSON と同じ)。
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Red => "red",
            Self::Orange => "orange",
            Self::Yellow => "yellow",
            Self::Green => "green",
            Self::Teal => "teal",
            Self::Blue => "blue",
            Self::Purple => "purple",
            Self::Pink => "pink",
        }
    }

    /// パレットの中の位置。
    fn index(self) -> usize {
        self as usize
    }
}

/// 新しいプロジェクトの色を決める規則。
pub struct ColorAssigner;

impl ColorAssigner {
    /// いま使われている色(`in_use`。重複を含めて全件)から、使われている数が最も少ない色を返す。
    /// 同数ならパレットの順で先の色。結果は入力の並びによらず決まる。
    #[must_use]
    pub fn pick(in_use: impl IntoIterator<Item = ProjectColor>) -> ProjectColor {
        let mut counts = [0usize; ProjectColor::PALETTE.len()];
        for color in in_use {
            counts[color.index()] += 1;
        }
        // `min_by_key` は同じ最小が複数あると最初の要素を返す(= パレットの順)。
        ProjectColor::PALETTE
            .into_iter()
            .min_by_key(|color| counts[color.index()])
            .unwrap_or(ProjectColor::Red)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 色の識別子は_8_つでパレットの順に並ぶ() {
        let names: Vec<&str> = ProjectColor::PALETTE.iter().map(|c| c.as_str()).collect();
        assert_eq!(
            names,
            [
                "red", "orange", "yellow", "green", "teal", "blue", "purple", "pink"
            ]
        );
        for (i, color) in ProjectColor::PALETTE.iter().enumerate() {
            assert_eq!(color.index(), i);
        }
    }

    #[test]
    fn 色の識別子の_json_は小文字の名前() {
        for color in ProjectColor::PALETTE {
            let json = serde_json::to_string(&color).expect("JSON にできる");
            assert_eq!(json, format!("\"{}\"", color.as_str()));
            assert_eq!(
                serde_json::from_str::<ProjectColor>(&json).ok(),
                Some(color)
            );
        }
        assert!(serde_json::from_str::<ProjectColor>("\"Green\"").is_err());
    }

    #[test]
    fn 何も使われていなければパレットの最初() {
        assert_eq!(ColorAssigner::pick([]), ProjectColor::Red);
    }

    #[test]
    fn 使われている数が最も少ない色_同数ならパレットの順() {
        use ProjectColor::{Blue, Green, Orange, Red, Yellow};

        assert_eq!(ColorAssigner::pick([Red, Orange]), Yellow);
        // 入力の並びによらない
        assert_eq!(ColorAssigner::pick([Orange, Red]), Yellow);
        // 全色 1 回 + 赤がもう 1 回 → 1 回の色のうち最初の橙
        let mut used = ProjectColor::PALETTE.to_vec();
        used.push(Red);
        assert_eq!(ColorAssigner::pick(used), Orange);
        // 空いた色があればそれ
        let without_green: Vec<_> = ProjectColor::PALETTE
            .into_iter()
            .filter(|c| *c != Green)
            .chain([Blue, Blue])
            .collect();
        assert_eq!(ColorAssigner::pick(without_green), Green);
    }
}
