//! いま見ているかの判定(`AttentionPolicy`、`Workspace::attended_terminals`)とアテンションの印
//! (`Workspace::note_attention`、写しの `attention`・`attentionPages`)。
//! FR-055、設計書 §7.1「アテンション」の 2・3・5、§15.2 #10 の A、ADR-011 の 7、計画書 T48b(【仮定】13・14)。
//!
//! 規則: いま見ているのはフォーカス中のターミナル 1 枚(タブ表示=表示中のタブ、タイル表示=フォーカス中のタイル、
//! 拡大中=拡大した 1 枚)。タイル表示で同じページに見えているだけのタイルには印が立ち、ページを送って見えただけでは消えない。
//!
//! 不変条件: フォーカス中のターミナルに印は残らない(印を立てる口でも、表示が変わる口のあとでも)。
//! 無作為な操作の列での確かめは `workspace_display.rs` の `どの操作の列のあとも不変条件が成り立つ`。

use std::path::PathBuf;

use arin_core::layout::tiles::DisplayMode;
use arin_core::persistence::codec::PersistedState;
use arin_core::ports::ChildExit;
use arin_core::project::{Project, ProjectColor, ProjectId, RootReference};
use arin_core::terminal::{AttentionPolicy, TerminalId};
use arin_core::workspace::{AddProjectOutcome, RemoveOutcome, RootResolution, Startup, Workspace};

const HOME: &str = "/Users/me";

fn pid(n: u8) -> ProjectId {
    ProjectId::from_random_bytes([n; 16])
}

fn root(n: u8) -> RootReference {
    RootReference::new(PathBuf::from(format!("/Users/me/dev/p{n}")), None)
}

/// プロジェクト 1〜`counts.len()` を並べ、プロジェクト n に `counts[n-1]` 枚のタブを持たせた状態(1 枚以上)。
/// 選択はプロジェクト 1、各プロジェクトの表示中のタブは最初のタブ、表示はタブ表示。
fn workspace(counts: &[usize]) -> Workspace {
    let projects: Vec<(ProjectId, RootReference)> = (1..=counts.len())
        .map(|n| {
            let n = u8::try_from(n).unwrap();
            (pid(n), root(n))
        })
        .collect();
    let mut ws = Workspace::restore(
        "1.2.3",
        PathBuf::from(HOME),
        [("HOME", HOME)],
        Startup {
            state: PersistedState {
                projects: projects
                    .iter()
                    .map(|(id, root)| {
                        Project::restore(id.clone(), root.clone(), None, ProjectColor::Green)
                    })
                    .collect(),
                selected_project_id: Some(pid(1)),
                ..PersistedState::default()
            },
            roots: projects
                .iter()
                .map(|(id, root)| (id.clone(), RootResolution::Found(root.clone())))
                .collect(),
            notice: None,
        },
    );
    for (i, &count) in counts.iter().enumerate() {
        let id = pid(u8::try_from(i + 1).unwrap());
        for _ in 1..count {
            ws.open_terminal(&id).unwrap();
        }
        let first = tabs_of(&ws, &id)[0].clone();
        ws.activate_terminal(&first).unwrap();
    }
    ws.select_project(&pid(1)).unwrap();
    ws
}

fn tabs_of(ws: &Workspace, project: &ProjectId) -> Vec<TerminalId> {
    ws.snapshot()
        .projects
        .into_iter()
        .find(|p| &p.id == project)
        .map(|p| p.terminal_ids)
        .unwrap_or_default()
}

fn all_ids(ws: &Workspace) -> Vec<TerminalId> {
    ws.snapshot().terminals.into_iter().map(|t| t.id).collect()
}

/// 写しで印が立っているターミナル(写しの並びの順)。
fn marked(ws: &Workspace) -> Vec<TerminalId> {
    ws.snapshot()
        .terminals
        .into_iter()
        .filter(|t| t.attention)
        .map(|t| t.id)
        .collect()
}

fn tid(n: u32) -> TerminalId {
    TerminalId::new(format!("terminal-{n}"))
}

/// フォーカス中のターミナルに印が無いこと。
fn assert_no_mark_on_focused(ws: &Workspace) {
    let marks = marked(ws);
    for id in ws.attended_terminals() {
        assert!(
            !marks.contains(&id),
            "フォーカス中の {id:?} に印が残っている"
        );
    }
}

