//! 名前の検査(`validate_name`)と両方残すの名前(`keep_both_name`)
//! (設計書 §5.1・§7.2、FR-025・FR-026・FR-030、計画書 M3 の【仮定】11、計画書 T39a)。

use arin_core::fileops::InvalidNameReason;
use arin_core::fileops::name::{MAX_NAME_BYTES, validate_name};
use arin_core::fileops::unique::keep_both_name;
use arin_core::ports::fs::EntryKind;

// ---- 名前の検査 ----

#[test]
fn ふつうの名前は通す() {
    for name in [
        "a.md",
        "README",
        ".env",
        ".github",
        "a b.ts",
        "資料",
        "📝 メモ.md",
        "~tilde",
        "-dash",
        "a:b",
        "...a",
        "a..b",
        " ",
    ] {
        assert_eq!(validate_name(name), Ok(()), "{name:?}");
    }
}

#[test]
fn 空の名前_スラッシュ_nul_ドット_ドットドットを拒否する() {
    let cases = [
        ("", InvalidNameReason::Empty),
        ("/", InvalidNameReason::ContainsSlash),
        ("a/b", InvalidNameReason::ContainsSlash),
        ("a/", InvalidNameReason::ContainsSlash),
        ("/a", InvalidNameReason::ContainsSlash),
        ("a\0b", InvalidNameReason::ContainsNul),
        ("\0", InvalidNameReason::ContainsNul),
        (".", InvalidNameReason::DotName),
        ("..", InvalidNameReason::DotName),
    ];
    for (name, reason) in cases {
        assert_eq!(validate_name(name), Err(reason), "{name:?}");
    }
}

#[test]
fn 複数に当たる名前は_nul_スラッシュ_ドットの順で理由を決める() {
    assert_eq!(validate_name("a/\0"), Err(InvalidNameReason::ContainsNul));
    assert_eq!(validate_name("../"), Err(InvalidNameReason::ContainsSlash));
    let long_with_slash = format!("{}/", "a".repeat(300));
    assert_eq!(
        validate_name(&long_with_slash),
        Err(InvalidNameReason::ContainsSlash)
    );
}

#[test]
fn _255_バイトを超える名前を拒否する() {
    assert_eq!(MAX_NAME_BYTES, 255);
    assert_eq!(validate_name(&"a".repeat(255)), Ok(()));
    assert_eq!(
        validate_name(&"a".repeat(256)),
        Err(InvalidNameReason::TooLong)
    );
    // 日本語は 1 文字 3 バイト。85 文字 = 255 バイトは通り、86 文字 = 258 バイトは拒否。
    let ok = "あ".repeat(85);
    assert_eq!(ok.len(), 255);
    assert_eq!(validate_name(&ok), Ok(()));
    let ng = "あ".repeat(86);
    assert_eq!(ng.len(), 258);
    assert_eq!(validate_name(&ng), Err(InvalidNameReason::TooLong));
    // 文字の数ではなくバイトで数える(絵文字は 4 バイト。63 個 + 3 バイト = 255)。
    let emoji = format!("{}abc", "📝".repeat(63));
    assert_eq!(emoji.len(), 255);
    assert_eq!(validate_name(&emoji), Ok(()));
    assert_eq!(
        validate_name(&format!("{emoji}d")),
        Err(InvalidNameReason::TooLong)
    );
}

// ---- 両方残すの名前 ----

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn 両方残すの名前は空白と番号を拡張子の前に足す() {
    assert_eq!(
        keep_both_name("a.md", EntryKind::File, &names(&["a.md"])),
        Ok("a 2.md".to_owned())
    );
    // 拡張子は最後の `.` から(【仮定】10 の名称変更の選択範囲と同じ分け方)。
    assert_eq!(
        keep_both_name("a.tar.gz", EntryKind::File, &names(&["a.tar.gz"])),
        Ok("a.tar 2.gz".to_owned())
    );
    assert_eq!(
        keep_both_name("資料.pdf", EntryKind::FileLink, &names(&["資料.pdf"])),
        Ok("資料 2.pdf".to_owned())
    );
}

