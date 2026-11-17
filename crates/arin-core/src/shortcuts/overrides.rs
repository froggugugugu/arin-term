//! 設定ファイルの値をショートカットの表に当てる規則と、問題の一覧・知らせ(計画書 T64a。ADR-012 の 2・3)。
//!
//! 入力は「コマンドの ID の文字列 → 値」の並び([`ShortcutOverride`])だけで、設定ファイルの型(`settings`)に依存しない
//! (TOML の値からの変換は T65b)。入出力・時計を持たない純粋な関数。
//!
//! 規則(ADR-012 の 3 の表):
//!
//! - 問題のある行はその行だけ既定に戻し、残りの行は使う(#1・#2・#4・#6〜#10)。知らない ID は無視する(#3)
//! - 受け口は設定では変えない(#5。値はキーの文字列だけで、受け口は既定の表のまま)
//! - 重なりは「関わる設定の行をすべて既定に戻す」を無くなるまで繰り返す(#9)。既定の表は重ならないので必ず止まる
//! - 結果は書いた順によらない。問題は表の順に並べ、知らない ID は書いた順で最後に並べる
//!
//! 知らせ・標準エラーに出すのは、コマンドの ID・キーの文字列・理由の種類だけ(AC-K09)。書いた人の文字列(ID とキー)は
//! 問題を作るときに制御文字を除き [`MAX_SHOWN_CHARS`] 文字で切る。

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use super::key::{KeyChord, KeyParseError, Modifier};
use super::reserved::is_reserved;
use super::table::{ShortcutBinding, ShortcutCommandId, ShortcutReceiver, ShortcutTable};

/// 知らせに出す ID とキーの文字列の最大の文字数(【仮定】ADR-012 の 3)。
pub const MAX_SHOWN_CHARS: usize = 64;
/// 知らせの 1 行に並べる問題の数の上限(残りは「ほか N 件」にまとめる)。
pub const MAX_NOTICE_ITEMS: usize = 8;

/// 設定の値。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverrideValue {
    /// 文字列(キーの文字列として読む)。
    Text(String),
    /// 文字列でない値(数・真偽・配列・2 段以上の表など)。中身は持たない。
    NotText,
}

/// 設定の 1 行(`[shortcuts]` の下の「コマンドの ID = 値」)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutOverride {
    /// コマンドの ID の文字列(書いたまま。大文字・小文字を区別する)。
    pub id: String,
    /// 値。
    pub value: OverrideValue,
}

/// キーの文字列を読めなかった理由の種類([`KeyParseError`] から元の文字列を除いたもの)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyParseReason {
    /// 空の文字列。
    Empty,
    /// 空の要素がある(`+` の前後に何も無い)。
    EmptyPart,
    /// 修飾キーでもキーでもない要素がある。
    UnknownPart,
    /// 同じ修飾キーを 2 度書いた。
    DuplicateModifier,
    /// 修飾キーだけでキーが無い。
    NoKey,
    /// キーが 2 つ以上ある。
    MultipleKeys,
    /// キーの後ろに修飾キーがある。
    KeyNotLast,
}

impl From<&KeyParseError> for KeyParseReason {
    fn from(error: &KeyParseError) -> Self {
        match error {
            KeyParseError::Empty => Self::Empty,
            KeyParseError::EmptyPart(_) => Self::EmptyPart,
            KeyParseError::UnknownPart { .. } => Self::UnknownPart,
            KeyParseError::DuplicateModifier { .. } => Self::DuplicateModifier,
            KeyParseError::NoKey(_) => Self::NoKey,
            KeyParseError::MultipleKeys { .. } => Self::MultipleKeys,
            KeyParseError::KeyNotLast { .. } => Self::KeyNotLast,
        }
    }
}

impl KeyParseReason {
    const fn text(self) -> &'static str {
        match self {
            Self::Empty => "キーが空です",
            Self::EmptyPart => "「+」の前後が空です",
            Self::UnknownPart => "修飾キーでもキーでもない部分があります",
            Self::DuplicateModifier => "同じ修飾キーを 2 度書いています",
            Self::NoKey => "修飾キーだけでキーがありません",
            Self::MultipleKeys => "キーが 2 つ以上あります",
            Self::KeyNotLast => "キーの後ろに修飾キーがあります",
        }
    }
}

