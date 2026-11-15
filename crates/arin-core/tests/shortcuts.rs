//! ショートカットの表とキーの文字列(計画書 T38d。前提 (e)【要件の追加(本人。2026-11-15)】、【仮定】21・22)。

use arin_core::shortcuts::key::{KeyChord, KeyParseError, Modifier};
use arin_core::shortcuts::table::{
    ShortcutBinding, ShortcutCommandId, ShortcutReceiver, ShortcutTable, ShortcutTableProblem,
};

use ShortcutReceiver::{EditItem, MenuItem, Screen};

fn chord(s: &str) -> KeyChord {
    KeyChord::parse(s).unwrap_or_else(|e| panic!("{s:?} を読めなかった: {e}"))
}

/// 既定の表の期待値(コマンドの ID の文字列・キー・受け口。表の順)。
///
/// キーは PRD §6.4 と M2 のメニュー(`src-tauri/src/menu.rs`)のとおり。受け口は【仮定】21。
const EXPECTED_DEFAULTS: [(&str, &str, ShortcutReceiver); 18] = [
    ("terminal.new-tab", "Cmd+T", MenuItem),
    ("terminal.close-tab", "Cmd+W", MenuItem),
    ("terminal.previous-tab", "Cmd+Shift+[", MenuItem),
    ("terminal.next-tab", "Cmd+Shift+]", MenuItem),
    ("edit.select-all", "Cmd+A", MenuItem),
    ("app.quit", "Cmd+Q", MenuItem),
    ("edit.copy", "Cmd+C", EditItem),
    ("edit.cut", "Cmd+X", EditItem),
    ("edit.paste", "Cmd+V", EditItem),
    ("tree.open", "Cmd+O", Screen),
    ("tree.reveal-in-finder", "Cmd+Alt+R", Screen),
    ("tree.insert-path", "Cmd+Enter", Screen),
    ("tree.copy-relative-path", "Cmd+Shift+Alt+C", Screen),
    ("tree.copy-absolute-path", "Cmd+Alt+C", Screen),
    ("tree.rename", "Enter", Screen),
    ("tree.trash", "Cmd+Backspace", Screen),
    ("tree.new-file", "Cmd+N", Screen),
    ("tree.new-folder", "Cmd+Shift+N", Screen),
];

// ---- 既定の表 ----

#[test]
fn すべてのコマンドの_id_が既定のキーを持つ() {
    let table = ShortcutTable::default_table();

    assert_eq!(table.bindings().len(), ShortcutCommandId::ALL.len());
    for id in ShortcutCommandId::ALL {
        let binding = table.get(id);
        assert_eq!(binding.command(), id);
        assert!(
            !binding.accelerator().to_string().is_empty(),
            "{id} のキーが空"
        );
    }
}

#[test]
fn 同じフォーカスで受けるキーに重なりが無い() {
    // メニューの項目と編集の項目は OS がフォーカスによらず受けるので、画面で受けるキーとも重なってはいけない。
    // そのため受け口によらず、表のどの 2 行も同じキーを持たない。
    let table = ShortcutTable::default_table();
    let bindings = table.bindings();
    for (i, a) in bindings.iter().enumerate() {
        for b in &bindings[i + 1..] {
            assert_ne!(
                a.accelerator(),
                b.accelerator(),
                "{} と {} のキーが重なる",
                a.command(),
                b.command()
            );
        }
    }
    // 表を作り直しても問題が出ない(既定の表は検査を通る)。
    assert!(ShortcutTable::from_bindings(bindings.to_vec()).is_ok());
}

#[test]
fn 既定のキーは_prd_の表と_m2_のメニューのとおり() {
    let table = ShortcutTable::default_table();
    let actual: Vec<(String, String, ShortcutReceiver)> = table
        .bindings()
        .iter()
        .map(|b| {
            (
                b.command().as_str().to_owned(),
                b.accelerator().to_string(),
                b.receiver(),
            )
        })
        .collect();
    let expected: Vec<(String, String, ShortcutReceiver)> = EXPECTED_DEFAULTS
        .iter()
        .map(|(id, key, receiver)| ((*id).to_owned(), (*key).to_owned(), *receiver))
        .collect();

    assert_eq!(actual, expected);
}