#[test]
fn 番号は_2_から空いている最小のもの() {
    assert_eq!(
        keep_both_name(
            "a.md",
            EntryKind::File,
            &names(&["a.md", "a 2.md", "a 3.md", "a 5.md"])
        ),
        Ok("a 4.md".to_owned())
    );
    // 元の名前が無くても 2 から始める(番号を付けた名前を返す)。
    assert_eq!(
        keep_both_name("a.md", EntryKind::File, &names(&[])),
        Ok("a 2.md".to_owned())
    );
    // 番号付きの名前を元にすると、その後ろにさらに番号を足す(元の名前の番号は読まない)。
    assert_eq!(
        keep_both_name("a 2.md", EntryKind::File, &names(&["a 2.md"])),
        Ok("a 2 2.md".to_owned())
    );
}

#[test]
fn 拡張子の無い名前とドットで始まる名前は末尾に足す() {
    assert_eq!(
        keep_both_name("Makefile", EntryKind::File, &names(&["Makefile"])),
        Ok("Makefile 2".to_owned())
    );
    assert_eq!(
        keep_both_name(".env", EntryKind::File, &names(&[".env", ".env 2"])),
        Ok(".env 3".to_owned())
    );
    // フォルダ(とフォルダへのリンク)は `.` を含んでも全体の後ろに足す。
    assert_eq!(
        keep_both_name("docs", EntryKind::Directory, &names(&["docs"])),
        Ok("docs 2".to_owned())
    );
    assert_eq!(
        keep_both_name("v1.2", EntryKind::Directory, &names(&["v1.2"])),
        Ok("v1.2 2".to_owned())
    );
    assert_eq!(
        keep_both_name("lib.d", EntryKind::DirectoryLink, &names(&["lib.d"])),
        Ok("lib.d 2".to_owned())
    );
    // `.` で始まり、ほかにも `.` がある名前は最後の `.` の前に足す。
    assert_eq!(
        keep_both_name(".env.local", EntryKind::File, &names(&[".env.local"])),
        Ok(".env 2.local".to_owned())
    );
}

#[test]
fn 既存の名前とは大文字小文字を区別せずに比べる() {
    // APFS は既定で大文字・小文字を区別しないので、`A 2.md` があれば `a 2.md` は置けない。
    assert_eq!(
        keep_both_name("a.md", EntryKind::File, &names(&["a.md", "A 2.MD"])),
        Ok("a 3.md".to_owned())
    );
}

#[test]
fn 番号を足して長すぎる名前になるなら拒否する() {
    // 254 バイト + 「 2」= 256 バイト。
    let name = format!("{}.md", "a".repeat(251));
    assert_eq!(name.len(), 254);
    assert_eq!(
        keep_both_name(&name, EntryKind::File, &names(&[&name])),
        Err(InvalidNameReason::TooLong)
    );
    // 253 バイト + 「 2」= ちょうど 255 バイトに収まるなら通す。
    let fits = format!("{}.md", "a".repeat(250));
    assert_eq!(
        keep_both_name(&fits, EntryKind::File, &names(&[&fits])).map(|n| n.len()),
        Ok(255)
    );
}

#[test]
fn 元の名前が正しくなければ拒否する() {
    assert_eq!(
        keep_both_name("", EntryKind::File, &names(&[])),
        Err(InvalidNameReason::Empty)
    );
    assert_eq!(
        keep_both_name("a/b", EntryKind::File, &names(&[])),
        Err(InvalidNameReason::ContainsSlash)
    );
    assert_eq!(
        keep_both_name("..", EntryKind::Directory, &names(&[])),
        Err(InvalidNameReason::DotName)
    );
}

#[test]
fn 両方残すの名前は決定的() {
    let existing = names(&["a.md", "a 2.md"]);
    let first = keep_both_name("a.md", EntryKind::File, &existing);
    for _ in 0..10 {
        assert_eq!(keep_both_name("a.md", EntryKind::File, &existing), first);
    }
}
