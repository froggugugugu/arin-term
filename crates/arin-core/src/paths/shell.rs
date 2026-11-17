//! ターミナルへ挿入する文字列のシェルエスケープ(設計書 §5.1 の `ShellEscaper`、§12「シェルへの注入」、
//! 計画書 T38b、【仮定】17)。
//!
//! どんな名前でも、シェル(zsh を正とする)が 1 つの引数として読み、コマンドとして実行されない形にする。
//!
//! - 制御文字(C0・DEL・C1)を含むときは、全体を `$'…'` で包み、制御文字を UTF-8 のバイトごとに `\xNN` で書く。
//!   PTY に改行などの制御のバイトを送らないため(行の確定・割り込み・端末の制御シーケンスにならない)。
//!   中では `\` と `'` をバックスラッシュで、`!`(履歴展開)を `\x21` で書き、ほかはそのまま。
//! - それ以外は、安全な文字(英数字と `_` `.` `/` `-` `+` `,` `:` `@` `%`、ASCII 以外の文字)を除く
//!   すべての ASCII の文字(空白、引用符、`$`、`` ` ``、`~`、`=` など)の前にバックスラッシュを付ける。
//!   `~` と `=` は先頭だけでなく途中でもエスケープする(zsh の `EXTENDED_GLOB`・`MAGIC_EQUAL_SUBST` に備えて)。
//! - 空の文字列は `''`(空の引数)。

use std::fmt::Write as _;

/// 1 つの引数としてシェルに読ませる形にエスケープする。結果は制御文字のバイト(C0・DEL)を含まない。
#[must_use]
pub fn escape_shell_word(text: &str) -> String {
    if text.is_empty() {
        return "''".to_owned();
    }
    if text.chars().any(char::is_control) {
        dollar_quote(text)
    } else {
        backslash_escape(text)
    }
}

/// エスケープせずにそのまま書いてよい文字か。ASCII 以外(日本語・絵文字など)はシェルにとって特別な意味が無い。
fn is_safe(c: char) -> bool {
    !c.is_ascii()
        || c.is_ascii_alphanumeric()
        || matches!(c, '_' | '.' | '/' | '-' | '+' | ',' | ':' | '@' | '%')
}

/// 安全でない文字の前にバックスラッシュを付ける(制御文字を含まない文字列だけに使う)。
fn backslash_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    for c in text.chars() {
        if !is_safe(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// 全体を `$'…'` で包む。制御文字は UTF-8 のバイトごとに `\xNN` で書く。
fn dollar_quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 4 + 3);
    out.push_str("$'");
    for c in text.chars() {
        match c {
            '\\' => out.push_str(r"\\"),
            '\'' => out.push_str(r"\'"),
            '!' => out.push_str(r"\x21"),
            c if c.is_control() => {
                let mut buf = [0u8; 4];
                for byte in c.encode_utf8(&mut buf).bytes() {
                    // String への書き込みは失敗しない。
                    let _ = write!(out, "\\x{byte:02x}");
                }
            }
            c => out.push(c),
        }
    }
    out.push('\'');
    out
}