/// 問題の理由の種類(ADR-012 の 3 の表の番号)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShortcutSettingReason {
    /// #1 値が文字列でない。既定に戻した。
    NotText,
    /// #2 キーの文字列を読めない。既定に戻した。
    UnreadableKey(KeyParseReason),
    /// #3 知らないコマンドの ID。無視した。
    UnknownCommand,
    /// #4 変えられないコマンド(受け口が「編集」の標準の項目)。無視した。
    NotChangeable,
    /// #6 予約のキー。既定に戻した。
    Reserved,
    /// #7 メニューの項目の行で `Cmd` を含まないキー。既定に戻した。
    MenuKeyNeedsCmd,
    /// #8 画面の行で `Cmd` を含まず、修飾キーの無い `Enter`・`Space`・`Backspace`・`Delete` でもないキー。既定に戻した。
    ScreenKeyNotAllowed,
    /// #9 ほかのコマンドと同じキー。既定に戻した。
    Conflict {
        /// 同じキーを使っているコマンド(複数あれば表の順で先のもの)。
        other: ShortcutCommandId,
    },
    /// #10 同じコマンドを 2 度書いた。両方を使わず既定に戻した。
    DuplicateCommand,
}

/// 設定の 1 行の問題。ID とキーの文字列は知らせに出せる形(制御文字を除き [`MAX_SHOWN_CHARS`] 文字で切った形)で持つ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutSettingProblem {
    command: Option<ShortcutCommandId>,
    id: String,
    key: Option<String>,
    reason: ShortcutSettingReason,
}

impl ShortcutSettingProblem {
    fn known(
        command: ShortcutCommandId,
        key: Option<String>,
        reason: ShortcutSettingReason,
    ) -> Self {
        Self {
            command: Some(command),
            id: command.as_str().to_owned(),
            key,
            reason,
        }
    }

    /// 問題の行のコマンド。知らない ID なら `None`。
    #[must_use]
    pub const fn command(&self) -> Option<ShortcutCommandId> {
        self.command
    }

    /// コマンドの ID の文字列(知らない ID は制御文字を除き切った形)。
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// キーの文字列。読めたキーは正規の書式、読めないキーは書いた文字列を制御文字を除き切った形。
    /// 値が文字列でないとき・同じコマンドを 2 度書いたときは `None`。
    #[must_use]
    pub fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    /// 理由の種類。
    #[must_use]
    pub const fn reason(&self) -> ShortcutSettingReason {
        self.reason
    }

    /// 標準エラー用の 1 行(頭の `[arin:settings]` は出す側が付ける)。
    #[must_use]
    pub fn diagnostic_line(&self) -> String {
        format!("ショートカットの設定: {self}")
    }
}

impl fmt::Display for ShortcutSettingProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.key {
            Some(key) => write!(f, "{} = {key}", self.id)?,
            None => f.write_str(&self.id)?,
        }
        f.write_str("(")?;
        match self.reason {
            ShortcutSettingReason::NotText => f.write_str("値が文字列ではありません")?,
            ShortcutSettingReason::UnreadableKey(reason) => {
                write!(f, "キーを読めません: {}", reason.text())?;
            }
            ShortcutSettingReason::UnknownCommand => f.write_str("知らないコマンドです")?,
            ShortcutSettingReason::NotChangeable => f.write_str("変えられないコマンドです")?,
            ShortcutSettingReason::Reserved => f.write_str("macOS が使うキーです")?,
            ShortcutSettingReason::MenuKeyNeedsCmd => f.write_str("⌘ を含むキーにしてください")?,
            ShortcutSettingReason::ScreenKeyNotAllowed => f.write_str(
                "⌘ を含むキーか、修飾キーの無い Enter・Space・Backspace・Delete にしてください",
            )?,
            ShortcutSettingReason::Conflict { other } => match &self.key {
                Some(key) => write!(f, "{key} は {other} が使っています")?,
                None => write!(f, "{other} と同じキーです")?,
            },
            ShortcutSettingReason::DuplicateCommand => f.write_str("2 度書かれています")?,
        }
        f.write_str(")")
    }
}