#[test]
fn id_の文字列は_m2_のメニューの_id_と同じ() {
    // M2 の `menu.rs` の項目の ID(`on_menu_event` に届く文字列)。
    let m2_menu_ids = [
        (ShortcutCommandId::TerminalNewTab, "terminal.new-tab"),
        (ShortcutCommandId::TerminalCloseTab, "terminal.close-tab"),
        (
            ShortcutCommandId::TerminalPreviousTab,
            "terminal.previous-tab",
        ),
        (ShortcutCommandId::TerminalNextTab, "terminal.next-tab"),
        (ShortcutCommandId::EditSelectAll, "edit.select-all"),
        (ShortcutCommandId::AppQuit, "app.quit"),
    ];
    for (id, text) in m2_menu_ids {
        assert_eq!(id.as_str(), text);
        assert_eq!(id.to_string(), text);
        assert_eq!(ShortcutCommandId::from_id(text), Some(id));
        assert_eq!(
            serde_json::to_value(id).expect("ID を JSON にできなかった"),
            serde_json::Value::String(text.to_owned())
        );
    }
    // すべての ID が文字列と行き来でき、重ならない。
    let mut seen = std::collections::BTreeSet::new();
    for id in ShortcutCommandId::ALL {
        assert_eq!(ShortcutCommandId::from_id(id.as_str()), Some(id));
        assert!(seen.insert(id.as_str()), "{id} が重なる");
    }
    assert_eq!(ShortcutCommandId::from_id("tree.unknown"), None);
    assert_eq!(ShortcutCommandId::from_id("TERMINAL.NEW-TAB"), None);
}

#[test]
fn 受け口ごとに行を引ける() {
    let table = ShortcutTable::default_table();
    let ids = |receiver| -> Vec<&str> {
        table
            .by_receiver(receiver)
            .map(|b| b.command().as_str())
            .collect()
    };

    assert_eq!(
        ids(MenuItem),
        [
            "terminal.new-tab",
            "terminal.close-tab",
            "terminal.previous-tab",
            "terminal.next-tab",
            "edit.select-all",
            "app.quit"
        ]
    );
    assert_eq!(ids(EditItem), ["edit.copy", "edit.cut", "edit.paste"]);
    assert_eq!(ids(Screen).len(), 9);
}

// ---- キーの文字列 ----

#[test]
fn キーの文字列を修飾キーとキーに分けて読める() {
    let c = chord("Cmd+Shift+N");
    assert!(c.modifiers().contains(Modifier::Cmd));
    assert!(c.modifiers().contains(Modifier::Shift));
    assert!(!c.modifiers().contains(Modifier::Alt));
    assert!(!c.modifiers().contains(Modifier::Ctrl));
    assert_eq!(c.key().as_str(), "N");
    assert_eq!(
        c.modifiers().iter().collect::<Vec<_>>(),
        [Modifier::Cmd, Modifier::Shift]
    );

    // 修飾キーの無いキー(ツリーの名称変更の Enter)。
    let enter = chord("Enter");
    assert!(enter.modifiers().is_empty());
    assert_eq!(enter.key().as_str(), "Enter");

    // 表の書式(キー)の一覧。
    for (input, key) in [
        ("Cmd+Shift+[", "["),
        ("Cmd+Shift+]", "]"),
        ("Cmd+Backspace", "Backspace"),
        ("Cmd+Enter", "Enter"),
        ("Cmd+1", "1"),
        ("Cmd+Delete", "Delete"),
        ("Escape", "Escape"),
        ("Ctrl+Tab", "Tab"),
        ("Cmd+Space", "Space"),
        ("Cmd+ArrowUp", "ArrowUp"),
        ("Cmd+\\", "\\"),
        ("Cmd+`", "`"),
    ] {
        assert_eq!(chord(input).key().as_str(), key, "{input:?}");
    }
}

#[test]
fn 修飾キーの順が違っても同じキーとみなす() {
    let canonical = chord("Cmd+Shift+Alt+C");
    for variant in [
        "Shift+Cmd+Alt+C",
        "Alt+Shift+Cmd+C",
        "cmd+shift+alt+c",
        "Command+Shift+Option+C",
        "Super+SHIFT+alt+c",
    ] {
        let parsed = chord(variant);
        assert_eq!(parsed, canonical, "{variant:?}");
        // 書き出すと正規の書式(修飾キーは Cmd・Shift・Alt・Ctrl の順、文字は大文字)になる。
        assert_eq!(parsed.to_string(), "Cmd+Shift+Alt+C", "{variant:?}");
    }
    assert_eq!(chord("Control+Cmd+k").to_string(), "Cmd+Ctrl+K");
    assert_eq!(chord("esc").to_string(), "Escape");
    assert_ne!(chord("Cmd+C"), chord("Cmd+Shift+C"));
}

