//! 設定の値をショートカットの表に当てる規則と問題の一覧(計画書 T64a。ADR-012 の 2・3、AC-K03・AC-K04・AC-K09 の規則の部分)。

use std::collections::BTreeSet;

use arin_core::shortcuts::key::KeyChord;
use arin_core::shortcuts::overrides::{
    KeyParseReason, MAX_NOTICE_ITEMS, MAX_SHOWN_CHARS, OverrideValue, ShortcutOverride,
    ShortcutSettingProblem, ShortcutSettingReason, apply_overrides, notice_line,
};
use arin_core::shortcuts::reserved::{RESERVED_KEYS, is_reserved};
use arin_core::shortcuts::sample::sample_settings_toml;
use arin_core::shortcuts::table::{ShortcutCommandId as C, ShortcutTable};

fn chord(s: &str) -> KeyChord {
    KeyChord::parse(s).unwrap_or_else(|e| panic!("{s:?} を読めなかった: {e}"))
}

fn text(id: &str, value: &str) -> ShortcutOverride {
    ShortcutOverride {
        id: id.to_owned(),
        value: OverrideValue::Text(value.to_owned()),
    }
}

fn not_text(id: &str) -> ShortcutOverride {
    ShortcutOverride {
        id: id.to_owned(),
        value: OverrideValue::NotText,
    }
}

fn apply(overrides: &[ShortcutOverride]) -> (ShortcutTable, Vec<ShortcutSettingProblem>) {
    apply_overrides(&ShortcutTable::default_table(), overrides)
}

fn key_of(table: &ShortcutTable, command: C) -> String {
    table.get(command).accelerator().to_string()
}

/// 問題の一覧を (ID・キー・理由) の組にする(比べやすくするため)。
fn summary(
    problems: &[ShortcutSettingProblem],
) -> Vec<(String, Option<String>, ShortcutSettingReason)> {
    problems
        .iter()
        .map(|p| (p.id().to_owned(), p.key().map(str::to_owned), p.reason()))
        .collect()
}

/// 1 行だけ書いたとき、その行が既定に戻って問題が 1 つだけ残ることを確かめ、理由を返す。
fn single_rejection(command: C, value: &str) -> ShortcutSettingReason {
    let (table, problems) = apply(&[text(command.as_str(), value)]);
    assert_eq!(
        table,
        ShortcutTable::default_table(),
        "{command} = {value:?} が既定に戻っていない"
    );
    assert_eq!(
        problems.len(),
        1,
        "{command} = {value:?} の問題: {problems:?}"
    );
    assert_eq!(problems[0].command(), Some(command));
    problems[0].reason()
}

// ---- 当てる ----

#[test]
fn 設定が空なら既定の表と同じで問題が無い() {
    let (table, problems) = apply(&[]);

    assert_eq!(table, ShortcutTable::default_table());
    assert!(problems.is_empty());
    assert_eq!(notice_line(&problems), None);
}

#[test]
fn 書いた行だけキーが替わり書かなかった行は既定のまま() {
    let (table, problems) = apply(&[
        text("terminal.new-tab", "Cmd+Shift+T"),
        text("tree.new-file", "Cmd+Alt+N"),
    ]);

    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(key_of(&table, C::TerminalNewTab), "Cmd+Shift+T");
    assert_eq!(key_of(&table, C::TreeNewFile), "Cmd+Alt+N");
    let defaults = ShortcutTable::default_table();
    for binding in defaults.bindings() {
        let got = table.get(binding.command());
        assert_eq!(got.receiver(), binding.receiver(), "受け口が変わった");
        if !matches!(binding.command(), C::TerminalNewTab | C::TreeNewFile) {
            assert_eq!(got, binding, "{} が既定のままでない", binding.command());
        }
    }
}

#[test]
fn 正規の書式でない文字列も同じキーとして読む() {
    let (table, problems) = apply(&[text("terminal.new-tab", "shift+cmd+t")]);

    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(
        table.get(C::TerminalNewTab).accelerator(),
        chord("Cmd+Shift+T")
    );
}

