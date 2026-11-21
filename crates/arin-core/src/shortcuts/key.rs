//! キーの文字列(`Cmd+Shift+N` の書式)を修飾キーとキーに分けて読む(計画書 T38d)。
//!
//! 書式は Tauri(muda 0.20)のアクセラレータにそろえる。M4.5(T64)で設定ファイルの値を検査する土台なので、
//! muda が読める書式のうち、画面(`src/logic/keys/shortcutTable.ts`)も読めるものだけを受け付け、それ以外は理由つきで拒否する。
//!
//! - 要素は `+` で区切る。修飾キーを先に、キーを最後に 1 つだけ書く。前後の空白・空の要素は拒否する
//! - 修飾キー: `Cmd`(別名 `Command`・`Super`)・`Shift`・`Alt`(別名 `Option`)・`Ctrl`(別名 `Control`)。
//!   大文字・小文字を区別しない。同じ修飾キーを 2 度書くと拒否する(別名どうしも同じ)
//! - キー: 英字 `A`〜`Z`・数字 `0`〜`9`・記号 `` [ ] - = , . / ; ' ` \ ``・名前のキー(`Enter`・`Backspace`・`Delete`・
//!   `Escape`(別名 `Esc`)・`Tab`・`Space`・`ArrowUp`・`ArrowDown`・`ArrowLeft`・`ArrowRight`)・ファンクションキー `F1`〜`F12`
//!   (2026-11-21 に足した。ADR-012 の追記)。大文字・小文字を区別しない
//! - 書き出すときは正規の書式にする: 修飾キーは `Cmd`・`Shift`・`Alt`・`Ctrl` の順、英字は大文字、名前のキーは上の綴り
//!   (`Shift+cmd+n` → `Cmd+Shift+N`)。修飾キーの順が違うだけの文字列は同じキーとみなす

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

/// 要素の区切り。
const SEPARATOR: char = '+';

/// 修飾キー。並びの順が正規の書式の順。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Modifier {
    /// ⌘
    Cmd,
    /// ⇧
    Shift,
    /// ⌥
    Alt,
    /// ⌃
    Ctrl,
}

impl Modifier {
    /// すべての修飾キー(正規の書式の順)。
    pub const ALL: [Self; 4] = [Self::Cmd, Self::Shift, Self::Alt, Self::Ctrl];

    /// 正規の書式の綴り。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cmd => "Cmd",
            Self::Shift => "Shift",
            Self::Alt => "Alt",
            Self::Ctrl => "Ctrl",
        }
    }

    /// 要素 1 つを修飾キーとして読む(別名を含む。大文字・小文字を区別しない)。
    fn from_part(part: &str) -> Option<Self> {
        match part.to_ascii_lowercase().as_str() {
            "cmd" | "command" | "super" => Some(Self::Cmd),
            "shift" => Some(Self::Shift),
            "alt" | "option" => Some(Self::Alt),
            "ctrl" | "control" => Some(Self::Ctrl),
            _ => None,
        }
    }

    const fn bit(self) -> u8 {
        match self {
            Self::Cmd => 1,
            Self::Shift => 1 << 1,
            Self::Alt => 1 << 2,
            Self::Ctrl => 1 << 3,
        }
    }
}

impl fmt::Display for Modifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 修飾キーの集合。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Modifiers(u8);

impl Modifiers {
    /// 空の集合。
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// 修飾キーを含むか。
    #[must_use]
    pub const fn contains(self, modifier: Modifier) -> bool {
        self.0 & modifier.bit() != 0
    }

    /// 空か(修飾キーの無いキー)。
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// 修飾キーを足した集合。
    #[must_use]
    pub const fn with(self, modifier: Modifier) -> Self {
        Self(self.0 | modifier.bit())
    }

    /// 含む修飾キー(正規の書式の順)。
    pub fn iter(self) -> impl Iterator<Item = Modifier> {
        Modifier::ALL.into_iter().filter(move |m| self.contains(*m))
    }
}

/// 名前で書くキー。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum NamedKey {
    Enter,
    Backspace,
    Delete,
    Escape,
    Tab,
    Space,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
}