// ---------------------------------------------------------------------------
// 判定(AttentionPolicy)

#[test]
fn いま見ている並びに無いターミナルだけが印の対象() {
    let attended = [tid(1)];

    assert!(!AttentionPolicy::should_mark(&tid(1), &attended));
    assert!(AttentionPolicy::should_mark(&tid(2), &attended));
    assert!(AttentionPolicy::should_mark(&tid(1), &[]));
}

#[test]
fn いま見ているのはどの表示モードでもフォーカス中の_1_枚() {
    // 4 枚とも 1 ページ目。タイル表示でもページの全タイルではなくフォーカス中の 1 枚
    let mut ws = workspace(&[4]);
    let focused = ws.active_terminal().unwrap();
    assert_eq!(ws.attended_terminals(), vec![focused.clone()], "タブ表示");

    ws.set_display_mode(DisplayMode::Tiles);
    assert_eq!(ws.visible_terminals().len(), 4, "画面には 4 枚とも描く");
    assert_eq!(ws.attended_terminals(), vec![focused.clone()], "タイル表示");

    ws.toggle_maximize();
    assert_eq!(ws.attended_terminals(), vec![focused], "拡大中");
}

#[test]
fn タブが無ければいま見ているターミナルは無い() {
    let mut ws = workspace(&[1]);
    let only = ws.active_terminal().unwrap();
    ws.close_terminal(&only).unwrap();

    assert!(ws.attended_terminals().is_empty());
}

// ---------------------------------------------------------------------------
// 印が立つ(タブ表示)

#[test]
fn 表示していないターミナルの契機で印が立つ() {
    // A に 2 枚(表示中は terminal-1)、B に 1 枚。A を選んでいる
    let mut ws = workspace(&[2, 1]);
    let a2 = tabs_of(&ws, &pid(1))[1].clone();
    let b1 = tabs_of(&ws, &pid(2))[0].clone();

    assert!(ws.note_attention(&a2), "同じプロジェクトの隠れたタブ");
    assert!(ws.note_attention(&b1), "選んでいないプロジェクトのタブ");

    assert_eq!(marked(&ws), vec![a2, b1]);
}

#[test]
fn 表示中のターミナルの契機では立たない() {
    // §15.2 #10 の A: アプリが背面でも、画面上で表示中のターミナルはアテンションにしない
    let mut ws = workspace(&[2]);
    let shown = ws.active_terminal().unwrap();

    assert!(!ws.note_attention(&shown));

    assert!(marked(&ws).is_empty());
}

#[test]
fn 立っているところにもう一度来ても変わったと返さない() {
    let mut ws = workspace(&[2]);
    let hidden = tabs_of(&ws, &pid(1))[1].clone();

    assert!(ws.note_attention(&hidden));
    assert!(!ws.note_attention(&hidden));

    assert_eq!(marked(&ws), vec![hidden]);
}

#[test]
fn 知らないターミナルの契機は何もしない() {
    // 読み取りスレッドの契機が、閉じたあとのターミナルについて届いても失敗にしない
    let mut ws = workspace(&[2]);
    let before = ws.snapshot();

    assert!(!ws.note_attention(&tid(99)));

    assert_eq!(ws.snapshot(), before);
}

// ---------------------------------------------------------------------------
// 印が立つ(タイル表示・拡大)

#[test]
fn タイル表示ではフォーカス中のタイルに立たない() {
    let mut ws = workspace(&[10]);
    ws.set_display_mode(DisplayMode::Tiles);
    let focused = ws.active_terminal().unwrap();

    assert!(!ws.note_attention(&focused));

    assert!(marked(&ws).is_empty());
}

#[test]
fn タイル表示で同じページのフォーカスの無いタイルに印が立つ() {
    // FR-055: 9 枚並べて、どのタイルが入力待ちになったかに気づける。10 枚: 1 ページ目に 9 枚
    let mut ws = workspace(&[10]);
    ws.set_display_mode(DisplayMode::Tiles);
    let page = ws.snapshot().display.tile_ids;
    assert_eq!(page.len(), 9);
    let focused = ws.active_terminal().unwrap();
    let others: Vec<TerminalId> = page.into_iter().filter(|id| *id != focused).collect();

    for id in &others {
        assert!(ws.note_attention(id), "同じページのフォーカスの無い {id:?}");
    }

    assert_eq!(marked(&ws), others);
    // 印のあるタイルは表示中のページ(0)にある
    assert_eq!(ws.snapshot().display.attention_pages, vec![0]);
}