#[test]
fn 読めないキーの行は既定に戻り理由の種類が残る() {
    let cases = [
        ("Cmd+", KeyParseReason::EmptyPart),
        ("Cmd+Hyper+T", KeyParseReason::UnknownPart),
        ("Cmd+Command+T", KeyParseReason::DuplicateModifier),
        ("Cmd+Shift", KeyParseReason::NoKey),
        ("Cmd+T+U", KeyParseReason::MultipleKeys),
        ("T+Cmd", KeyParseReason::KeyNotLast),
    ];
    for (value, reason) in cases {
        assert_eq!(
            single_rejection(C::TerminalNewTab, value),
            ShortcutSettingReason::UnreadableKey(reason),
            "{value:?}"
        );
    }
    let (_, problems) = apply(&[text("terminal.new-tab", "Cmd+Hyper+T")]);
    assert_eq!(problems[0].id(), "terminal.new-tab");
    assert_eq!(problems[0].key(), Some("Cmd+Hyper+T"));
}

#[test]
fn 空の文字列は読めないキーとして既定に戻す() {
    assert_eq!(
        single_rejection(C::TerminalCloseTab, ""),
        ShortcutSettingReason::UnreadableKey(KeyParseReason::Empty)
    );
}

#[test]
fn 文字列でない値の行は既定に戻す() {
    let (table, problems) = apply(&[not_text("terminal.new-tab")]);

    assert_eq!(table, ShortcutTable::default_table());
    assert_eq!(
        summary(&problems),
        vec![(
            "terminal.new-tab".to_owned(),
            None,
            ShortcutSettingReason::NotText
        )]
    );
}

#[test]
fn 知らないコマンドの_id_は無視し問題に残す() {
    let (table, problems) = apply(&[
        text("terminal.nwe-tab", "Cmd+Shift+T"),
        text("terminal.new-tab", "Cmd+Shift+T"),
        not_text("zzz"),
    ]);

    assert_eq!(key_of(&table, C::TerminalNewTab), "Cmd+Shift+T");
    assert_eq!(
        summary(&problems),
        vec![
            (
                "terminal.nwe-tab".to_owned(),
                Some("Cmd+Shift+T".to_owned()),
                ShortcutSettingReason::UnknownCommand
            ),
            (
                "zzz".to_owned(),
                None,
                ShortcutSettingReason::UnknownCommand
            ),
        ]
    );
    assert_eq!(problems[0].command(), None);
}

#[test]
fn id_は大文字と小文字を区別する() {
    let (table, problems) = apply(&[text("Terminal.New-Tab", "Cmd+Shift+T")]);

    assert_eq!(table, ShortcutTable::default_table());
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].reason(), ShortcutSettingReason::UnknownCommand);
    assert_eq!(problems[0].id(), "Terminal.New-Tab");
}

#[test]
fn 切り取り_コピー_ペーストは変えずに問題に残す() {
    for command in [C::EditCut, C::EditCopy, C::EditPaste] {
        assert_eq!(
            single_rejection(command, "Cmd+Shift+K"),
            ShortcutSettingReason::NotChangeable,
            "{command}"
        );
        // 既定と同じキー・文字列でない値・2 度書いた場合も同じ。
        assert_eq!(
            single_rejection(command, &key_of(&ShortcutTable::default_table(), command)),
            ShortcutSettingReason::NotChangeable
        );
        let (table, problems) =
            apply(&[not_text(command.as_str()), text(command.as_str(), "Cmd+K")]);
        assert_eq!(table, ShortcutTable::default_table());
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].reason(), ShortcutSettingReason::NotChangeable);
    }
}

// ---- 予約のキー ----

/// ADR-012 の 3 の予約のキー(テストの側にも書き、一覧の足し引きに気づけるようにする)。
const EXPECTED_RESERVED: [&str; 18] = [
    "Cmd+H",
    "Cmd+Alt+H",
    "Cmd+Tab",
    "Cmd+Shift+Tab",
    "Cmd+Space",
    "Ctrl+Space",
    "Cmd+Ctrl+Space",
    "Cmd+Alt+Space",
    "Cmd+Shift+3",
    "Cmd+Shift+4",
    "Cmd+Shift+5",
    "Cmd+Ctrl+Q",
    "Cmd+Alt+Escape",
    "Cmd+`",
    "Ctrl+ArrowUp",
    "Ctrl+ArrowDown",
    "Ctrl+ArrowLeft",
    "Ctrl+ArrowRight",
];

