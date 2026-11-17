//! 設定ファイルを読んだ結果の型と、使っている内容・いまの問題(`SettingsStore`)の振る舞い
//! (計画書 T64b。ADR-012 の 1・3「ファイル全体の問題」・7、設計書 §7.2、AC-K03・AC-K05 の規則の部分)。
//!
//! TOML の解析はここではしない(T65a が `arin-platform` で行う)。解析済みの結果を組み立てて渡す。

use arin_core::ports::settings_file::{
    MAX_SETTINGS_FILE_BYTES, SettingsFileError, SettingsFileRead,
};
use arin_core::settings::{
    SHORTCUTS_SECTION, SectionBody, SettingEntry, SettingValue, Settings, SettingsDocument,
    SettingsLoad, SettingsProblem, SettingsSection, SettingsStore, SettingsUnreadable,
    TextPosition, decode_contents,
};

fn entry(key: &str, value: &str) -> SettingEntry {
    SettingEntry {
        key: key.to_owned(),
        value: SettingValue::Text(value.to_owned()),
    }
}

fn section(name: &str, entries: Vec<SettingEntry>) -> SettingsSection {
    SettingsSection {
        name: name.to_owned(),
        body: SectionBody::Table(entries),
    }
}

fn loaded(sections: Vec<SettingsSection>) -> SettingsLoad {
    SettingsLoad::Loaded(SettingsDocument { sections })
}

fn shortcuts_load(entries: Vec<SettingEntry>) -> SettingsLoad {
    loaded(vec![section(SHORTCUTS_SECTION, entries)])
}

fn syntax(line: u32, column: u32) -> SettingsLoad {
    SettingsLoad::Syntax(TextPosition::new(line, column))
}

fn started(load: SettingsLoad) -> SettingsStore {
    let mut store = SettingsStore::default();
    store.apply(load);
    store
}

#[test]
fn ファイルが無いときは空で知らせが無い() {
    let store = started(SettingsLoad::Missing);

    assert_eq!(store.settings(), &Settings::default());
    assert!(store.settings().shortcuts().is_empty());
    assert_eq!(store.problem(), None);
    assert_eq!(store.notice_line(), None);
}

#[test]
fn 読めた_shortcuts_の行を書いた順に使う() {
    let store = started(shortcuts_load(vec![
        entry("terminal.new-tab", "Cmd+Shift+T"),
        SettingEntry {
            key: "tree.new-file".to_owned(),
            value: SettingValue::NotText,
        },
    ]));

    assert_eq!(
        store.settings().shortcuts(),
        &[
            entry("terminal.new-tab", "Cmd+Shift+T"),
            SettingEntry {
                key: "tree.new-file".to_owned(),
                value: SettingValue::NotText,
            },
        ]
    );
    assert_eq!(store.problem(), None);
    assert_eq!(store.notice_line(), None);
}

#[test]
fn 構文エラーの起動時は空で行と列を知らせる() {
    let store = started(syntax(12, 5));

    assert!(store.settings().shortcuts().is_empty());
    assert_eq!(
        store.problem(),
        Some(&SettingsProblem::Syntax(TextPosition::new(12, 5)))
    );
    let notice = store.notice_line().expect("知らせがあるはず");
    assert!(notice.contains("12 行 5 列目"), "{notice}");
    assert!(notice.contains("既定のまま"), "{notice}");
    assert!(!notice.contains('\n'), "1 行のはず: {notice}");
}

#[test]
fn 構文エラーの読み直しでは直前の内容を保つ() {
    let mut store = started(shortcuts_load(vec![entry(
        "terminal.new-tab",
        "Cmd+Shift+T",
    )]));
    let before = store.settings().clone();

    let changed = store.apply(syntax(3, 1));

    assert!(changed, "知らせが増えたので変わったとみなすはず");
    assert_eq!(store.settings(), &before);
    let notice = store.notice_line().expect("知らせがあるはず");
    assert!(notice.contains("3 行 1 列目"), "{notice}");
    assert!(notice.contains("直前のまま"), "{notice}");
}

#[test]
fn 構文エラーを直して読み直すと新しい内容になり知らせが消える() {
    let mut store = started(shortcuts_load(vec![entry(
        "terminal.new-tab",
        "Cmd+Shift+T",
    )]));
    store.apply(syntax(3, 1));

    let changed = store.apply(shortcuts_load(vec![entry("tree.new-file", "Cmd+Alt+N")]));

    assert!(changed);
    assert_eq!(
        store.settings().shortcuts(),
        &[entry("tree.new-file", "Cmd+Alt+N")]
    );
    assert_eq!(store.problem(), None);
    assert_eq!(store.notice_line(), None);
}

#[test]
fn ファイルが消えたら空に戻る() {
    let mut store = started(shortcuts_load(vec![entry(
        "terminal.new-tab",
        "Cmd+Shift+T",
    )]));
    store.apply(syntax(1, 1));

    let changed = store.apply(SettingsLoad::Missing);

    assert!(changed);
    assert_eq!(store.settings(), &Settings::default());
    assert_eq!(store.problem(), None);
    assert_eq!(store.notice_line(), None);
}

