//! 起動時の指定(画面の描画方式、全角幅の方式、9 枚の検証画面)の解釈(計画書 T21・T22、設計書 §2.2)。
//!
//! **M1 の検証用。** 人間が DOM 描画と WebGL 描画、`unicode11` と `graphemes` を実機で比べて決め、
//! 9 枚同時表示と非表示のインスタンスを確かめる(T26 の Go/No-Go)ために、起動時の環境変数で切り替えられるようにする。
//! 比べるのは配布用の `.app` なので、リリースビルドでも有効にする。
//! T27 で描画方式と全角幅を決まった方式に固定して外し、9 枚の検証画面の指定は M4(T49)のタイル表示で外す前提。
//!
//! 状態の写し(`AppSnapshot`)には混ぜない。写しは保存する状態の正本で、開発用の指定を入れないため、
//! 画面は起動時に 1 回だけ別のコマンド(`get_launch_options`)で受け取る。
//!
//! 環境変数を読むのは `src-tauri/src/launch.rs`。ここは読んだ値を引数で受け、列挙に直すだけ(入出力をしない)。
//! 項目を足すとき(T22 の検証画面の指定など)は、`LaunchOptions` に項目を、`LaunchOptionInputs` に値を足し、
//! `LaunchOptions::resolve` で `parse_choice` を 1 行足す。

use std::ffi::OsStr;
use std::fmt;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 描画方式を選ぶ環境変数。
pub const ENV_RENDERER: &str = "ARIN_RENDERER";
/// 全角幅の方式を選ぶ環境変数。
pub const ENV_UNICODE: &str = "ARIN_UNICODE";
/// 9 枚の検証画面(T22)を有効にする環境変数。`1` で有効、`0` か指定なしで無効。
///
/// M1 の検証用(9 枚同時表示と非表示のインスタンスの確認)。リリースビルドでも有効。T27 / M4(T49)で外す前提。
pub const ENV_VERIFY_GRID: &str = "ARIN_VERIFY_GRID";

/// 9 枚の検証画面で持つターミナルの数(3 × 3)。
pub const VERIFY_GRID_TERMINAL_COUNT: usize = 9;

/// ターミナルの描画方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum RendererKind {
    /// xterm.js の既定の DOM 描画。
    #[default]
    Dom,
    /// `@xterm/addon-webgl` による WebGL2 の描画。作れないときは画面が DOM に戻して報告する。
    Webgl,
}

/// 全角幅と絵文字の幅の決め方。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum UnicodeWidth {
    /// Unicode 11 の幅の表(`@xterm/addon-unicode11`)。
    #[default]
    Unicode11,
    /// Unicode 15 の幅と書記素クラスタ(`@xterm/addon-unicode-graphemes`)。
    Graphemes,
}

/// 起動時の指定。画面が起動時に 1 回だけ受け取る。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LaunchOptions {
    /// 描画方式(`ARIN_RENDERER`)。
    pub renderer: RendererKind,
    /// 全角幅の方式(`ARIN_UNICODE`)。
    pub unicode: UnicodeWidth,
    /// 9 枚の検証画面(`ARIN_VERIFY_GRID`)。M1 の検証用で、M4(T49)で外す。
    pub verify_grid: bool,
}

/// 環境変数から読んだままの値。無い・空なら既定になる。
#[derive(Debug, Clone, Copy, Default)]
pub struct LaunchOptionInputs<'a> {
    /// `ARIN_RENDERER` の値。
    pub renderer: Option<&'a OsStr>,
    /// `ARIN_UNICODE` の値。
    pub unicode: Option<&'a OsStr>,
    /// `ARIN_VERIFY_GRID` の値。
    pub verify_grid: Option<&'a OsStr>,
}

/// 使えない値が指定されたこと。既定に戻して起動し、標準エラーに 1 行で出す。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidLaunchOption {
    /// 環境変数の名前。
    pub name: &'static str,
    /// 指定された値(UTF-8 でない部分は置き換えてある)。
    pub value: String,
    /// 使える値。
    pub allowed: &'static [&'static str],
    /// 代わりに使う既定の値。
    pub fallback: &'static str,
}

impl fmt::Display for InvalidLaunchOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} の値 {:?} は使えない(使える値: {})。既定の {} で起動する",
            self.name,
            self.value,
            self.allowed.join("・"),
            self.fallback
        )
    }
}

