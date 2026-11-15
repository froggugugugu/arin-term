//! プロジェクトの識別子(設計書 §6.2 の `projects[].id`、計画書 M2 の【仮定】1)。
//!
//! UUID v4 の形の文字列(小文字の 16 進とハイフン、`8-4-4-4-12`)。`uuid` クレートは使わない。
//! 乱数は `IdSource`(`arin-platform`)から受け取り、ここでは整形と検査だけをする。

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;
use ts_rs::TS;

/// UUID の文字列の長さ。
const UUID_LEN: usize = 36;
/// ハイフンの位置(0 始まり)。
const HYPHENS: [usize; 4] = [8, 13, 18, 23];

/// プロジェクトの識別子。画面とのやり取りと `state.json` では文字列。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, TS)]
pub struct ProjectId(String);

/// UUID の形でない文字列を ID にしようとした。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("プロジェクトの ID として使えない文字列: {0:?}(UUID の形にする)")]
pub struct ProjectIdError(pub String);

impl ProjectId {
    /// 16 バイトの乱数から UUID v4 の形の ID を作る(版と変種のビットを立てる)。
    #[must_use]
    pub fn from_random_bytes(mut bytes: [u8; 16]) -> Self {
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        let mut text = String::with_capacity(UUID_LEN);
        for (i, byte) in bytes.iter().enumerate() {
            if matches!(i, 4 | 6 | 8 | 10) {
                text.push('-');
            }
            text.push(hex_digit(byte >> 4));
            text.push(hex_digit(byte & 0x0f));
        }
        Self(text)
    }

    /// 文字列から作る。UUID の形(`8-4-4-4-12` の 16 進)であることを確かめ、小文字にそろえる。
    /// 版のビットは見ない(手で書いた `state.json` でも読めるように)。
    ///
    /// # Errors
    ///
    /// UUID の形でないとき。
    pub fn parse(text: &str) -> Result<Self, ProjectIdError> {
        let well_formed = text.len() == UUID_LEN
            && text.char_indices().all(|(i, c)| {
                if HYPHENS.contains(&i) {
                    c == '-'
                } else {
                    c.is_ascii_hexdigit()
                }
            });
        if well_formed {
            Ok(Self(text.to_ascii_lowercase()))
        } else {
            Err(ProjectIdError(text.to_owned()))
        }
    }

    /// 文字列としての中身。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 4 ビットの値(0〜15)を小文字の 16 進の 1 文字にする。
fn hex_digit(nibble: u8) -> char {
    char::from(b"0123456789abcdef"[usize::from(nibble & 0x0f)])
}

impl fmt::Display for ProjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ProjectId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn プロジェクト_id_は_16_バイトから_uuid_v4_の形になる() {
        let zero = ProjectId::from_random_bytes([0; 16]);
        assert_eq!(zero.as_str(), "00000000-0000-4000-8000-000000000000");

        let ones = ProjectId::from_random_bytes([0xff; 16]);
        assert_eq!(ones.as_str(), "ffffffff-ffff-4fff-bfff-ffffffffffff");

        let seq = ProjectId::from_random_bytes([
            0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab,
            0xcd, 0xef,
        ]);
        assert_eq!(seq.to_string(), "01234567-89ab-4def-8123-456789abcdef");
        assert_eq!(ProjectId::parse(seq.as_str()), Ok(seq));
    }

    #[test]
    fn uuid_の形なら大文字も読めて小文字にそろえる() {
        let id = ProjectId::parse("0123ABCD-89AB-4DEF-8123-456789ABCDEF").expect("読める");
        assert_eq!(id.as_str(), "0123abcd-89ab-4def-8123-456789abcdef");
    }

    #[test]
    fn uuid_の形でない文字列はプロジェクト_id_にできない() {
        for bad in [
            "",
            "terminal-1",
            "0123abcd89ab4def8123456789abcdef",
            "0123abcd-89ab-4def-8123-456789abcdeg",
            "0123abcd-89ab-4def-8123_456789abcdef",
            "0123abcd-89ab-4def-8123-456789abcdef0",
            "0123abcd-89ab-4def-8123-456789abcdé",
        ] {
            assert_eq!(
                ProjectId::parse(bad),
                Err(ProjectIdError(bad.to_owned())),
                "{bad:?}"
            );
        }
        assert!(ProjectIdError("x".into()).to_string().contains("\"x\""));
    }

    #[test]
    fn プロジェクト_id_の_json_は文字列() {
        let id = ProjectId::from_random_bytes([7; 16]);

        let json = serde_json::to_string(&id).expect("JSON にできる");
        assert_eq!(json, format!("\"{id}\""));
        assert_eq!(serde_json::from_str::<ProjectId>(&json).ok(), Some(id));

        assert!(serde_json::from_str::<ProjectId>("\"terminal-1\"").is_err());
        assert!(serde_json::from_str::<ProjectId>("12").is_err());
    }

    #[test]
    fn 型は文字列として生成される() {
        assert_eq!(ProjectId::inline(&ts_rs::Config::new()), "string");
    }
}