#[test]
fn 予約のキーの行は既定に戻す() {
    let expected: BTreeSet<KeyChord> = EXPECTED_RESERVED.iter().map(|s| chord(s)).collect();
    let actual: BTreeSet<KeyChord> = RESERVED_KEYS.iter().map(|s| chord(s)).collect();
    assert_eq!(actual, expected, "予約のキーの一覧が ADR-012 と違う");
    assert_eq!(
        RESERVED_KEYS.len(),
        EXPECTED_RESERVED.len(),
        "一覧に重複がある"
    );

    for key in EXPECTED_RESERVED {
        assert!(is_reserved(chord(key)), "{key}");
        for command in [C::TerminalNewTab, C::TreeNewFile] {
            assert_eq!(
                single_rejection(command, key),
                ShortcutSettingReason::Reserved,
                "{command} = {key}"
            );
        }
    }
    // 書き方が違っても同じキーなら予約。
    assert_eq!(
        single_rejection(C::TerminalNewTab, "command+h"),
        ShortcutSettingReason::Reserved
    );
}

#[test]
fn 既定の表は予約のキーを使わない() {
    for binding in ShortcutTable::default_table().bindings() {
        assert!(
            !is_reserved(binding.accelerator()),
            "{} の既定のキー {} が予約のキー",
            binding.command(),
            binding.accelerator()
        );
    }
    assert!(!is_reserved(chord("Cmd+Q")), "終了のキーは予約にしない");
}

// ---- 修飾キーの条件 ----

#[test]
fn メニューの項目の行で_cmd_を含まないキーは既定に戻す() {
    for key in ["Ctrl+C", "Alt+N", "Shift+T", "T", "Enter", "Ctrl+Shift+T"] {
        for command in [C::TerminalNewTab, C::ViewNextPage] {
            assert_eq!(
                single_rejection(command, key),
                ShortcutSettingReason::MenuKeyNeedsCmd,
                "{command} = {key}"
            );
        }
    }
}

#[test]
fn 画面の行は修飾キーの無い_enter_space_backspace_delete_を許す() {
    for key in ["Enter", "Space", "Backspace", "Delete", "enter"] {
        // 既定の tree.rename(Enter)と重ならないよう、tree.rename を別のキーにする。
        let (table, problems) =
            apply(&[text("tree.trash", key), text("tree.rename", "Cmd+Shift+R")]);
        assert!(problems.is_empty(), "{key}: {problems:?}");
        assert_eq!(table.get(C::TreeTrash).accelerator(), chord(key));
    }
}

#[test]
fn 画面の行で修飾キーの無い文字_矢印_tab_escape_は既定に戻す() {
    let keys = [
        "A",
        "1",
        "[",
        "ArrowUp",
        "ArrowDown",
        "ArrowLeft",
        "ArrowRight",
        "Tab",
        "Escape",
        "Shift+A",
        "Shift+Enter",
        "Alt+A",
        "Ctrl+A",
    ];
    for key in keys {
        assert_eq!(
            single_rejection(C::TreeNewFile, key),
            ShortcutSettingReason::ScreenKeyNotAllowed,
            "{key}"
        );
    }
}

// ---- 重なり ----

#[test]
fn 既定の行と重なる設定の行は既定に戻し相手のコマンドを問題に残す() {
    let (table, problems) = apply(&[text("terminal.new-tab", "Cmd+W")]);

    assert_eq!(table, ShortcutTable::default_table());
    assert_eq!(
        summary(&problems),
        vec![(
            "terminal.new-tab".to_owned(),
            Some("Cmd+W".to_owned()),
            ShortcutSettingReason::Conflict {
                other: C::TerminalCloseTab
            }
        )]
    );
}