#[test]
fn 既定の表のキーの文字列は正規の書式のまま往復する() {
    for (_, key, _) in EXPECTED_DEFAULTS {
        assert_eq!(chord(key).to_string(), key);
        assert_eq!(
            key.parse::<KeyChord>().map(|c| c.to_string()),
            Ok(key.to_owned())
        );
    }
}

#[test]
fn 読めない文字列は理由つきで拒否する() {
    use KeyParseError as E;
    let cases: [(&str, KeyParseError); 16] = [
        ("", E::Empty),
        ("Cmd+", E::EmptyPart("Cmd+".into())),
        ("+A", E::EmptyPart("+A".into())),
        ("Cmd++", E::EmptyPart("Cmd++".into())),
        ("Cmd++A", E::EmptyPart("Cmd++A".into())),
        (
            "Cmd+Foo",
            E::UnknownPart {
                accelerator: "Cmd+Foo".into(),
                part: "Foo".into(),
            },
        ),
        (
            "Hyper+A",
            E::UnknownPart {
                accelerator: "Hyper+A".into(),
                part: "Hyper".into(),
            },
        ),
        (
            "Cmd+あ",
            E::UnknownPart {
                accelerator: "Cmd+あ".into(),
                part: "あ".into(),
            },
        ),
        // 前後の空白は許さない(設定の値の書き間違いを黙って直さない)。
        (
            "Cmd + A",
            E::UnknownPart {
                accelerator: "Cmd + A".into(),
                part: "Cmd ".into(),
            },
        ),
        (
            " Cmd+A",
            E::UnknownPart {
                accelerator: " Cmd+A".into(),
                part: " Cmd".into(),
            },
        ),
        // 修飾キーだけ。
        ("Cmd", E::NoKey("Cmd".into())),
        ("Cmd+Shift", E::NoKey("Cmd+Shift".into())),
        // 同じ修飾キーの重なり(別名どうしも同じとみなす)。
        (
            "Cmd+Cmd+A",
            E::DuplicateModifier {
                accelerator: "Cmd+Cmd+A".into(),
                modifier: Modifier::Cmd,
            },
        ),
        (
            "Alt+Option+A",
            E::DuplicateModifier {
                accelerator: "Alt+Option+A".into(),
                modifier: Modifier::Alt,
            },
        ),
        // キーが 2 つ。
        (
            "Cmd+A+B",
            E::MultipleKeys {
                accelerator: "Cmd+A+B".into(),
            },
        ),
        // キーの後ろに修飾キー(muda はキーを最後の要素として読むので、同じ書式にそろえる)。
        (
            "A+Cmd",
            E::KeyNotLast {
                accelerator: "A+Cmd".into(),
            },
        ),
    ];
    for (input, expected) in cases {
        let actual = KeyChord::parse(input);
        assert_eq!(actual, Err(expected), "{input:?}");
        // 理由は文言にも出る(M4.5 で画面に出す)。
        let message = actual.expect_err("失敗するはず").to_string();
        assert!(!message.is_empty());
    }
}

#[test]
fn キーの文字列は_json_では文字列で_読めない値は拒否する() {
    let c = chord("Shift+Cmd+N");
    assert_eq!(
        serde_json::to_value(c).expect("JSON にできなかった"),
        serde_json::json!("Cmd+Shift+N")
    );
    let back: KeyChord = serde_json::from_str("\"cmd+shift+n\"").expect("読めなかった");
    assert_eq!(back, c);
    assert!(serde_json::from_str::<KeyChord>("\"Cmd+Cmd+N\"").is_err());
}

// ---- 表の検査(M4.5 で設定の値とまとめるときの土台) ----

fn default_bindings() -> Vec<ShortcutBinding> {
    ShortcutTable::default_table().bindings().to_vec()
}

fn replace(bindings: &mut [ShortcutBinding], id: ShortcutCommandId, key: &str) {
    let row = bindings
        .iter_mut()
        .find(|b| b.command() == id)
        .expect("行が無い");
    *row = ShortcutBinding::new(id, chord(key), row.receiver());
}