#[test]
fn タイル表示で別のプロジェクトの見えているタイルに印が立つ() {
    // A に 2 枚、B に 1 枚。3 枚とも 1 ページ目で、フォーカスは A の 1 枚目
    let mut ws = workspace(&[2, 1]);
    ws.set_display_mode(DisplayMode::Tiles);
    let b1 = tabs_of(&ws, &pid(2))[0].clone();
    assert!(ws.visible_terminals().contains(&b1));

    assert!(ws.note_attention(&b1));

    assert_eq!(marked(&ws), vec![b1]);
}

#[test]
fn 別のページのタイルに立つ() {
    let mut ws = workspace(&[10]);
    ws.set_display_mode(DisplayMode::Tiles);
    let tenth = all_ids(&ws)[9].clone();

    assert!(ws.note_attention(&tenth));

    assert_eq!(marked(&ws), vec![tenth]);
}

#[test]
fn 拡大中は拡大したタイルのほかに立つ() {
    // 3 枚とも 1 ページ目。拡大すると見えるのはフォーカス中の 1 枚だけ
    let mut ws = workspace(&[3]);
    ws.set_display_mode(DisplayMode::Tiles);
    assert!(ws.toggle_maximize());
    let ids = all_ids(&ws);
    let focused = ws.active_terminal().unwrap();

    assert!(!ws.note_attention(&focused), "拡大したタイル");
    for id in ids.iter().filter(|id| **id != focused) {
        assert!(ws.note_attention(id), "同じページの拡大していない {id:?}");
    }

    assert_eq!(marked(&ws).len(), 2);
}

// ---------------------------------------------------------------------------
// 印が消える(そのターミナルがフォーカス中になったとき)

#[test]
fn タイル表示でフォーカスすると消える() {
    let mut ws = workspace(&[10]);
    ws.set_display_mode(DisplayMode::Tiles);
    let ids = all_ids(&ws);
    ws.note_attention(&ids[3]);
    ws.note_attention(&ids[5]);

    assert!(ws.focus_terminal(&ids[3]).unwrap());

    // フォーカスしなかった ids[5] の印は残る
    assert_eq!(marked(&ws), vec![ids[5].clone()]);
}

#[test]
fn ページを送って先頭のタイルにフォーカスが移ると消える() {
    let mut ws = workspace(&[10]);
    ws.set_display_mode(DisplayMode::Tiles);
    let tenth = all_ids(&ws)[9].clone();
    ws.note_attention(&tenth);

    assert!(ws.next_page());

    assert_eq!(ws.active_terminal(), Some(tenth));
    assert!(marked(&ws).is_empty());
}

#[test]
fn ページを送っただけではフォーカスの無いタイルの印は消えない() {
    // 12 枚: 2 ページ目は 10〜12 枚目。送るとフォーカスは 10 枚目へ移り、11・12 枚目は見えるだけ
    let mut ws = workspace(&[12]);
    ws.set_display_mode(DisplayMode::Tiles);
    let ids = all_ids(&ws);
    ws.note_attention(&ids[10]);
    ws.note_attention(&ids[11]);

    assert!(ws.next_page());

    assert!(ws.visible_terminals().contains(&ids[10]));
    assert_eq!(marked(&ws), vec![ids[10].clone(), ids[11].clone()]);
    assert_eq!(ws.snapshot().display.page, 1);
    assert_eq!(ws.snapshot().display.attention_pages, vec![1]);
}

#[test]
fn ページを送っても見えないページの印は残る() {
    // 19 枚: 3 ページ。3 ページ目の印は 2 ページ目へ送っても残る
    let mut ws = workspace(&[19]);
    ws.set_display_mode(DisplayMode::Tiles);
    let last = all_ids(&ws)[18].clone();
    ws.note_attention(&last);

    assert!(ws.next_page());

    assert_eq!(marked(&ws), vec![last]);
}