/// 環境変数の値から選ぶ列挙。
trait Choice: Copy + Default + PartialEq + 'static {
    /// 使える値の名前。小文字で、画面へ渡す文字列(serde の名前)と同じ。`VALUES` と同じ順。
    const NAMES: &'static [&'static str];
    /// 名前に対応する値。
    const VALUES: &'static [Self];
}

impl Choice for RendererKind {
    const NAMES: &'static [&'static str] = &["dom", "webgl"];
    const VALUES: &'static [Self] = &[Self::Dom, Self::Webgl];
}

impl Choice for UnicodeWidth {
    const NAMES: &'static [&'static str] = &["unicode11", "graphemes"];
    const VALUES: &'static [Self] = &[Self::Unicode11, Self::Graphemes];
}

/// 有効・無効の指定。`ARIN_SMOKE` と同じく `1` で有効にする。
impl Choice for bool {
    const NAMES: &'static [&'static str] = &["0", "1"];
    const VALUES: &'static [Self] = &[false, true];
}

/// 値を列挙に直す。無い・空なら既定。大文字・小文字は区別しない。前後の空白は許さない(使えない値になる)。
/// 使えない値なら既定にして、その旨を `invalid` に足す。
fn parse_choice<T: Choice>(
    name: &'static str,
    value: Option<&OsStr>,
    invalid: &mut Vec<InvalidLaunchOption>,
) -> T {
    let Some(value) = value.filter(|v| !v.is_empty()) else {
        return T::default();
    };
    let index = value
        .to_str()
        .and_then(|text| T::NAMES.iter().position(|n| n.eq_ignore_ascii_case(text)));
    if let Some(&found) = index.and_then(|i| T::VALUES.get(i)) {
        return found;
    }
    let fallback = T::default();
    let fallback_name = T::VALUES
        .iter()
        .position(|v| *v == fallback)
        .and_then(|i| T::NAMES.get(i).copied())
        .unwrap_or("?");
    invalid.push(InvalidLaunchOption {
        name,
        value: value.to_string_lossy().into_owned(),
        allowed: T::NAMES,
        fallback: fallback_name,
    });
    fallback
}

impl LaunchOptions {
    /// 読んだ値から指定を決める。使えない値は既定に戻し、その一覧を一緒に返す(呼び出し側が標準エラーに出す)。
    #[must_use]
    pub fn resolve(inputs: LaunchOptionInputs<'_>) -> (Self, Vec<InvalidLaunchOption>) {
        let mut invalid = Vec::new();
        let options = Self {
            renderer: parse_choice(ENV_RENDERER, inputs.renderer, &mut invalid),
            unicode: parse_choice(ENV_UNICODE, inputs.unicode, &mut invalid),
            verify_grid: parse_choice(ENV_VERIFY_GRID, inputs.verify_grid, &mut invalid),
        };
        (options, invalid)
    }

