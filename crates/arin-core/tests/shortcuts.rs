//! ショートカットの表とキーの文字列(計画書 T38d。前提 (e)【要件の追加(本人。2026-11-15)】、【仮定】21・22)。

use arin_core::shortcuts::key::{KeyChord, KeyParseError, Modifier};
use arin_core::shortcuts::reserved::RESERVED_KEYS;
use arin_core::shortcuts::sample::sample_settings_toml;
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
/// 最後の 4 行は M4 の表示の操作(計画書 M4 の前提 (f)・【仮定】9、ADR-011 の 6。T47a)。
/// 末尾の 9 行は M4.7 のタイルの番号キー(ADR-014 の 4)。
/// `app.open-settings` は M4.5 で足した行(計画書 T64c。ADR-012 の 6)。M4.6 で意味を「設定…」(設定画面を開く)に替えた
/// (ADR-013 の 6。ID・キー・受け口はそのまま)。
const EXPECTED_DEFAULTS: [(&str, &str, ShortcutReceiver); 32] = [
    ("terminal.new-tab", "Cmd+T", MenuItem),
    ("terminal.close-tab", "Cmd+W", MenuItem),
    ("terminal.previous-tab", "Cmd+Shift+[", MenuItem),
    ("terminal.next-tab", "Cmd+Shift+]", MenuItem),
    ("edit.select-all", "Cmd+A", MenuItem),
    ("app.quit", "Cmd+Q", MenuItem),
    ("app.open-settings", "Cmd+,", MenuItem),
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
    ("view.toggle-display-mode", "Cmd+Shift+G", MenuItem),
    ("view.previous-page", "Cmd+Alt+ArrowLeft", MenuItem),
    ("view.next-page", "Cmd+Alt+ArrowRight", MenuItem),
    ("view.toggle-maximize", "Cmd+Shift+Enter", MenuItem),
    // M4.7 のタイルの番号キー(ADR-014 の 4)。
    ("view.focus-tile-1", "Cmd+1", MenuItem),
    ("view.focus-tile-2", "Cmd+2", MenuItem),
    ("view.focus-tile-3", "Cmd+3", MenuItem),
    ("view.focus-tile-4", "Cmd+4", MenuItem),
    ("view.focus-tile-5", "Cmd+5", MenuItem),
    ("view.focus-tile-6", "Cmd+6", MenuItem),
    ("view.focus-tile-7", "Cmd+7", MenuItem),
    ("view.focus-tile-8", "Cmd+8", MenuItem),
    ("view.focus-tile-9", "Cmd+9", MenuItem),
];

