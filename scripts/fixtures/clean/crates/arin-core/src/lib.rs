//! 違反の無い見本。ドキュメントコメントに std::fs と書いても違反にしない。

#![forbid(unsafe_code)]

// use std::process::Command; ← コメントアウトした行は数えない
/* ブロックコメントの std::net も数えない
   /* 入れ子のコメント std::fs */ まだコメント std::process */

use std::collections::HashMap;
use std::{fmt, io};

/// 文字列の中の std::fs は数えない。
pub const NOTE: &str = "std::fs は arin-platform で使う";
pub const RAW: &str = r#"std::net "引用符" を含む生文字列"#;

pub fn table<'a>(name: &'a str) -> HashMap<&'a str, char> {
    let mut map = HashMap::new();
    map.insert(name, '"');
    map
}

pub fn describe(e: &dyn fmt::Debug) -> io::Result<String> {
    Ok(format!("{e:?}"))
}