/// 画面に出す知らせの 1 行。問題が無ければ `None`。
#[must_use]
pub fn notice_line(problems: &[ShortcutSettingProblem]) -> Option<String> {
    if problems.is_empty() {
        return None;
    }
    let items: Vec<String> = problems
        .iter()
        .take(MAX_NOTICE_ITEMS)
        .map(ToString::to_string)
        .collect();
    let rest = problems.len().saturating_sub(MAX_NOTICE_ITEMS);
    let more = if rest == 0 {
        String::new()
    } else {
        format!("、ほか {rest} 件")
    };
    Some(format!(
        "ショートカットの設定の {} 行を使わず、既定のキーのままにしました: {}{more}",
        problems.len(),
        items.join("、")
    ))
}

/// 書いた人の文字列を知らせに出せる形にする(制御文字と、文字の向き・行を変える文字を除き、[`MAX_SHOWN_CHARS`] 文字で切る)。
fn shown(text: &str) -> String {
    text.chars()
        .filter(|&c| !c.is_control() && !is_direction_or_line_mark(c))
        .take(MAX_SHOWN_CHARS)
        .collect()
}

/// 知らせの見た目の並びを変えられる文字(双方向の制御 U+200E・200F・202A〜202E・2066〜2069 と、行・段落の区切り U+2028・2029)。
const fn is_direction_or_line_mark(c: char) -> bool {
    matches!(
        c,
        '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{2028}' | '\u{2029}'
    )
}

/// 修飾キーの条件(ADR-012 の 3 の #7・#8)に合わなければ理由を返す。
fn modifier_problem(receiver: ShortcutReceiver, chord: KeyChord) -> Option<ShortcutSettingReason> {
    if chord.modifiers().contains(Modifier::Cmd) {
        return None;
    }
    match receiver {
        ShortcutReceiver::MenuItem => Some(ShortcutSettingReason::MenuKeyNeedsCmd),
        ShortcutReceiver::Screen => {
            let bare_allowed = chord.modifiers().is_empty()
                && matches!(
                    chord.key().as_str(),
                    "Enter" | "Space" | "Backspace" | "Delete"
                );
            (!bare_allowed).then_some(ShortcutSettingReason::ScreenKeyNotAllowed)
        }
        // 編集の項目の行は、ここに来る前に「変えられない」で外している。
        ShortcutReceiver::EditItem => Some(ShortcutSettingReason::NotChangeable),
    }
}

/// 1 つのコマンドに書かれた行を検査し、使うキーか問題を返す(重なりの検査の前)。
fn check_rows(
    binding: &ShortcutBinding,
    values: &[&OverrideValue],
) -> Result<KeyChord, ShortcutSettingProblem> {
    let command = binding.command();
    let text_of = |value: &OverrideValue| match value {
        OverrideValue::Text(text) => Some(shown(text)),
        OverrideValue::NotText => None,
    };
    if binding.receiver() == ShortcutReceiver::EditItem {
        let key = match values {
            [only] => text_of(only),
            _ => None,
        };
        return Err(ShortcutSettingProblem::known(
            command,
            key,
            ShortcutSettingReason::NotChangeable,
        ));
    }
    let [value] = values else {
        return Err(ShortcutSettingProblem::known(
            command,
            None,
            ShortcutSettingReason::DuplicateCommand,
        ));
    };
    let OverrideValue::Text(text) = value else {
        return Err(ShortcutSettingProblem::known(
            command,
            None,
            ShortcutSettingReason::NotText,
        ));
    };
    let chord = KeyChord::parse(text).map_err(|error| {
        ShortcutSettingProblem::known(
            command,
            Some(shown(text)),
            ShortcutSettingReason::UnreadableKey(KeyParseReason::from(&error)),
        )
    })?;
    let reason = if is_reserved(chord) {
        Some(ShortcutSettingReason::Reserved)
    } else {
        modifier_problem(binding.receiver(), chord)
    };
    match reason {
        Some(reason) => Err(ShortcutSettingProblem::known(
            command,
            Some(chord.to_string()),
            reason,
        )),
        None => Ok(chord),
    }
}