    /// 起動時に持つターミナルの数。9 枚の検証画面なら `VERIFY_GRID_TERMINAL_COUNT`、そうでなければ 1。
    #[must_use]
    pub fn terminal_count(&self) -> usize {
        if self.verify_grid {
            VERIFY_GRID_TERMINAL_COUNT
        } else {
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve(renderer: Option<&str>, unicode: Option<&str>) -> (LaunchOptions, Vec<String>) {
        resolve_all(renderer, unicode, None)
    }

    fn resolve_all(
        renderer: Option<&str>,
        unicode: Option<&str>,
        verify_grid: Option<&str>,
    ) -> (LaunchOptions, Vec<String>) {
        let (options, invalid) = LaunchOptions::resolve(LaunchOptionInputs {
            renderer: renderer.map(OsStr::new),
            unicode: unicode.map(OsStr::new),
            verify_grid: verify_grid.map(OsStr::new),
        });
        (options, invalid.iter().map(ToString::to_string).collect())
    }

    fn verify_grid(value: Option<&str>) -> (bool, Vec<String>) {
        let (options, invalid) = resolve_all(None, None, value);
        (options.verify_grid, invalid)
    }

    #[test]
    fn 指定が無ければ_dom_と_unicode11_で検証画面は無効() {
        assert_eq!(
            resolve(None, None),
            (
                LaunchOptions {
                    renderer: RendererKind::Dom,
                    unicode: UnicodeWidth::Unicode11,
                    verify_grid: false,
                },
                vec![]
            )
        );
    }

    #[test]
    fn 空文字列は指定なしと同じで報告しない() {
        assert_eq!(
            resolve_all(Some(""), Some(""), Some("")),
            (LaunchOptions::default(), vec![])
        );
    }

    #[test]
    fn 検証画面は_1_で有効_0_で無効() {
        assert_eq!(verify_grid(Some("1")), (true, vec![]));
        assert_eq!(verify_grid(Some("0")), (false, vec![]));
        assert_eq!(verify_grid(None), (false, vec![]));
    }

    #[test]
    fn 検証画面の使えない値は無効に戻して報告する() {
        for value in ["yes", "true", " 1", "2"] {
            let (enabled, invalid) = verify_grid(Some(value));
            assert!(!enabled, "ARIN_VERIFY_GRID={value:?} で有効になった");
            assert_eq!(
                invalid,
                vec![format!(
                    "ARIN_VERIFY_GRID の値 {value:?} は使えない(使える値: 0・1)。既定の 0 で起動する"
                )]
            );
        }
    }

    #[test]
    fn ターミナルの数は検証画面なら_9_枚_そうでなければ_1_枚() {
        let mut options = LaunchOptions::default();
        assert_eq!(options.terminal_count(), 1);
        options.verify_grid = true;
        assert_eq!(options.terminal_count(), 9);
        assert_eq!(VERIFY_GRID_TERMINAL_COUNT, 9);
    }

    #[test]
    fn 使える値をそのまま選ぶ() {
        assert_eq!(resolve(Some("dom"), None).0.renderer, RendererKind::Dom);
        assert_eq!(resolve(Some("webgl"), None).0.renderer, RendererKind::Webgl);
        assert_eq!(
            resolve(None, Some("unicode11")).0.unicode,
            UnicodeWidth::Unicode11
        );
        assert_eq!(
            resolve(None, Some("graphemes")).0.unicode,
            UnicodeWidth::Graphemes
        );
    }

    #[test]
    fn 大文字小文字は区別しない() {
        let (options, invalid) = resolve(Some("WebGL"), Some("GRAPHEMES"));
        assert_eq!(options.renderer, RendererKind::Webgl);
        assert_eq!(options.unicode, UnicodeWidth::Graphemes);
        assert_eq!(invalid, Vec::<String>::new());
    }

    #[test]
    fn 使えない値は既定に戻して_1_件ずつ報告する() {
        let (options, invalid) = resolve(Some("canvas"), Some(" graphemes"));
        assert_eq!(options, LaunchOptions::default());
        assert_eq!(
            invalid,
            vec![
                "ARIN_RENDERER の値 \"canvas\" は使えない(使える値: dom・webgl)。既定の dom で起動する"
                    .to_owned(),
                "ARIN_UNICODE の値 \" graphemes\" は使えない(使える値: unicode11・graphemes)。既定の unicode11 で起動する"
                    .to_owned(),
            ]
        );
    }

    #[test]
    fn 片方だけ使えなければ_もう片方は指定どおり() {
        let (options, invalid) = resolve(Some("webgl"), Some("unicode15"));
        assert_eq!(options.renderer, RendererKind::Webgl);
        assert_eq!(options.unicode, UnicodeWidth::Unicode11);
        assert_eq!(invalid.len(), 1);
    }

    #[test]
    fn utf_8_でない値は使えない値として扱う() {
        use std::os::unix::ffi::OsStrExt;

        let (options, invalid) = LaunchOptions::resolve(LaunchOptionInputs {
            renderer: Some(OsStr::from_bytes(b"web\xffgl")),
            unicode: None,
            verify_grid: None,
        });
        assert_eq!(options.renderer, RendererKind::Dom);
        assert_eq!(invalid.len(), 1);
        assert_eq!(invalid[0].value, "web\u{fffd}gl");
    }

    #[test]
    fn 画面へは小文字の名前で渡る() {
        let json = serde_json::to_string(&LaunchOptions {
            renderer: RendererKind::Webgl,
            unicode: UnicodeWidth::Graphemes,
            verify_grid: true,
        })
        .expect("JSON にできるはず");
        assert_eq!(
            json,
            r#"{"renderer":"webgl","unicode":"graphemes","verifyGrid":true}"#
        );
    }
}
