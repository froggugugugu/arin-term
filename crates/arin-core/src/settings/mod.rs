//! 設定ファイル `settings.toml` の読み込み結果と、使っている内容(設計書 §5.1 `arin-core/settings`、§6.1 `SettingsStore`、
//! §7.2「設定ファイル」、ADR-012 の 1・3・7、計画書 T64b)。入出力はしない。
//!
//! - [`load`] は読んだ結果の型([`SettingsLoad`])。TOML の解析は `arin-platform`(T65a)が行い、解析済みの形で渡す
//! - [`SettingsStore`] は使っている内容([`Settings`])と、いまの問題([`SettingsProblem`])。設定ファイル自体が正本で、
//!   ここからは書き戻さない
//!
//! M4.5 で使う節は `[shortcuts]` だけで、ほかの節は持たない(黙って無視する。M6 の T56 が節を足す)。
//! ショートカットの規則(行ごとの検査)は `shortcuts` が持つ。ここは `shortcuts` の型に依存しない(変換は T65b)。
//!
//! ファイル全体の問題(構文エラー・読めない)のときは、直前の内容を保つ(起動時の「直前」は空=既定)。
//! ファイルが消えたら空に戻る。知らせに出すのは行と列・理由の種類・節の名前だけで、中身とパスは出さない。

pub mod load;

use std::fmt;

pub use load::{
    SectionBody, SettingEntry, SettingValue, SettingsDocument, SettingsLoad, SettingsSection,
    SettingsUnreadable, TextPosition, decode_contents,
};

/// ショートカットの節の名前(ADR-012 の 2)。
pub const SHORTCUTS_SECTION: &str = "shortcuts";

/// 使っている設定の内容。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Settings {
    shortcuts: Vec<SettingEntry>,
}

impl Settings {
    /// 読めた設定ファイルから、使う節だけを取り出す。`[shortcuts]` が表でなければその節を空にして問題を返す。
    fn from_document(document: SettingsDocument) -> (Self, Option<SettingsProblem>) {
        let mut settings = Self::default();
        let mut problem = None;
        if let Some(section) = document
            .sections
            .into_iter()
            .find(|section| section.name == SHORTCUTS_SECTION)
        {
            match section.body {
                SectionBody::Table(entries) => settings.shortcuts = entries,
                SectionBody::NotTable => {
                    problem = Some(SettingsProblem::SectionNotTable(SHORTCUTS_SECTION));
                }
            }
        }
        (settings, problem)
    }

    /// `[shortcuts]` の行(書いた順)。書かれていなければ空。
    #[must_use]
    pub fn shortcuts(&self) -> &[SettingEntry] {
        &self.shortcuts
    }
}

/// 設定ファイルの問題(ファイル全体の問題か、節の問題)。中身の文字列は持たない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsProblem {
    /// TOML の構文エラー。
    Syntax(TextPosition),
    /// ファイルとして読めない。
    Unreadable(SettingsUnreadable),
    /// 節が表でない(節の名前)。その節だけ空にした。
    SectionNotTable(&'static str),
}

impl SettingsProblem {
    /// 標準エラー用の 1 行(頭の `[arin:settings]` は出す側が付ける)。
    #[must_use]
    pub fn diagnostic_line(&self) -> String {
        format!("設定ファイル: {self}")
    }

    const fn is_whole_file(self) -> bool {
        matches!(self, Self::Syntax(_) | Self::Unreadable(_))
    }
}

impl fmt::Display for SettingsProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax(p) => write!(f, "{} 行 {} 列目を TOML として読めない", p.line, p.column),
            Self::Unreadable(reason) => write!(f, "読めない({reason})"),
            Self::SectionNotTable(name) => write!(f, "[{name}] が表ではない"),
        }
    }
}

/// 使っている設定の内容と、いまの問題(設計書 §6.1)。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SettingsStore {
    settings: Settings,
    /// いまの内容が、読めた設定ファイルから来たか(ファイル全体の問題で「直前のまま」か「既定のまま」かを分ける)。
    from_file: bool,
    problem: Option<SettingsProblem>,
}

impl SettingsStore {
    /// 読んだ結果を当てる。起動時も読み直しも同じ口を使う(起動時は空の内容に当てる)。
    ///
    /// - 無い: 空に戻す。問題は無い
    /// - 読めた: 新しい内容にする。`[shortcuts]` が表でなければその節を空にして問題に残す
    /// - 構文エラー・読めない: 内容は直前のまま。問題に残す
    ///
    /// 内容か問題が変わったら `true` を返す。
    pub fn apply(&mut self, load: SettingsLoad) -> bool {
        let before = self.clone();
        match load {
            SettingsLoad::Missing => {
                self.settings = Settings::default();
                self.from_file = false;
                self.problem = None;
            }
            SettingsLoad::Loaded(document) => {
                let (settings, problem) = Settings::from_document(document);
                self.settings = settings;
                self.from_file = true;
                self.problem = problem;
            }
            SettingsLoad::Syntax(position) => {
                self.problem = Some(SettingsProblem::Syntax(position))
            }
            SettingsLoad::Unreadable(reason) => {
                self.problem = Some(SettingsProblem::Unreadable(reason));
            }
        }
        *self != before
    }

    /// 使っている内容。
    #[must_use]
    pub const fn settings(&self) -> &Settings {
        &self.settings
    }

    /// いまの問題。無ければ `None`。
    #[must_use]
    pub const fn problem(&self) -> Option<&SettingsProblem> {
        self.problem.as_ref()
    }

    /// 画面に出す知らせの 1 行。問題が無ければ `None`。
    #[must_use]
    pub fn notice_line(&self) -> Option<String> {
        let problem = self.problem?;
        let kept = if problem.is_whole_file() && self.from_file {
            "設定は直前のままです"
        } else {
            "設定は既定のままです"
        };
        Some(match problem {
            SettingsProblem::Syntax(p) => format!(
                "設定ファイルの {} 行 {} 列目を読めないため、{kept}",
                p.line, p.column
            ),
            SettingsProblem::Unreadable(reason) => {
                format!("設定ファイルを読めないため({reason})、{kept}")
            }
            SettingsProblem::SectionNotTable(name) => {
                format!("設定ファイルの [{name}] が表ではないため、その節は既定のままです")
            }
        })
    }
}