#[test]
fn タブ表示で選んで表示すると消える() {
    // 同じプロジェクトのタブの切替
    let mut ws = workspace(&[3, 1]);
    let a = tabs_of(&ws, &pid(1));
    ws.note_attention(&a[1]);
    ws.note_attention(&a[2]);

    ws.activate_terminal(&a[1]).unwrap();
    assert_eq!(marked(&ws), vec![a[2].clone()]);

    ws.select_next_terminal();
    assert!(marked(&ws).is_empty());
}

#[test]
fn プロジェクトを切り替えてそのタブが見えると消える() {
    let mut ws = workspace(&[1, 2]);
    let b = tabs_of(&ws, &pid(2));
    ws.note_attention(&b[0]);
    ws.note_attention(&b[1]);

    ws.select_project(&pid(2)).unwrap();

    // B の表示中は最初のタブなので、2 枚目の印は残る
    assert_eq!(marked(&ws), vec![b[1].clone()]);
}

#[test]
fn 登録済みのフォルダを登録し直して選ばれたプロジェクトのタブの印が消える() {
    // 同じフォルダの登録は既存を選ぶ(FR-010)。選択が変わる口なので、見えたタブの印を消す
    let mut ws = workspace(&[1, 1]);
    let b1 = tabs_of(&ws, &pid(2))[0].clone();
    ws.note_attention(&b1);

    let outcome = ws.register_project(pid(99), root(2)).unwrap();

    assert_eq!(
        outcome,
        AddProjectOutcome::AlreadyRegistered { project_id: pid(2) }
    );
    assert!(marked(&ws).is_empty());
}

#[test]
fn タイルにフォーカスすると消える() {
    // 拡大中にほかのタイルへフォーカスすると、拡大したままそのタイルが見える
    let mut ws = workspace(&[3]);
    ws.set_display_mode(DisplayMode::Tiles);
    ws.toggle_maximize();
    let other = all_ids(&ws)[2].clone();
    ws.note_attention(&other);

    assert!(ws.focus_terminal(&other).unwrap());

    assert!(marked(&ws).is_empty());
}

#[test]
fn 拡大を解いて見えただけのタイルの印は残る() {
    let mut ws = workspace(&[3]);
    ws.set_display_mode(DisplayMode::Tiles);
    ws.toggle_maximize();
    let other = all_ids(&ws)[1].clone();
    ws.note_attention(&other);

    assert!(ws.toggle_maximize());

    assert!(ws.visible_terminals().contains(&other));
    assert_eq!(marked(&ws), vec![other]);
}

#[test]
fn タイル表示に切り替えて見えただけのタイルの印は残る() {
    // タブ表示では A の 2 枚目は隠れている。タイル表示にすると同じページに並ぶが、フォーカスは A の 1 枚目のまま
    let mut ws = workspace(&[2, 1]);
    let a2 = tabs_of(&ws, &pid(1))[1].clone();
    let b1 = tabs_of(&ws, &pid(2))[0].clone();
    ws.note_attention(&a2);
    ws.note_attention(&b1);

    assert!(ws.set_display_mode(DisplayMode::Tiles));

    assert_eq!(marked(&ws), vec![a2, b1]);
}

#[test]
fn タブ表示に切り替えても見えないタイルの印は残る() {
    // タイル表示で拡大中に立った印は、タブ表示に戻して隠れたままなら残る
    let mut ws = workspace(&[2]);
    ws.set_display_mode(DisplayMode::Tiles);
    ws.toggle_maximize();
    let a2 = tabs_of(&ws, &pid(1))[1].clone();
    ws.note_attention(&a2);

    assert!(ws.set_display_mode(DisplayMode::Tabs));

    assert_eq!(marked(&ws), vec![a2]);
}

#[test]
fn 新しいタブを開いても隠れたタブの印は残る() {
    let mut ws = workspace(&[2]);
    let a2 = tabs_of(&ws, &pid(1))[1].clone();
    ws.note_attention(&a2);

    ws.open_terminal(&pid(1)).unwrap();

    assert_eq!(marked(&ws), vec![a2]);
}