#[test]
fn 同じキーの二重の割り当てを検出する() {
    let mut bindings = default_bindings();
    // 画面で受けるキーを、メニューの項目のキーと同じにする(修飾キーの順が違っても同じキー)。
    replace(&mut bindings, ShortcutCommandId::TreeNewFile, "Cmd+T");
    replace(&mut bindings, ShortcutCommandId::TreeTrash, "Shift+Cmd+]");

    let problems = ShortcutTable::from_bindings(bindings).expect_err("重なりを見逃した");
    assert_eq!(
        problems,
        vec![
            ShortcutTableProblem::Conflict {
                first: ShortcutCommandId::TerminalNewTab,
                second: ShortcutCommandId::TreeNewFile,
                accelerator: chord("Cmd+T"),
            },
            ShortcutTableProblem::Conflict {
                first: ShortcutCommandId::TerminalNextTab,
                second: ShortcutCommandId::TreeTrash,
                accelerator: chord("Cmd+Shift+]"),
            },
        ]
    );
    assert!(problems[0].to_string().contains("tree.new-file"));
}

#[test]
fn 足りない行と同じコマンドの二重の行を検出する() {
    let mut bindings = default_bindings();
    bindings.retain(|b| b.command() != ShortcutCommandId::TreeRename);
    let open = bindings
        .iter()
        .find(|b| b.command() == ShortcutCommandId::TreeOpen)
        .cloned()
        .expect("行が無い");
    bindings.push(ShortcutBinding::new(
        ShortcutCommandId::TreeOpen,
        chord("Cmd+Shift+O"),
        open.receiver(),
    ));

    let problems = ShortcutTable::from_bindings(bindings).expect_err("問題を見逃した");
    assert_eq!(
        problems,
        vec![
            ShortcutTableProblem::DuplicateCommand(ShortcutCommandId::TreeOpen),
            ShortcutTableProblem::Missing(ShortcutCommandId::TreeRename),
        ]
    );
}

#[test]
fn 表の順は渡した行の順によらずコマンドの_id_の順() {
    let mut reversed = default_bindings();
    reversed.reverse();
    let table = ShortcutTable::from_bindings(reversed).expect("既定の行で作れない");

    assert_eq!(table, ShortcutTable::default_table());
    assert_eq!(table.to_json(), ShortcutTable::default_table().to_json());
}

#[test]
fn 受け口を_1_行で替えられる() {
    // 【仮定】21: WKWebView が画面へ渡さないキーがあれば、その行の受け口だけをメニューの項目に替える。
    let mut bindings = default_bindings();
    let row = bindings
        .iter_mut()
        .find(|b| b.command() == ShortcutCommandId::TreeTrash)
        .expect("行が無い");
    *row = ShortcutBinding::new(row.command(), row.accelerator(), MenuItem);

    let table = ShortcutTable::from_bindings(bindings).expect("作れない");
    assert_eq!(table.get(ShortcutCommandId::TreeTrash).receiver(), MenuItem);
    assert_eq!(table.by_receiver(MenuItem).count(), 7);
}

#[test]
fn 表の_json_の形() {
    let json: serde_json::Value =
        serde_json::from_str(&ShortcutTable::default_table().to_json()).expect("JSON でない");

    let rows = json.as_array().expect("配列でない");
    assert_eq!(rows.len(), EXPECTED_DEFAULTS.len());
    for (row, (id, key, receiver)) in rows.iter().zip(EXPECTED_DEFAULTS) {
        let receiver = match receiver {
            MenuItem => "menu-item",
            EditItem => "edit-item",
            Screen => "screen",
        };
        assert_eq!(
            row,
            &serde_json::json!({ "command": id, "accelerator": key, "receiver": receiver })
        );
    }

    // 文字列としても決定的: 行は表の順、項目は command・accelerator・receiver の順、2 文字の字下げ、末尾に改行 1 つ。
    let rows_text: Vec<String> = EXPECTED_DEFAULTS
        .iter()
        .map(|(id, key, receiver)| {
            let receiver = match receiver {
                MenuItem => "menu-item",
                EditItem => "edit-item",
                Screen => "screen",
            };
            let key = key.replace('\\', "\\\\");
            format!(
                "  {{\n    \"command\": \"{id}\",\n    \"accelerator\": \"{key}\",\n    \"receiver\": \"{receiver}\"\n  }}"
            )
        })
        .collect();
    let expected = format!("[\n{}\n]\n", rows_text.join(",\n"));
    assert_eq!(ShortcutTable::default_table().to_json(), expected);
}