#[test]
fn 同じ結果を読み直しても変わらない() {
    let mut store = started(shortcuts_load(vec![entry(
        "terminal.new-tab",
        "Cmd+Shift+T",
    )]));

    assert!(!store.apply(shortcuts_load(vec![entry(
        "terminal.new-tab",
        "Cmd+Shift+T"
    )])));
    assert!(store.apply(syntax(2, 2)));
    assert!(!store.apply(syntax(2, 2)));
}

#[test]
fn 読めない理由ごとの知らせ() {
    let cases = [
        (SettingsUnreadable::NotUtf8, "UTF-8"),
        (SettingsUnreadable::TooLarge, "大きすぎ"),
        (SettingsUnreadable::NotAFile, "ファイルではない"),
        (SettingsUnreadable::PermissionDenied, "権限"),
        (SettingsUnreadable::Io, "読み取りに失敗"),
    ];
    for (reason, expected) in cases {
        // 起動時は空(既定のまま)。
        let store = started(SettingsLoad::Unreadable(reason));
        assert!(store.settings().shortcuts().is_empty(), "{reason:?}");
        assert_eq!(
            store.problem(),
            Some(&SettingsProblem::Unreadable(reason)),
            "{reason:?}"
        );
        let notice = store.notice_line().expect("知らせがあるはず");
        assert!(notice.contains(expected), "{reason:?}: {notice}");
        assert!(notice.contains("既定のまま"), "{reason:?}: {notice}");
        assert!(!notice.contains('\n'), "{reason:?}: {notice}");

        // 動作中はファイル全体の問題として直前の内容を保つ。
        let mut store = started(shortcuts_load(vec![entry(
            "terminal.new-tab",
            "Cmd+Shift+T",
        )]));
        store.apply(SettingsLoad::Unreadable(reason));
        assert_eq!(
            store.settings().shortcuts(),
            &[entry("terminal.new-tab", "Cmd+Shift+T")],
            "{reason:?}"
        );
        let notice = store.notice_line().expect("知らせがあるはず");
        assert!(notice.contains(expected), "{reason:?}: {notice}");
        assert!(notice.contains("直前のまま"), "{reason:?}: {notice}");
    }
}

#[test]
fn shortcuts_が表でないときはその節だけ空にして知らせる() {
    let mut store = started(shortcuts_load(vec![entry(
        "terminal.new-tab",
        "Cmd+Shift+T",
    )]));

    store.apply(loaded(vec![SettingsSection {
        name: SHORTCUTS_SECTION.to_owned(),
        body: SectionBody::NotTable,
    }]));

    // ファイルとしては読めたので、直前の内容は保たずに節を空にする。
    assert!(store.settings().shortcuts().is_empty());
    assert_eq!(
        store.problem(),
        Some(&SettingsProblem::SectionNotTable(SHORTCUTS_SECTION))
    );
    let notice = store.notice_line().expect("知らせがあるはず");
    assert!(notice.contains("[shortcuts]"), "{notice}");
    assert!(!notice.contains('\n'), "{notice}");
}

#[test]
fn 知らない節は黙って無視する() {
    let store = started(loaded(vec![
        section("terminal", vec![entry("font_size", "13")]),
        SettingsSection {
            name: "tree".to_owned(),
            body: SectionBody::NotTable,
        },
        section(
            SHORTCUTS_SECTION,
            vec![entry("terminal.new-tab", "Cmd+Shift+T")],
        ),
        section(
            "Shortcuts",
            vec![entry("terminal.close-tab", "Cmd+Shift+W")],
        ),
    ]));

    assert_eq!(
        store.settings().shortcuts(),
        &[entry("terminal.new-tab", "Cmd+Shift+T")]
    );
    assert_eq!(store.problem(), None);
    assert_eq!(store.notice_line(), None);
}

#[test]
fn shortcuts_の節が無ければ空で知らせが無い() {
    let store = started(loaded(vec![section("terminal", vec![entry("a", "b")])]));

    assert!(store.settings().shortcuts().is_empty());
    assert_eq!(store.notice_line(), None);
}