#[test]
fn タブを閉じて隣のタブが表示中になると消える() {
    let mut ws = workspace(&[2]);
    let a = tabs_of(&ws, &pid(1));
    ws.note_attention(&a[1]);

    ws.close_terminal(&a[0]).unwrap();

    assert!(marked(&ws).is_empty());
}

// ---------------------------------------------------------------------------
// 印が残らない(ターミナルが無くなったとき)

#[test]
fn 閉じたタブの印は残らない() {
    let mut ws = workspace(&[3]);
    let a = tabs_of(&ws, &pid(1));
    ws.note_attention(&a[2]);

    ws.close_terminal(&a[2]).unwrap();

    assert!(marked(&ws).is_empty());
    // 閉じたあとの契機は何もしない
    assert!(!ws.note_attention(&a[2]));
}

#[test]
fn シェルが終わって閉じたタブの印は残らない() {
    let mut ws = workspace(&[2]);
    let a2 = tabs_of(&ws, &pid(1))[1].clone();
    ws.mark_running(&a2).unwrap();
    ws.note_attention(&a2);

    ws.mark_exited(
        &a2,
        Some(ChildExit {
            exit_code: 0,
            signal: None,
        }),
    );

    assert!(marked(&ws).is_empty());
}

#[test]
fn 削除したプロジェクトのタブの印は残らない() {
    let mut ws = workspace(&[1, 2]);
    let b = tabs_of(&ws, &pid(2));
    ws.note_attention(&b[0]);
    ws.note_attention(&b[1]);

    let removed = ws.remove_project(&pid(2), &[], true).unwrap();

    assert!(matches!(removed, RemoveOutcome::Removed { .. }));
    assert!(marked(&ws).is_empty());
    assert!(ws.snapshot().display.attention_pages.is_empty());
}

// ---------------------------------------------------------------------------
// 写し

#[test]
fn 写しのターミナルに印が載る() {
    let mut ws = workspace(&[2]);
    let a = tabs_of(&ws, &pid(1));
    ws.note_attention(&a[1]);

    let terminals = ws.snapshot().terminals;

    assert!(!terminals[0].attention);
    assert!(terminals[1].attention);
}

#[test]
fn 写しにアテンションのあるページの番号が載る() {
    // 28 枚: 4 ページ。2 ページ目(1)に 2 枚、4 ページ目(3)に 1 枚。番号は 0 から、昇順で重なりなし
    let mut ws = workspace(&[28]);
    ws.set_display_mode(DisplayMode::Tiles);
    let ids = all_ids(&ws);
    ws.note_attention(&ids[27]);
    ws.note_attention(&ids[10]);
    ws.note_attention(&ids[9]);

    let display = ws.snapshot().display;

    assert_eq!(display.page, 0);
    assert_eq!(display.attention_pages, vec![1, 3]);
}

#[test]
fn 印が無ければアテンションのあるページは空() {
    let mut ws = workspace(&[10]);
    ws.set_display_mode(DisplayMode::Tiles);

    assert!(ws.snapshot().display.attention_pages.is_empty());
}

#[test]
fn 表示が変わる操作のあともフォーカス中のターミナルに印は無い() {
    // すべてのターミナルに印を立てようとしてから、表示が変わる口を順に通す
    let mut ws = workspace(&[4, 7, 2]);
    ws.set_display_mode(DisplayMode::Tiles);
    let mark_all = |ws: &mut Workspace| {
        for id in all_ids(ws) {
            ws.note_attention(&id);
        }
        assert_no_mark_on_focused(ws);
    };

    mark_all(&mut ws);
    ws.next_page();
    assert_no_mark_on_focused(&ws);
    mark_all(&mut ws);
    ws.toggle_maximize();
    assert_no_mark_on_focused(&ws);
    mark_all(&mut ws);
    ws.select_project(&pid(3)).unwrap();
    assert_no_mark_on_focused(&ws);
    mark_all(&mut ws);
    ws.toggle_maximize();
    assert_no_mark_on_focused(&ws);
    mark_all(&mut ws);
    ws.previous_page();
    assert_no_mark_on_focused(&ws);
    mark_all(&mut ws);
    ws.set_display_mode(DisplayMode::Tabs);
    assert_no_mark_on_focused(&ws);
    mark_all(&mut ws);
    ws.select_previous_terminal();
    assert_no_mark_on_focused(&ws);
}