#[test]
fn 設定の行どうしが重なると両方を既定に戻す() {
    let (table, problems) = apply(&[
        text("tree.new-file", "Cmd+K"),
        text("terminal.new-tab", "cmd+k"),
        text("view.next-page", "Cmd+Shift+L"),
    ]);

    assert_eq!(key_of(&table, C::TerminalNewTab), "Cmd+T");
    assert_eq!(key_of(&table, C::TreeNewFile), "Cmd+N");
    assert_eq!(key_of(&table, C::ViewNextPage), "Cmd+Shift+L");
    assert_eq!(
        summary(&problems),
        vec![
            (
                "terminal.new-tab".to_owned(),
                Some("Cmd+K".to_owned()),
                ShortcutSettingReason::Conflict {
                    other: C::TreeNewFile
                }
            ),
            (
                "tree.new-file".to_owned(),
                Some("Cmd+K".to_owned()),
                ShortcutSettingReason::Conflict {
                    other: C::TerminalNewTab
                }
            ),
        ]
    );
}

#[test]
fn 既定に戻したことで生じた重なりも解ける() {
    // new-tab と new-file が Cmd+K で重なり既定に戻ると、new-tab の既定 Cmd+T が close-tab の Cmd+T と重なる。
    let (table, problems) = apply(&[
        text("terminal.new-tab", "Cmd+K"),
        text("tree.new-file", "Cmd+K"),
        text("terminal.close-tab", "Cmd+T"),
    ]);

    assert_eq!(table, ShortcutTable::default_table());
    assert_eq!(
        summary(&problems),
        vec![
            (
                "terminal.new-tab".to_owned(),
                Some("Cmd+K".to_owned()),
                ShortcutSettingReason::Conflict {
                    other: C::TreeNewFile
                }
            ),
            (
                "terminal.close-tab".to_owned(),
                Some("Cmd+T".to_owned()),
                ShortcutSettingReason::Conflict {
                    other: C::TerminalNewTab
                }
            ),
            (
                "tree.new-file".to_owned(),
                Some("Cmd+K".to_owned()),
                ShortcutSettingReason::Conflict {
                    other: C::TerminalNewTab
                }
            ),
        ]
    );
}

#[test]
fn 両方を書けばキーを入れ替えられる() {
    let (table, problems) = apply(&[
        text("terminal.new-tab", "Cmd+W"),
        text("terminal.close-tab", "Cmd+T"),
    ]);

    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(key_of(&table, C::TerminalNewTab), "Cmd+W");
    assert_eq!(key_of(&table, C::TerminalCloseTab), "Cmd+T");
}

#[test]
fn 同じコマンドを_2_度書くと両方を既定に戻す() {
    // 引用符の有無だけが違う 2 行は、TOML を読む側で同じ ID の 2 つの行になる(ADR-012 の 2)。
    for second in [
        text("terminal.new-tab", "Cmd+Shift+T"),
        not_text("terminal.new-tab"),
    ] {
        let (table, problems) = apply(&[text("terminal.new-tab", "Cmd+Shift+T"), second]);
        assert_eq!(table, ShortcutTable::default_table());
        assert_eq!(
            summary(&problems),
            vec![(
                "terminal.new-tab".to_owned(),
                None,
                ShortcutSettingReason::DuplicateCommand
            )]
        );
    }
}

/// すべての並べ替え(ヒープのアルゴリズム)。
fn permutations<T: Clone>(items: &[T]) -> Vec<Vec<T>> {
    fn go<T: Clone>(k: usize, items: &mut Vec<T>, out: &mut Vec<Vec<T>>) {
        if k <= 1 {
            out.push(items.clone());
            return;
        }
        go(k - 1, items, out);
        for i in 0..k - 1 {
            if k.is_multiple_of(2) {
                items.swap(i, k - 1);
            } else {
                items.swap(0, k - 1);
            }
            go(k - 1, items, out);
        }
    }
    let mut items = items.to_vec();
    let mut out = Vec::new();
    go(items.len(), &mut items, &mut out);
    out
}

#[test]
fn 結果は書いた順によらない() {
    let overrides = [
        text("terminal.new-tab", "Cmd+K"),
        text("tree.new-file", "Cmd+K"),
        text("terminal.close-tab", "Cmd+T"),
        text("terminal.next-tab", "Cmd+Shift+L"),
        text("tree.open", "Cmd+Shift+L"),
        text("view.toggle-maximize", "Cmd+Shift+M"),
        not_text("edit.select-all"),
    ];
    let all = permutations(&overrides);
    assert_eq!(all.len(), 5040);

    let expected = apply(&overrides);
    for order in &all {
        assert_eq!(apply(order), expected, "{order:?}");
    }
}