#[test]
fn 知らせに中身の文字列が入らない() {
    const SECRET: &str = "ひみつの文字列-ghp_0123456789";
    let secret_sections = || {
        vec![
            section(SECRET, vec![entry(SECRET, SECRET)]),
            section(SHORTCUTS_SECTION, vec![entry(SECRET, SECRET)]),
        ]
    };

    // 読めた中身があるところへ、ファイル全体の問題が起きたとき。
    let mut notices = Vec::new();
    for load in [
        syntax(7, 3),
        SettingsLoad::Unreadable(SettingsUnreadable::NotUtf8),
        SettingsLoad::Unreadable(SettingsUnreadable::Io),
    ] {
        let mut store = started(loaded(secret_sections()));
        store.apply(load);
        notices.push(store.notice_line().expect("知らせがあるはず"));
        notices.push(store.problem().expect("問題があるはず").diagnostic_line());
    }
    // 節が表でないとき(ほかの節に中身がある)。
    let store = started(loaded(vec![
        section(SECRET, vec![entry(SECRET, SECRET)]),
        SettingsSection {
            name: SHORTCUTS_SECTION.to_owned(),
            body: SectionBody::NotTable,
        },
    ]));
    notices.push(store.notice_line().expect("知らせがあるはず"));
    notices.push(store.problem().expect("問題があるはず").diagnostic_line());

    for notice in notices {
        assert!(!notice.contains(SECRET), "{notice}");
        assert!(!notice.contains("ひみつ"), "{notice}");
        assert!(!notice.contains('/'), "パスを出さないはず: {notice}");
    }
}

#[test]
fn 標準エラーの行は頭を付けずに_1_行() {
    let line = SettingsProblem::Syntax(TextPosition::new(4, 9)).diagnostic_line();

    assert!(line.contains("4 行 9 列目"), "{line}");
    assert!(
        !line.contains("[arin:settings]"),
        "頭は出す側が付ける: {line}"
    );
    assert!(!line.contains('\n'), "{line}");
}

#[test]
fn 位置はバイトの位置から_1_始まりの行と文字の列で数える() {
    let text = "[shortcuts]\n\"a\" = \"あい\" x\n";
    // 2 行目の `x`。`"a" = "あい" ` は 11 文字(あ・い は 3 バイトずつ)。
    let offset = text.find('x').expect("x がある");

    assert_eq!(
        TextPosition::from_byte_offset(text, offset),
        TextPosition::new(2, 12)
    );
    assert_eq!(
        TextPosition::from_byte_offset(text, 0),
        TextPosition::new(1, 1)
    );
    // 文字の途中を指されたら、その文字の位置にする。
    let inside = text.find('い').expect("い がある") + 1;
    assert_eq!(
        TextPosition::from_byte_offset(text, inside),
        TextPosition::new(2, 9)
    );
    // 末尾より後ろは末尾にする。
    assert_eq!(
        TextPosition::from_byte_offset(text, text.len() + 10),
        TextPosition::new(3, 1)
    );
}

#[test]
fn 改行が_crlf_でも行を数えられる() {
    let text = "a = 1\r\nb = x\r\n";
    let offset = text.find('x').expect("x がある");

    assert_eq!(
        TextPosition::from_byte_offset(text, offset),
        TextPosition::new(2, 5)
    );
}

#[test]
fn 中身は_utf_8_で大きさの上限以内なら文字列にする() {
    assert_eq!(decode_contents(b"[shortcuts]\n"), Ok("[shortcuts]\n"));
    assert_eq!(
        decode_contents(&[0xff, 0xfe]),
        Err(SettingsUnreadable::NotUtf8)
    );

    let limit = usize::try_from(MAX_SETTINGS_FILE_BYTES).expect("usize に収まる");
    let at_limit = vec![b'#'; limit];
    assert!(decode_contents(&at_limit).is_ok());
    let over = vec![b'#'; limit + 1];
    assert_eq!(decode_contents(&over), Err(SettingsUnreadable::TooLarge));
}

#[test]
fn ポートの読み取りの結果から読んだ結果を作る() {
    let doc = SettingsDocument {
        sections: vec![section(SHORTCUTS_SECTION, vec![entry("a", "b")])],
    };

    assert_eq!(
        SettingsLoad::from_file(Ok(SettingsFileRead::Missing), |_| unreachable!()),
        SettingsLoad::Missing
    );
    assert_eq!(
        SettingsLoad::from_file(Ok(SettingsFileRead::Contents(b"x".to_vec())), |text| {
            assert_eq!(text, "x");
            Ok(doc.clone())
        }),
        SettingsLoad::Loaded(doc.clone())
    );
    assert_eq!(
        SettingsLoad::from_file(Ok(SettingsFileRead::Contents(b"x".to_vec())), |_| {
            Err(TextPosition::new(1, 2))
        }),
        SettingsLoad::Syntax(TextPosition::new(1, 2))
    );
    assert_eq!(
        SettingsLoad::from_file(
            Ok(SettingsFileRead::Contents(vec![0xff])),
            |_| unreachable!()
        ),
        SettingsLoad::Unreadable(SettingsUnreadable::NotUtf8)
    );
    let errors = [
        (SettingsFileError::TooLarge, SettingsUnreadable::TooLarge),
        (SettingsFileError::NotAFile, SettingsUnreadable::NotAFile),
        (
            SettingsFileError::PermissionDenied,
            SettingsUnreadable::PermissionDenied,
        ),
        (SettingsFileError::Io, SettingsUnreadable::Io),
    ];
    for (error, expected) in errors {
        assert_eq!(
            SettingsLoad::from_file(Err(error), |_| unreachable!()),
            SettingsLoad::Unreadable(expected)
        );
    }
}