/// 既定の表に設定の行を当てた表と、問題の一覧を返す(ADR-012 の 3 の #1〜#10)。
///
/// 結果の表は必ず [`ShortcutTable::from_bindings`] の検査を通り、受け口は `defaults` のまま。
/// 問題は表の順(知らない ID は書いた順で最後。同じ知らない ID は 1 つにまとめる)。
#[must_use]
pub fn apply_overrides(
    defaults: &ShortcutTable,
    overrides: &[ShortcutOverride],
) -> (ShortcutTable, Vec<ShortcutSettingProblem>) {
    // コマンドごとに書かれた値を集める。知らない ID は書いた順に残す。
    let mut written: BTreeMap<ShortcutCommandId, Vec<&OverrideValue>> = BTreeMap::new();
    let mut unknown: Vec<&ShortcutOverride> = Vec::new();
    // 同じ知らない ID かどうかは集合で見る(行の数だけ並びを探すと、行が多い設定で起動が止まる)。
    let mut unknown_ids: BTreeSet<&str> = BTreeSet::new();
    for row in overrides {
        match ShortcutCommandId::from_id(&row.id) {
            Some(command) => written.entry(command).or_default().push(&row.value),
            None => {
                if unknown_ids.insert(row.id.as_str()) {
                    unknown.push(row);
                }
            }
        }
    }

    // 行ごとの検査。
    let mut candidates: BTreeMap<ShortcutCommandId, KeyChord> = BTreeMap::new();
    let mut problems: BTreeMap<ShortcutCommandId, ShortcutSettingProblem> = BTreeMap::new();
    for (&command, values) in &written {
        match check_rows(defaults.get(command), values) {
            Ok(chord) => {
                candidates.insert(command, chord);
            }
            Err(problem) => {
                problems.insert(command, problem);
            }
        }
    }

    // 重なりの検査。関わる設定の行をすべて既定に戻し、無くなるまで繰り返す。
    // 1 回で少なくとも 1 行が既定に戻り、既定の行どうしは重ならないので、設定の行の数の回数以内に止まる。
    loop {
        let current = |command: ShortcutCommandId| {
            candidates
                .get(&command)
                .copied()
                .unwrap_or_else(|| defaults.get(command).accelerator())
        };
        let conflicts: Vec<(ShortcutCommandId, KeyChord, ShortcutCommandId)> = candidates
            .iter()
            .filter_map(|(&command, &chord)| {
                defaults
                    .bindings()
                    .iter()
                    .map(ShortcutBinding::command)
                    .find(|&other| other != command && current(other) == chord)
                    .map(|other| (command, chord, other))
            })
            .collect();
        if conflicts.is_empty() {
            break;
        }
        for (command, chord, other) in conflicts {
            candidates.remove(&command);
            problems.insert(
                command,
                ShortcutSettingProblem::known(
                    command,
                    Some(chord.to_string()),
                    ShortcutSettingReason::Conflict { other },
                ),
            );
        }
    }

    let bindings = defaults
        .bindings()
        .iter()
        .map(|b| {
            let chord = candidates
                .get(&b.command())
                .copied()
                .unwrap_or_else(|| b.accelerator());
            ShortcutBinding::new(b.command(), chord, b.receiver())
        })
        .collect();
    // 上の検査で重なりを除いたので必ず通る。万一通らなければ既定の表を使う(安全側)。
    let table = ShortcutTable::from_bindings(bindings).unwrap_or_else(|_| defaults.clone());

    let mut ordered: Vec<ShortcutSettingProblem> = defaults
        .bindings()
        .iter()
        .filter_map(|b| problems.remove(&b.command()))
        .collect();
    ordered.extend(unknown.into_iter().map(|row| ShortcutSettingProblem {
        command: None,
        id: shown(&row.id),
        key: match &row.value {
            OverrideValue::Text(text) => Some(shown(text)),
            OverrideValue::NotText => None,
        },
        reason: ShortcutSettingReason::UnknownCommand,
    }));
    (table, ordered)
}