#[test]
fn 結果の表は必ず表の検査を通る() {
    let defaults = ShortcutTable::default_table();
    // すべてのコマンドに、既定のキー・ほかのコマンドの既定のキー・新しいキー・読めないキーを順に当てる。
    let pool: Vec<String> = defaults
        .bindings()
        .iter()
        .map(|b| b.accelerator().to_string())
        .chain(["Cmd+K", "Cmd+Shift+K", "Delete", "Cmd+H", "T", "", "Cmd+"].map(str::to_owned))
        .collect();
    for shift in 0..pool.len() {
        let overrides: Vec<ShortcutOverride> = C::ALL
            .iter()
            .enumerate()
            .map(|(i, c)| text(c.as_str(), &pool[(i + shift) % pool.len()]))
            .collect();
        let (table, _) = apply_overrides(&defaults, &overrides);
        assert_eq!(
            ShortcutTable::from_bindings(table.bindings().to_vec()),
            Ok(table.clone()),
            "shift {shift}"
        );
        for binding in table.bindings() {
            assert_eq!(
                binding.receiver(),
                defaults.get(binding.command()).receiver()
            );
        }
    }
}

#[test]
fn 終了のキーは_cmd_を含めば変えられる() {
    let (table, problems) = apply(&[text("app.quit", "Cmd+Shift+Q")]);
    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(key_of(&table, C::AppQuit), "Cmd+Shift+Q");

    assert_eq!(
        single_rejection(C::AppQuit, "Ctrl+Q"),
        ShortcutSettingReason::MenuKeyNeedsCmd
    );
    // ほかのコマンドに Cmd+Q を割り当てると、終了の既定の行と重なる。
    assert_eq!(
        single_rejection(C::TerminalNewTab, "Cmd+Q"),
        ShortcutSettingReason::Conflict { other: C::AppQuit }
    );
}

// ---- 知らせ ----

#[test]
fn 知らせは_1_行で_id_とキーの文字列と理由だけ() {
    let (_, problems) = apply(&[
        text("terminal.new-tab", "Cmd+W"),
        text("tree.new-file", "Cmd+Hyper+N"),
        not_text("tree.open"),
        text("edit.copy", "Cmd+Shift+C"),
        text("nope", "Cmd+J"),
    ]);
    let notice = notice_line(&problems).expect("問題があるので知らせがある");

    assert!(!notice.contains('\n') && !notice.contains('\r'), "{notice}");
    for part in [
        "terminal.new-tab",
        "Cmd+W",
        "terminal.close-tab",
        "tree.new-file",
        "Cmd+Hyper+N",
        "tree.open",
        "edit.copy",
        "nope",
        "Cmd+J",
    ] {
        assert!(notice.contains(part), "{part:?} が無い: {notice}");
    }
    // 理由は種類だけで、読み取りの失敗の文言(元の文字列や要素を引用したもの)を出さない。
    assert!(!notice.contains('"'), "{notice}");
    assert!(!notice.contains("Hyper\""), "{notice}");

    for problem in &problems {
        let line = problem.diagnostic_line();
        assert!(!line.contains('\n'), "{line}");
        assert!(line.contains(problem.id()), "{line}");
        assert!(
            !line.starts_with("[arin:"),
            "頭の文字列は出す側が付ける: {line}"
        );
    }
}

#[test]
fn 知らせの_id_とキーは制御文字を除き_64_文字で切る() {
    assert_eq!(MAX_SHOWN_CHARS, 64);
    let long_id = format!("a\u{7}\n\u{1b}b{}", "あ".repeat(100));
    let long_key = format!("Cmd+\u{0}\t{}", "Z".repeat(100));
    let (_, problems) = apply(&[text(&long_id, "Cmd+J"), text("terminal.new-tab", &long_key)]);

    let id_shown = format!("ab{}", "あ".repeat(62));
    let key_shown = format!("Cmd+{}", "Z".repeat(60));
    assert_eq!(problems[0].id(), "terminal.new-tab");
    assert_eq!(problems[0].key(), Some(key_shown.as_str()));
    assert_eq!(problems[1].id(), id_shown);

    let notice = notice_line(&problems).expect("知らせがある");
    assert!(notice.contains(&id_shown));
    assert!(!notice.contains(&format!("{id_shown}あ")));
    assert!(notice.contains(&key_shown));
    assert!(!notice.contains(&format!("{key_shown}Z")));
    assert!(!notice.chars().any(char::is_control), "{notice:?}");
    for problem in &problems {
        assert!(!problem.diagnostic_line().chars().any(char::is_control));
    }
}

