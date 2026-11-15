//! ルート基準の相対パス(`RelPath`。設計書 §5.2・§7.2「画面から届く相対パス」、計画書 T38a)。

use arin_core::paths::{RelPath, RelPathError};
use ts_rs::TS;

fn rel(text: &str) -> RelPath {
    RelPath::parse(text).unwrap_or_else(|e| panic!("{text:?} を読めなかった: {e}"))
}

#[test]
fn 相対パスはスラッシュ区切りの名前の並びとして読める() {
    let path = rel("src/ui/a b.ts");

    assert_eq!(path.as_str(), "src/ui/a b.ts");
    assert_eq!(path.to_string(), "src/ui/a b.ts");
    assert_eq!(
        path.components().collect::<Vec<_>>(),
        ["src", "ui", "a b.ts"]
    );
    assert!(!path.is_root());

    let one = rel("README.md");
    assert_eq!(one.components().collect::<Vec<_>>(), ["README.md"]);
    // 日本語と絵文字もただの名前。
    assert_eq!(
        rel("資料/📝 メモ.md").components().collect::<Vec<_>>(),
        ["資料", "📝 メモ.md"]
    );
}

#[test]
fn 空の文字列はルートを指す() {
    let root = rel("");

    assert!(root.is_root());
    assert_eq!(root, RelPath::root());
    assert_eq!(root.as_str(), "");
    assert_eq!(root.components().count(), 0);
}

#[test]
fn スラッシュで始まるものは相対パスにできない() {
    for bad in ["/", "/etc/passwd", "/Users/x/dev/app/src", "//a"] {
        assert_eq!(
            RelPath::parse(bad),
            Err(RelPathError::Absolute(bad.to_owned())),
            "{bad:?}"
        );
    }
}

#[test]
fn ドットドットとドットの要素は拒否する() {
    for bad in ["..", ".", "../a", "a/..", "a/../b", "./a", "a/.", "a/./b"] {
        assert_eq!(
            RelPath::parse(bad),
            Err(RelPathError::DotComponent(bad.to_owned())),
            "{bad:?}"
        );
    }
    // ドットで始まるだけの名前、ドットが 3 つの名前はただの名前。
    for ok in [".env", ".git/config", "...", "a/..b", "a/b.."] {
        assert_eq!(rel(ok).as_str(), ok);
    }
}

#[test]
fn 空の要素と末尾のスラッシュは拒否する() {
    for bad in ["a//b", "a/", "src/ui/", "a//", "a/b//c"] {
        assert_eq!(
            RelPath::parse(bad),
            Err(RelPathError::EmptyComponent(bad.to_owned())),
            "{bad:?}"
        );
    }
}

#[test]
fn nul_を含むものは拒否する() {
    for bad in ["\0", "a\0b", "src/\0", "/\0", "../\0"] {
        assert_eq!(
            RelPath::parse(bad),
            Err(RelPathError::Nul(bad.to_owned())),
            "{bad:?}"
        );
    }
    // 理由の文言に、元の文字列を見える形で載せる。
    let message = RelPathError::Nul("a\0b".to_owned()).to_string();
    assert!(message.contains("\\0"), "{message}");
}

#[test]
fn チルダで始まる名前はただの名前として読む() {
    let path = rel("~/a");

    assert_eq!(path.components().collect::<Vec<_>>(), ["~", "a"]);
    assert_eq!(rel("~").file_name(), Some("~"));
    assert_eq!(rel("src/~backup").file_name(), Some("~backup"));
}

#[test]
fn 相対パスの_json_は文字列() {
    let path = rel("src/a b.ts");

    let json = serde_json::to_string(&path).expect("JSON にできる");
    assert_eq!(json, "\"src/a b.ts\"");
    assert_eq!(serde_json::from_str::<RelPath>(&json).ok(), Some(path));
    assert_eq!(
        serde_json::from_str::<RelPath>("\"\"").ok(),
        Some(RelPath::root())
    );

    // 読むときも同じ検査を通す(画面から届く経路はここだけ)。
    for bad in [
        "\"/etc\"",
        "\"../a\"",
        "\"a//b\"",
        "\"a\\u0000b\"",
        "12",
        "null",
    ] {
        assert!(serde_json::from_str::<RelPath>(bad).is_err(), "{bad}");
    }

    assert_eq!(RelPath::inline(&ts_rs::Config::new()), "string");
}

#[test]
fn 親と名前に分けられ_ルートの親は無い() {
    let path = rel("src/ui/a.ts");
    let (parent, name) = path.split_parent().expect("ルートではない");
    assert_eq!(parent, rel("src/ui"));
    assert_eq!(name, "a.ts");
    assert_eq!(path.parent(), Some(rel("src/ui")));
    assert_eq!(path.file_name(), Some("a.ts"));

    let top = rel("README.md");
    assert_eq!(top.split_parent(), Some((RelPath::root(), "README.md")));
    assert_eq!(top.parent(), Some(RelPath::root()));

    let root = RelPath::root();
    assert_eq!(root.split_parent(), None);
    assert_eq!(root.parent(), None);
    assert_eq!(root.file_name(), None);
}

#[test]
fn 子を足しても検査を通る形のまま() {
    let root = RelPath::root();
    let src = root.join("src").expect("名前を足せる");
    assert_eq!(src, rel("src"));
    let file = src.join("a b.ts").expect("名前を足せる");
    assert_eq!(file, rel("src/a b.ts"));
    assert_eq!(RelPath::parse(file.as_str()).as_ref(), Ok(&file));
    assert_eq!(file.split_parent(), Some((src.clone(), "a b.ts")));

    // 足すのは 1 つの名前だけ。検査を通らない形にはしない。
    assert_eq!(
        src.join(""),
        Err(RelPathError::EmptyComponent(String::new()))
    );
    assert_eq!(
        src.join(".."),
        Err(RelPathError::DotComponent("..".to_owned()))
    );
    assert_eq!(
        src.join("."),
        Err(RelPathError::DotComponent(".".to_owned()))
    );
    assert_eq!(
        src.join("a/b"),
        Err(RelPathError::Separator("a/b".to_owned()))
    );
    assert_eq!(
        src.join("/etc"),
        Err(RelPathError::Separator("/etc".to_owned()))
    );
    assert_eq!(src.join("a\0"), Err(RelPathError::Nul("a\0".to_owned())));
}