/// 表示の 4 行(コマンドの ID・ID の文字列・既定のキー・受け口)。
const VIEW_ROWS: [(ShortcutCommandId, &str, &str); 4] = [
    (
        ShortcutCommandId::ViewToggleDisplayMode,
        "view.toggle-display-mode",
        "Cmd+Shift+G",
    ),
    (
        ShortcutCommandId::ViewPreviousPage,
        "view.previous-page",
        "Cmd+Alt+ArrowLeft",
    ),
    (
        ShortcutCommandId::ViewNextPage,
        "view.next-page",
        "Cmd+Alt+ArrowRight",
    ),
    (
        ShortcutCommandId::ViewToggleMaximize,
        "view.toggle-maximize",
        "Cmd+Shift+Enter",
    ),
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
fn 表示の_4_行の既定のキーと受け口() {
    // ⇧⌘G・⌥⌘←・⌥⌘→・⇧⌘↩。受け口はどれもメニューの項目(メニューバーの「表示」は T47c で足す)。
    let table = ShortcutTable::default_table();
    for (id, _, key) in VIEW_ROWS {
        let binding = table.get(id);
        assert_eq!(binding.accelerator(), chord(key), "{id}");
        assert_eq!(binding.accelerator().to_string(), key, "{id}");
        assert_eq!(binding.receiver(), MenuItem, "{id}");
    }
    // 表示の 4 行がこの順で並ぶ(ID の順)。後ろには M4.7 のタイルの番号キーの 9 行が続く(ADR-014 の 4)。
    let len = table.bindings().len();
    let tail: Vec<ShortcutCommandId> = table.bindings()[len - 13..len - 9]
        .iter()
        .map(ShortcutBinding::command)
        .collect();
    assert_eq!(tail, VIEW_ROWS.map(|(id, _, _)| id));
    // ⇧ や ⌥ の無い似たキーとは別のキー(⇧⌘↩ は ⌘↩ のパスの挿入と重ならない。ADR-011 の 6)。
    assert_ne!(
        table
            .get(ShortcutCommandId::ViewToggleMaximize)
            .accelerator(),
        table.get(ShortcutCommandId::TreeInsertPath).accelerator()
    );
    assert_ne!(chord("Cmd+Shift+G"), chord("Cmd+G"));
    assert_ne!(chord("Cmd+Alt+ArrowLeft"), chord("Cmd+ArrowLeft"));
}

#[test]
fn 表示の_4_行の_id_の文字列() {
    for (id, text, _) in VIEW_ROWS {
        assert_eq!(id.as_str(), text);
        assert_eq!(id.to_string(), text);
        assert_eq!(ShortcutCommandId::from_id(text), Some(id));
        assert_eq!(
            serde_json::to_value(id).expect("ID を JSON にできなかった"),
            serde_json::Value::String(text.to_owned())
        );
        assert_eq!(
            serde_json::from_value::<ShortcutCommandId>(serde_json::Value::String(text.to_owned()))
                .expect("ID を JSON から読めなかった"),
            id
        );
        assert!(ShortcutCommandId::ALL.contains(&id), "{id} が ALL に無い");
    }
    assert_eq!(ShortcutCommandId::ALL.len(), 32);
}

#[test]
fn 設定の既定は_cmd_カンマで受け口はメニューの項目() {
    // M4.6(ADR-013 の 6、【要確認】2 の A)で、この行の意味を「設定ファイルを開く」から「設定画面を開く」(メニューの「設定…」)に
    // 替えた。ID・キー・受け口は M4.5(ADR-012 の 6)のまま変えないので、期待は前のテスト
    // (`設定ファイルを開くの既定は_cmd_カンマで受け口はメニューの項目`)と同じ。「設定ファイルを開く…」は表の外の項目になった。
    let id = ShortcutCommandId::AppOpenSettings;
    assert_eq!(id.as_str(), "app.open-settings");
    assert_eq!(id.to_string(), "app.open-settings");
    assert_eq!(ShortcutCommandId::from_id("app.open-settings"), Some(id));
    assert_eq!(
        serde_json::to_value(id).expect("ID を JSON にできなかった"),
        serde_json::Value::String("app.open-settings".to_owned())
    );
    assert!(ShortcutCommandId::ALL.contains(&id));

    let table = ShortcutTable::default_table();
    let binding = table.get(id);
    assert_eq!(binding.accelerator(), chord("Cmd+,"));
    assert_eq!(binding.accelerator().to_string(), "Cmd+,");
    assert_eq!(binding.receiver(), MenuItem);
    // 表の順では終了のすぐ後ろ(アプリ名のメニューの行どうしで並べる)。
    let ids: Vec<ShortcutCommandId> = table
        .bindings()
        .iter()
        .map(ShortcutBinding::command)
        .collect();
    let quit = ids
        .iter()
        .position(|&c| c == ShortcutCommandId::AppQuit)
        .expect("終了の行が無い");
    assert_eq!(ids[quit + 1], id);
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
            "app.quit",
            "app.open-settings",
            "view.toggle-display-mode",
            "view.previous-page",
            "view.next-page",
            "view.toggle-maximize",
            "view.focus-tile-1",
            "view.focus-tile-2",
            "view.focus-tile-3",
            "view.focus-tile-4",
            "view.focus-tile-5",
            "view.focus-tile-6",
            "view.focus-tile-7",
            "view.focus-tile-8",
            "view.focus-tile-9"
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
        ("Cmd+Alt+ArrowLeft", "ArrowLeft"),
        ("Cmd+Alt+ArrowRight", "ArrowRight"),
        ("Cmd+Shift+Enter", "Enter"),
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
    // 既定の 20 行(M4.7 でタイルの番号キー 9 行を足した)+ 替えた 1 行。
    assert_eq!(table.by_receiver(MenuItem).count(), 21);
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

// ---- 設定ファイルの見本(計画書 T64c。ADR-012 の 6) ----

/// 見本の行のうち、コメントを外せば設定の行になる `# "ID" = "キー"` の行(ID とキーの組。書いた順)。
fn sample_rows(sample: &str) -> Vec<(String, String)> {
    sample
        .lines()
        .filter_map(|line| line.strip_prefix("# \""))
        .filter_map(|rest| {
            let (id, value) = rest.split_once("\" = \"")?;
            let value = value.strip_suffix('"')?;
            Some((
                id.to_owned(),
                value.replace("\\\"", "\"").replace("\\\\", "\\"),
            ))
        })
        .collect()
}

#[test]
fn 見本はすべてのコマンドの_id_と既定のキーをコメントで並べる() {
    let table = ShortcutTable::default_table();
    let sample = sample_settings_toml(&table);

    // 変えられるコマンド(受け口が「編集」の標準の項目でないもの)は、表の順に `# "ID" = "既定のキー"` の行で並ぶ。
    let expected: Vec<(String, String)> = table
        .bindings()
        .iter()
        .filter(|b| b.receiver() != EditItem)
        .map(|b| (b.command().as_str().to_owned(), b.accelerator().to_string()))
        .collect();
    assert_eq!(sample_rows(&sample), expected);
    assert_eq!(expected.len(), ShortcutCommandId::ALL.len() - 3);

    // 変えられないコマンドも、ID と既定のキーがコメントの行に書いてある(コメントを外しても設定の行にならない形で)。
    for binding in table.by_receiver(EditItem) {
        let id = binding.command().as_str();
        let key = binding.accelerator().to_string();
        assert!(
            sample
                .lines()
                .any(|line| line.starts_with('#') && line.contains(id) && line.contains(&key)),
            "{id} と {key} が見本に無い"
        );
    }
    // 設定(設定画面を開く。ADR-013 の 6)の行もある。並びは M4.5 から変わらない。
    assert!(sample.contains("# \"app.open-settings\" = \"Cmd+,\""));
}

#[test]
fn 見本に文字の大きさは設定画面で変える旨がある() {
    // ADR-013 の 6: `[terminal]` に `font_size` を書いても効かないことが分かるように、先頭の説明に 1 行足す。
    let sample = sample_settings_toml(&ShortcutTable::default_table());
    let heading = sample
        .lines()
        .position(|line| line == "[shortcuts]")
        .expect("見出しが無い");
    let intro: Vec<&str> = sample.lines().take(heading).collect();
    let line = "# 文字の大きさはこのファイルではなく、アプリの「設定…」(⌘,)で変えます";
    let at = intro
        .iter()
        .position(|l| *l == line)
        .unwrap_or_else(|| panic!("見本の説明に文字の大きさの行が無い: {intro:?}"));
    // 先頭の見出しの行のすぐ下(説明の最初の段落)。
    assert_eq!(at, 1, "{intro:?}");
    assert_eq!(intro[0], "# Arin の設定ファイル(settings.toml)");
}

#[test]
fn 表の行は_32() {
    // ADR-013 の 6(【要確認】2 の A): ⌘, を設定画面へ移しても、表の行は足さない(「設定ファイルを開く…」は表の外の項目)。
    // ADR-014 の 4: タイルの番号キー 9 行を足して 23 → 32 行。
    assert_eq!(ShortcutCommandId::ALL.len(), 32);
    let table = ShortcutTable::default_table();
    assert_eq!(table.bindings().len(), 32);
    assert!(
        ShortcutCommandId::from_id("app.open-settings-file").is_none(),
        "表の外の項目の ID が表にある"
    );
    assert_eq!(
        sample_rows(&sample_settings_toml(&table)).len(),
        32 - 3,
        "見本の行の数が変わった"
    );
}

#[test]
fn タイルの番号キーは_cmd_1_から_9_で受け口はメニューの項目() {
    let table = ShortcutTable::default_table();
    let sample = sample_settings_toml(&table);
    for n in 1..=9usize {
        let id = format!("view.focus-tile-{n}");
        let command = ShortcutCommandId::from_id(&id).unwrap_or_else(|| panic!("{id} が無い"));
        assert_eq!(command.tile_position(), Some(n), "{id}");
        let binding = table.get(command);
        assert_eq!(binding.accelerator(), chord(&format!("Cmd+{n}")), "{id}");
        assert_eq!(binding.receiver(), MenuItem, "{id}");
        // 見本に 1 行ずつある
        let row = format!("# \"{id}\" = \"Cmd+{n}\"");
        assert!(sample.lines().any(|l| l == row), "見本に {row} が無い");
        // 予約のキーではない(設定ファイルで替えられる)
        assert!(
            !arin_core::shortcuts::reserved::is_reserved(chord(&format!("Cmd+{n}"))),
            "Cmd+{n} が予約のキー"
        );
    }
    // 番号キー以外は番号を持たない
    let others = ShortcutCommandId::ALL
        .into_iter()
        .filter(|c| c.tile_position().is_some())
        .count();
    assert_eq!(others, 9);
    assert_eq!(ShortcutCommandId::ViewNextPage.tile_position(), None);
}

#[test]
fn 見本の行はコメントか_shortcuts_の見出しだけ() {
    let sample = sample_settings_toml(&ShortcutTable::default_table());
    assert!(sample.ends_with('\n'), "末尾に改行が無い");
    assert!(!sample.ends_with("\n\n"), "末尾に空の行がある");

    let headings: Vec<&str> = sample
        .lines()
        .filter(|line| !line.starts_with('#'))
        .collect();
    assert_eq!(
        headings,
        ["[shortcuts]"],
        "コメントでも見出しでもない行がある"
    );
    // 見出しより前は説明、後ろはコマンドの行(説明の後ろに置く)。
    let heading = sample
        .lines()
        .position(|line| line == "[shortcuts]")
        .expect("見出しが無い");
    let first_row = sample
        .lines()
        .position(|line| line.starts_with("# \""))
        .expect("コマンドの行が無い");
    assert!(heading < first_row);
    // TOML のコメントに置けない制御文字(タブ以外)を含まない。
    assert!(
        !sample
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t'),
        "制御文字がある"
    );
}

#[test]
fn 見本に変えられないコマンドと予約のキーが書いてある() {
    let sample = sample_settings_toml(&ShortcutTable::default_table());
    let comments: String = sample
        .lines()
        .filter(|line| line.starts_with('#') && !line.starts_with("# \""))
        .collect::<Vec<_>>()
        .join("\n");

    for id in ["edit.cut", "edit.copy", "edit.paste"] {
        assert!(
            comments.contains(id),
            "変えられないコマンド {id} が説明に無い"
        );
    }
    assert!(comments.contains("変えられません"));
    for key in RESERVED_KEYS {
        assert!(comments.contains(key), "予約のキー {key} が説明に無い");
    }
    // 書き方の決まり・効かないときの確かめ方・リンクの先の変更は拾えないこと。
    for needle in [
        "[shortcuts]",
        "引用符",
        "既定",
        "効かない",
        "システム設定",
        "シンボリックリンク",
    ] {
        assert!(comments.contains(needle), "説明に {needle:?} が無い");
    }
}