#[test]
fn 問題が無ければ知らせは無い() {
    let (_, problems) = apply(&[text("terminal.new-tab", "Cmd+Shift+T")]);
    assert!(problems.is_empty());
    assert_eq!(notice_line(&problems), None);
}

// ---- 設定ファイルの見本(計画書 T64c。ADR-012 の 6、AC-K08 の規則の部分) ----

/// 見本の `# "ID" = "キー"` の行のコメントを外した設定の行(書いた順)。
fn uncommented_sample_rows(sample: &str) -> Vec<ShortcutOverride> {
    sample
        .lines()
        .filter_map(|line| line.strip_prefix("# \""))
        .filter_map(|rest| {
            let (id, value) = rest.split_once("\" = \"")?;
            let value = value.strip_suffix('"')?;
            Some(text(id, &value.replace("\\\"", "\"").replace("\\\\", "\\")))
        })
        .collect()
}

#[test]
fn 見本の行のコメントを外しても検査で問題が出ない() {
    let defaults = ShortcutTable::default_table();
    let rows = uncommented_sample_rows(&sample_settings_toml(&defaults));
    assert!(!rows.is_empty(), "見本にコマンドの行が無い");

    // 1 行ずつ外しても、全部まとめて外しても、問題は無く既定の表のまま。
    for row in &rows {
        let (table, problems) = apply_overrides(&defaults, std::slice::from_ref(row));
        assert!(problems.is_empty(), "{} の行で問題: {problems:?}", row.id);
        assert_eq!(table, defaults, "{} の行で表が変わった", row.id);
    }
    let (table, problems) = apply_overrides(&defaults, &rows);
    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(table, defaults);
}

#[test]
fn 知らない_id_が大量でもすぐ終わり知らせは上限の数とほかの件数だけ() {
    // 1 MiB の設定ファイルに収まる規模(レビューの指摘: 並びを毎回探すと起動が止まる)。
    let count = 200_000;
    let rows: Vec<ShortcutOverride> = (0..count).map(|i| text(&format!("x{i}"), "1")).collect();

    let started = std::time::Instant::now();
    let (table, problems) = apply(&rows);
    let elapsed = started.elapsed();

    assert_eq!(table, ShortcutTable::default_table());
    assert_eq!(problems.len(), count);
    assert!(elapsed < std::time::Duration::from_secs(5), "{elapsed:?}");
    let notice = notice_line(&problems).expect("知らせがある");
    assert!(notice.contains(&format!("{count} 行")), "{notice}");
    assert!(
        notice.ends_with(&format!("、ほか {} 件", count - MAX_NOTICE_ITEMS)),
        "{notice}"
    );
    assert!(
        notice.contains(&format!("x{} ", MAX_NOTICE_ITEMS - 1)),
        "{notice}"
    );
    assert!(
        !notice.contains(&format!("x{MAX_NOTICE_ITEMS} ")),
        "{notice}"
    );
}

#[test]
fn 同じ知らない_id_は_1_つにまとめる() {
    let (_, problems) = apply(&[
        text("nope", "Cmd+T"),
        text("nope", "Cmd+Y"),
        text("other", "Cmd+T"),
    ]);
    let ids: Vec<&str> = problems.iter().map(ShortcutSettingProblem::id).collect();
    assert_eq!(ids, vec!["nope", "other"]);
}

#[test]
fn 知らせから文字の向きと行を変える文字を除く() {
    let (_, problems) = apply(&[text("a\u{202E}b\u{2066}c\u{200F}d\u{2028}e", "x\u{202D}y")]);
    let notice = notice_line(&problems).expect("知らせがある");
    assert!(notice.contains("abcde"), "{notice}");
    for mark in ['\u{202E}', '\u{2066}', '\u{200F}', '\u{2028}', '\u{202D}'] {
        assert!(!notice.contains(mark), "{mark:?} が残っている: {notice:?}");
    }
}