impl NamedKey {
    const ALL: [Self; 10] = [
        Self::Enter,
        Self::Backspace,
        Self::Delete,
        Self::Escape,
        Self::Tab,
        Self::Space,
        Self::ArrowUp,
        Self::ArrowDown,
        Self::ArrowLeft,
        Self::ArrowRight,
    ];

    const fn as_str(self) -> &'static str {
        match self {
            Self::Enter => "Enter",
            Self::Backspace => "Backspace",
            Self::Delete => "Delete",
            Self::Escape => "Escape",
            Self::Tab => "Tab",
            Self::Space => "Space",
            Self::ArrowUp => "ArrowUp",
            Self::ArrowDown => "ArrowDown",
            Self::ArrowLeft => "ArrowLeft",
            Self::ArrowRight => "ArrowRight",
        }
    }

    fn from_part(part: &str) -> Option<Self> {
        if part.eq_ignore_ascii_case("esc") {
            return Some(Self::Escape);
        }
        Self::ALL
            .into_iter()
            .find(|k| k.as_str().eq_ignore_ascii_case(part))
    }
}

/// 1 文字で書く記号のキー(muda と画面の両方が読めるもの)。
const SYMBOL_KEYS: [&str; 11] = ["[", "]", "-", "=", ",", ".", "/", ";", "'", "`", "\\"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum KeyRepr {
    /// 英字(大文字)・数字・記号の 1 文字(ASCII)。綴りは `SYMBOL_KEYS` か英数字。
    Char(u8),
    Named(NamedKey),
    /// ファンクションキー(1〜12)。
    Function(u8),
}

/// ファンクションキーの綴り(`F1`〜`F12`。添字は番号 − 1)。
const FUNCTION_KEYS: [&str; 12] = [
    "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12",
];

/// キー(修飾キーを除いた部分)。読めた値だけを持つ。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Key(KeyRepr);

impl Key {
    /// 要素 1 つをキーとして読む。読めなければ `None`。
    fn from_part(part: &str) -> Option<Self> {
        if let [b] = part.as_bytes() {
            if b.is_ascii_alphanumeric() {
                return Some(Self(KeyRepr::Char(b.to_ascii_uppercase())));
            }
            if SYMBOL_KEYS.contains(&part) {
                return Some(Self(KeyRepr::Char(*b)));
            }
            return None;
        }
        if let Some(index) = FUNCTION_KEYS
            .iter()
            .position(|name| name.eq_ignore_ascii_case(part))
        {
            // 添字は 0〜11 なので u8 に収まる。
            return u8::try_from(index + 1)
                .ok()
                .map(|n| Self(KeyRepr::Function(n)));
        }
        NamedKey::from_part(part).map(|k| Self(KeyRepr::Named(k)))
    }

    /// ファンクションキー(`F1`〜`F12`)か。
    #[must_use]
    pub const fn is_function(&self) -> bool {
        matches!(self.0, KeyRepr::Function(_))
    }

    /// 正規の書式の綴り(`N`・`[`・`Enter` など)。
    #[must_use]
    pub fn as_str(&self) -> &str {
        match &self.0 {
            // 英数字と記号は ASCII の 1 バイトなので、必ず UTF-8 として読める。
            KeyRepr::Char(b) => std::str::from_utf8(std::slice::from_ref(b)).unwrap_or_default(),
            KeyRepr::Named(k) => k.as_str(),
            // 作るのは `from_part` だけで、番号は 1〜12。
            KeyRepr::Function(n) => FUNCTION_KEYS
                .get(usize::from(*n).wrapping_sub(1))
                .copied()
                .unwrap_or_default(),
        }
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 修飾キーとキーの組(ショートカット 1 つ)。読めた値だけを持つ。
///
/// 等しさは修飾キーの集合とキーで決まる(書いた順・大文字・小文字・別名によらない)。
/// 文字列にすると正規の書式になる。JSON では文字列。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KeyChord {
    modifiers: Modifiers,
    key: Key,
}

/// キーの文字列を読めなかった理由。元の文字列を持つ。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum KeyParseError {
    /// 空の文字列。
    #[error("キーの文字列が空")]
    Empty,
    /// 空の要素がある(`Cmd+`・`+A`・`Cmd++A`)。
    #[error("キーの文字列 {0:?} に空の要素がある(`+` の前後に何も無い)")]
    EmptyPart(String),
    /// 修飾キーでもキーでもない要素(未知の名前・前後の空白・使えない文字)。
    #[error("キーの文字列 {accelerator:?} の {part:?} は修飾キーでもキーでもない")]
    UnknownPart {
        /// 元の文字列。
        accelerator: String,
        /// 読めなかった要素。
        part: String,
    },
    /// 同じ修飾キーを 2 度書いた(別名どうしを含む)。
    #[error("キーの文字列 {accelerator:?} で修飾キー {modifier} が重なっている")]
    DuplicateModifier {
        /// 元の文字列。
        accelerator: String,
        /// 重なった修飾キー。
        modifier: Modifier,
    },
    /// 修飾キーだけでキーが無い。
    #[error("キーの文字列 {0:?} にキーが無い(修飾キーだけ)")]
    NoKey(String),
    /// キーが 2 つ以上ある。
    #[error("キーの文字列 {accelerator:?} にキーが 2 つ以上ある")]
    MultipleKeys {
        /// 元の文字列。
        accelerator: String,
    },
    /// キーの後ろに修飾キーがある(キーは最後に書く)。
    #[error("キーの文字列 {accelerator:?} でキーの後ろに修飾キーがある(キーは最後に書く)")]
    KeyNotLast {
        /// 元の文字列。
        accelerator: String,
    },
}

impl KeyChord {
    /// 修飾キーとキーから作る。
    #[must_use]
    pub const fn new(modifiers: Modifiers, key: Key) -> Self {
        Self { modifiers, key }
    }

    /// キーの文字列を読む。
    ///
    /// # Errors
    ///
    /// 書式に合わないとき(理由は [`KeyParseError`])。
    pub fn parse(accelerator: &str) -> Result<Self, KeyParseError> {
        if accelerator.is_empty() {
            return Err(KeyParseError::Empty);
        }
        let parts: Vec<&str> = accelerator.split(SEPARATOR).collect();
        if parts.iter().any(|p| p.is_empty()) {
            return Err(KeyParseError::EmptyPart(accelerator.to_owned()));
        }

        let mut modifiers = Modifiers::empty();
        let mut key: Option<Key> = None;
        for part in parts {
            if let Some(modifier) = Modifier::from_part(part) {
                if key.is_some() {
                    return Err(KeyParseError::KeyNotLast {
                        accelerator: accelerator.to_owned(),
                    });
                }
                if modifiers.contains(modifier) {
                    return Err(KeyParseError::DuplicateModifier {
                        accelerator: accelerator.to_owned(),
                        modifier,
                    });
                }
                modifiers = modifiers.with(modifier);
                continue;
            }
            let Some(parsed) = Key::from_part(part) else {
                return Err(KeyParseError::UnknownPart {
                    accelerator: accelerator.to_owned(),
                    part: part.to_owned(),
                });
            };
            if key.is_some() {
                return Err(KeyParseError::MultipleKeys {
                    accelerator: accelerator.to_owned(),
                });
            }
            key = Some(parsed);
        }

        match key {
            Some(key) => Ok(Self { modifiers, key }),
            None => Err(KeyParseError::NoKey(accelerator.to_owned())),
        }
    }

    /// 修飾キーの集合。
    #[must_use]
    pub const fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    /// キー。
    #[must_use]
    pub const fn key(&self) -> Key {
        self.key
    }
}

impl fmt::Display for KeyChord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for modifier in self.modifiers.iter() {
            write!(f, "{modifier}{SEPARATOR}")?;
        }
        f.write_str(self.key.as_str())
    }
}

impl FromStr for KeyChord {
    type Err = KeyParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl Serialize for KeyChord {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for KeyChord {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(serde::de::Error::custom)
    }
}
