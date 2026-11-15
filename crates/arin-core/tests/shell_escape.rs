//! ターミナルへ挿入する文字列のシェルエスケープ(設計書 §5.1 の `ShellEscaper`、§12「シェルへの注入」、
//! 計画書 T38b、【仮定】17)。zsh に渡して元に戻るかの往復は T44a(実 zsh)で確かめる。

use arin_core::paths::shell::escape_shell_word;

/// エスケープした文字列に、PTY に送ると行の確定や制御になるバイト(C0 と DEL)が無いこと。
fn assert_no_control_bytes(escaped: &str) {
    assert!(
        escaped.bytes().all(|b| b >= 0x20 && b != 0x7f),
        "制御文字のバイトが残っている: {escaped:?}"
    );
}

#[test]
fn 空白はバックスラッシュでエスケープする() {
    assert_eq!(escape_shell_word("src/a b.ts"), r"src/a\ b.ts");
    assert_eq!(escape_shell_word("a  b"), r"a\ \ b");
}

#[test]
fn 特殊文字の無い名前はそのまま() {
    for name in [
        "README.md",
        "src/ui/App.tsx",
        "a_b-c.d+e,f:g@h%i",
        "-rf",
        "v1.2.3",
    ] {
        assert_eq!(escape_shell_word(name), name, "{name:?}");
    }
}

#[test]
fn シェルの特殊文字をすべてエスケープする() {
    let table: [(&str, &str); 21] = [
        ("a'b", r"a\'b"),
        ("a\"b", r#"a\"b"#),
        (r"a\b", r"a\\b"),
        ("a$b", r"a\$b"),
        ("a`b", r"a\`b"),
        ("a!b", r"a\!b"),
        ("a&b", r"a\&b"),
        ("a*b", r"a\*b"),
        ("a?b", r"a\?b"),
        ("a(b", r"a\(b"),
        ("a)b", r"a\)b"),
        ("a[b", r"a\[b"),
        ("a]b", r"a\]b"),
        ("a{b", r"a\{b"),
        ("a}b", r"a\}b"),
        ("a;b", r"a\;b"),
        ("a<b", r"a\<b"),
        ("a>b", r"a\>b"),
        ("a|b", r"a\|b"),
        ("a#b", r"a\#b"),
        ("a^b", r"a\^b"),
    ];
    for (name, expected) in table {
        assert_eq!(escape_shell_word(name), expected, "{name:?}");
    }
}

#[test]
fn 特殊文字が並んでも_1_文字ずつエスケープする() {
    assert_eq!(escape_shell_word("$(rm -rf ~)"), r"\$\(rm\ -rf\ \~\)");
    assert_eq!(escape_shell_word("`reboot`"), r"\`reboot\`");
    assert_eq!(escape_shell_word("a;b&&c||d"), r"a\;b\&\&c\|\|d");
    assert_eq!(escape_shell_word("${HOME}"), r"\$\{HOME\}");
    assert_eq!(escape_shell_word("'\"'"), r#"\'\"\'"#);
}

#[test]
fn 先頭のチルダとイコールをエスケープする() {
    assert_eq!(escape_shell_word("~"), r"\~");
    assert_eq!(escape_shell_word("~root/x"), r"\~root/x");
    assert_eq!(escape_shell_word("=ls"), r"\=ls");
    // 途中のものも(zsh の EXTENDED_GLOB の `~`、MAGIC_EQUAL_SUBST の `=` に備えて)エスケープする。
    assert_eq!(escape_shell_word("a~b=c"), r"a\~b\=c");
}

#[test]
fn 改行やタブを含む名前はドル引用符で包み改行のバイトを含まない() {
    let table: [(&str, &str); 6] = [
        ("a\nb", r"$'a\x0ab'"),
        ("a\rb", r"$'a\x0db'"),
        ("a\tb", r"$'a\x09b'"),
        ("a\u{1b}[31mb", r"$'a\x1b[31mb'"),
        ("a\u{7f}b", r"$'a\x7fb'"),
        ("a\u{3}b", r"$'a\x03b'"),
    ];
    for (name, expected) in table {
        let escaped = escape_shell_word(name);
        assert_eq!(escaped, expected, "{name:?}");
        assert_no_control_bytes(&escaped);
    }
}

#[test]
fn ドル引用符の中では引用符とバックスラッシュと感嘆符だけを書き換える() {
    assert_eq!(escape_shell_word("it's\n"), r"$'it\'s\x0a'");
    assert_eq!(escape_shell_word("a\\b\n"), r"$'a\\b\x0a'");
    assert_eq!(escape_shell_word("!!\n"), r"$'\x21\x21\x0a'");
    // ほかの特殊文字はドル引用符の中では意味を持たないのでそのまま。
    assert_eq!(
        escape_shell_word("$(x) `y` a b;c\n"),
        r"$'$(x) `y` a b;c\x0a'"
    );
}

#[test]
fn c1_制御文字もドル引用符でバイトとして書く() {
    // U+009B(CSI)は UTF-8 で C2 9B。
    let escaped = escape_shell_word("a\u{9b}b");
    assert_eq!(escaped, r"$'a\xc2\x9bb'");
    assert_no_control_bytes(&escaped);
}

#[test]
fn 日本語と絵文字はそのまま() {
    assert_eq!(escape_shell_word("資料/📝メモ.md"), "資料/📝メモ.md");
    assert_eq!(escape_shell_word("👨‍👩‍👧.txt"), "👨‍👩‍👧.txt");
    // 空白だけはエスケープする。
    assert_eq!(escape_shell_word("資料/📝 メモ.md"), r"資料/📝\ メモ.md");
    // 制御文字と一緒でも多バイト文字は崩さない。
    assert_eq!(escape_shell_word("メモ\n.md"), r"$'メモ\x0a.md'");
}

#[test]
fn 空の文字列は空の引数になる() {
    assert_eq!(escape_shell_word(""), "''");
}

#[test]
fn エスケープした文字列は制御文字のバイトを含まない() {
    // 0x00〜0x7f のすべての文字と C1 を 1 文字ずつ試す。
    for code in (0u32..=0x7f).chain(0x80..=0x9f) {
        let c = char::from_u32(code).expect("ASCII と C1 は文字");
        let name = format!("a{c}b");
        let escaped = escape_shell_word(&name);
        assert_no_control_bytes(&escaped);
        assert!(
            !escaped.contains(' ') || escaped.contains(r"\ ") || escaped.starts_with("$'"),
            "空白がエスケープされていない: {escaped:?}"
        );
    }
}
