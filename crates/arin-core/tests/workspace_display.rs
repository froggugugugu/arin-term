//! `Workspace` の表示の状態(表示モード・ページ・拡大・タイルへのフォーカス)と写しの `display`。
//! FR-048・FR-051〜054、NFR-009、設計書 §3.4・§6.1・§8、ADR-011、計画書 T46b(前提 (a)(c)(d)、【仮定】1〜7)。
//!
//! 不変条件(どの操作のあとも成り立つ):
//! - タイルの並びはプロジェクト順 → タブ順(写しの `terminals[]` と同じ順)
//! - フォーカス中のタイル=選択中のプロジェクトの表示中のタブ
//! - ページは範囲内(0 枚なら 0)。タイル表示ではフォーカス中のタイルは表示中のページにある
//! - 拡大中なら、拡大しているのはフォーカス中のタイル(実在する)

use std::path::PathBuf;

use arin_core::layout::tiles::{DisplayMode, TILES_PER_PAGE, TileGrid};
use arin_core::persistence::codec::PersistedState;
use arin_core::ports::ChildExit;
use arin_core::project::{Project, ProjectColor, ProjectId, RootReference};
use arin_core::terminal::{TerminalError, TerminalId};
use arin_core::workspace::{RemoveOutcome, RootResolution, Startup, Workspace};

const HOME: &str = "/Users/me";

fn pid(n: u8) -> ProjectId {
    ProjectId::from_random_bytes([n; 16])
}

fn root_path(n: u8) -> PathBuf {
    PathBuf::from(format!("/Users/me/dev/p{n}"))
}

/// プロジェクト 1〜`counts.len()` を並べ、プロジェクト n に `counts[n-1]` 枚のタブを持たせた状態。
/// 選択はプロジェクト 1。タブはプロジェクトごとにまとめて開く(0 枚のものは起動時の 1 枚を閉じる)。
fn workspace(counts: &[usize]) -> Workspace {
    let projects: Vec<(ProjectId, RootReference)> = (1..=counts.len())
        .map(|n| {
            let n = u8::try_from(n).unwrap();
            (pid(n), RootReference::new(root_path(n), None))
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
        if count == 0 {
            let only = tabs_of(&ws, &id)[0].clone();
            ws.close_terminal(&only).unwrap();
        }
        for _ in 1..count {
            ws.open_terminal(&id).unwrap();
        }
    }
    // タブを開いた順で表示中が動くので、プロジェクト 1 を選び直した状態から始める。
    ws.select_project(&pid(1)).unwrap();
    check_invariants(&ws);
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

fn selected(ws: &Workspace) -> Option<ProjectId> {
    ws.snapshot().selected_project_id
}

fn active_of(ws: &Workspace, project: &ProjectId) -> Option<TerminalId> {
    ws.snapshot()
        .projects
        .into_iter()
        .find(|p| &p.id == project)
        .and_then(|p| p.active_terminal_id)
}

fn page(ws: &Workspace) -> u32 {
    ws.snapshot().display.page
}

/// 不変条件を確かめる。
fn check_invariants(ws: &Workspace) {
    let snapshot = ws.snapshot();
    let display = &snapshot.display;
    let all: Vec<TerminalId> = snapshot.terminals.iter().map(|t| t.id.clone()).collect();

    // フォーカス中のタイル=選択中のプロジェクトの表示中のタブ
    let expected_focus = snapshot
        .selected_project_id
        .as_ref()
        .and_then(|id| snapshot.projects.iter().find(|p| &p.id == id))
        .and_then(|p| p.active_terminal_id.clone());
    assert_eq!(
        display.focused_terminal_id, expected_focus,
        "フォーカスが表示中のタブと違う"
    );
    assert_eq!(ws.active_terminal(), expected_focus);

    // ページは範囲内。空のページを指さない
    let pages = all.len().div_ceil(TILES_PER_PAGE);
    assert_eq!(display.page_count as usize, pages);
    if pages == 0 {
        assert_eq!(display.page, 0);
        assert!(display.tile_ids.is_empty());
        assert_eq!(display.grid, TileGrid { cols: 0, rows: 0 });
    } else {
        assert!(
            (display.page as usize) < pages,
            "ページが範囲の外: {display:?}"
        );
        assert!(!display.tile_ids.is_empty(), "空のページを指している");
    }

    // そのページのタイルは全体の並びの連続した一部
    let start = display.page as usize * TILES_PER_PAGE;
    let end = (start + TILES_PER_PAGE).min(all.len());
    assert_eq!(display.tile_ids, all[start.min(all.len())..end].to_vec());

    // タイル表示ではフォーカス中のタイルは表示中のページにある
    if display.mode == DisplayMode::Tiles
        && let Some(focus) = &display.focused_terminal_id
    {
        assert!(
            display.tile_ids.contains(focus),
            "フォーカス中のタイルが表示中のページに無い"
        );
    }

    // 拡大中はタイル表示で、フォーカス中のタイルがある
    if display.maximized {
        assert_eq!(display.mode, DisplayMode::Tiles);
        assert!(display.focused_terminal_id.is_some());
    }

    // 見えるものは実在する
    for id in ws.visible_terminals() {
        assert!(
            all.contains(&id),
            "実在しないターミナルが見えている: {id:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// 並びとページ(FR-051・FR-052)

#[test]
fn タイルの並びはプロジェクト順でその中はタブ順() {
    let mut ws = workspace(&[1, 1]);
    // B のタブを開いたあとで A のタブを足す(ID の番号の順とプロジェクト順がずれる)。
    let b2 = ws.open_terminal(&pid(2)).unwrap();
    let a2 = ws.open_terminal(&pid(1)).unwrap();
    ws.set_display_mode(DisplayMode::Tiles);

    let a = tabs_of(&ws, &pid(1));
    let b = tabs_of(&ws, &pid(2));
    assert_eq!(a[1], a2);
    assert_eq!(b[1], b2);
    let expected = [a.clone(), b.clone()].concat();
    assert_eq!(ws.snapshot().display.tile_ids, expected);
    assert_eq!(all_ids(&ws), expected, "写しの terminals と同じ順");
    assert_eq!(ws.visible_terminals(), expected);
    check_invariants(&ws);
}

#[test]
fn 見つからないプロジェクトのタブもタイルに並びフォーカスできる() {
    // 【仮定】1。C-06(見つからないプロジェクトのカードとタブは残す)
    let mut ws = workspace(&[1, 1]);
    assert!(ws.mark_project_missing(&pid(2), &root_path(2)).unwrap());
    ws.set_display_mode(DisplayMode::Tiles);
    let b1 = tabs_of(&ws, &pid(2))[0].clone();

    assert!(ws.snapshot().display.tile_ids.contains(&b1));
    assert!(ws.focus_terminal(&b1).unwrap());
    assert_eq!(selected(&ws), Some(pid(2)));
    check_invariants(&ws);
}

#[test]
fn _10_枚でタイル表示にすると_1_ページ目に_9_枚_ページ数は_2() {
    // AC-M14 の Rust 側
    let mut ws = workspace(&[10]);
    let a = tabs_of(&ws, &pid(1));
    ws.focus_terminal(&a[0]).unwrap();
    assert!(ws.set_display_mode(DisplayMode::Tiles));

    let display = ws.snapshot().display;
    assert_eq!(display.mode, DisplayMode::Tiles);
    assert_eq!(display.page, 0);
    assert_eq!(display.page_count, 2);
    assert_eq!(display.grid, TileGrid { cols: 3, rows: 3 });
    assert_eq!(display.tile_ids, a[..9].to_vec());
    assert!(!display.maximized);
    check_invariants(&ws);
}

#[test]
fn タイル表示に切り替えるとフォーカス中のタイルのページを出す() {
    let mut ws = workspace(&[10]);
    let a = tabs_of(&ws, &pid(1));
    ws.focus_terminal(&a[9]).unwrap();
    ws.set_display_mode(DisplayMode::Tiles);

    let display = ws.snapshot().display;
    assert_eq!(display.page, 1);
    assert_eq!(display.grid, TileGrid { cols: 1, rows: 1 });
    assert_eq!(display.tile_ids, vec![a[9].clone()]);
    check_invariants(&ws);
}

#[test]
fn ターミナルが_0_枚のタイル表示はページもタイルも無い() {
    // 【仮定】2(0 枚のタイル表示は案内だけ)
    let mut ws = workspace(&[0]);
    ws.set_display_mode(DisplayMode::Tiles);

    let display = ws.snapshot().display;
    assert_eq!(display.page, 0);
    assert_eq!(display.page_count, 0);
    assert_eq!(display.grid, TileGrid { cols: 0, rows: 0 });
    assert!(display.tile_ids.is_empty());
    assert_eq!(display.focused_terminal_id, None);
    assert!(ws.visible_terminals().is_empty());
    assert!(!ws.next_page());
    assert!(!ws.previous_page());
    assert!(!ws.toggle_maximize());
    check_invariants(&ws);
}

// ---------------------------------------------------------------------------
// タイルへのフォーカスとカードの連動(FR-053、AC-M15)

#[test]
fn タイルにフォーカスすると持ち主のプロジェクトが選択されそのタブが表示中になる() {
    // AC-M15 の Rust 側
    let mut ws = workspace(&[1, 2]);
    ws.set_display_mode(DisplayMode::Tiles);
    let b = tabs_of(&ws, &pid(2));

    assert!(ws.focus_terminal(&b[0]).unwrap());

    assert_eq!(selected(&ws), Some(pid(2)));
    assert_eq!(active_of(&ws, &pid(2)), Some(b[0].clone()));
    assert_eq!(
        ws.snapshot().display.focused_terminal_id,
        Some(b[0].clone())
    );
    check_invariants(&ws);
}

#[test]
fn 同じプロジェクトのタイルへのフォーカスは選択を変えない() {
    let mut ws = workspace(&[3, 1]);
    ws.set_display_mode(DisplayMode::Tiles);
    let a = tabs_of(&ws, &pid(1));

    assert!(ws.focus_terminal(&a[1]).unwrap());

    assert_eq!(selected(&ws), Some(pid(1)));
    assert_eq!(active_of(&ws, &pid(1)), Some(a[1].clone()));
    check_invariants(&ws);
}

#[test]
fn フォーカス中のタイルへのフォーカスは変わったと返さない() {
    let mut ws = workspace(&[2]);
    ws.set_display_mode(DisplayMode::Tiles);
    let focus = ws.active_terminal().unwrap();
    let before = ws.snapshot();

    assert!(!ws.focus_terminal(&focus).unwrap());
    assert_eq!(ws.snapshot(), before);
}

#[test]
fn 知らないターミナルと持ち主の無いターミナルへのフォーカスは失敗し何も変えない() {
    let mut ws = workspace(&[2]);
    let before = ws.snapshot();
    let unknown = TerminalId::new("no-such-terminal");
    assert_eq!(
        ws.focus_terminal(&unknown),
        Err(TerminalError::NotFound(unknown.clone()))
    );
    assert_eq!(ws.snapshot(), before);

    // 持ち主の無いターミナル(検証画面)はプロジェクトを選べないので失敗にする
    let mut grid = Workspace::for_verify_grid("1.2.3", PathBuf::from(HOME), [("HOME", HOME)]);
    let first = grid.snapshot().terminals[0].id.clone();
    let before = grid.snapshot();
    assert_eq!(
        grid.focus_terminal(&first),
        Err(TerminalError::NotFound(first.clone()))
    );
    assert_eq!(grid.snapshot(), before);
}

#[test]
fn カードを選ぶとそのプロジェクトの最初のタイルのページへ移る() {
    // FR-053 の 2 つ目の AC。A が 9 枚、B が 2 枚 → B のタイルは 2 ページ目
    let mut ws = workspace(&[9, 2]);
    ws.set_display_mode(DisplayMode::Tiles);
    assert_eq!(page(&ws), 0);
    let b = tabs_of(&ws, &pid(2));
    let active_b = active_of(&ws, &pid(2));

    ws.select_project(&pid(2)).unwrap();

    assert_eq!(page(&ws), 1);
    assert_eq!(
        active_of(&ws, &pid(2)),
        active_b,
        "同じページにあるので表示中のタブは変えない"
    );
    assert!(ws.snapshot().display.tile_ids.contains(&b[0]));
    check_invariants(&ws);
}

#[test]
fn 表示中のタブがそのページに無ければ最初のタイルを表示中にする() {
    // A が 8 枚、B が 3 枚 → B の 1 枚目は 1 ページ目、2・3 枚目は 2 ページ目。B の表示中は 3 枚目
    let mut ws = workspace(&[8, 3]);
    let b = tabs_of(&ws, &pid(2));
    assert_eq!(active_of(&ws, &pid(2)), Some(b[2].clone()));
    ws.set_display_mode(DisplayMode::Tiles);

    ws.select_project(&pid(2)).unwrap();

    assert_eq!(active_of(&ws, &pid(2)), Some(b[0].clone()));
    assert_eq!(page(&ws), 0);
    check_invariants(&ws);
}

#[test]
fn タブ表示でカードを選んでも表示中のタブは変えない() {
    let mut ws = workspace(&[8, 3]);
    let b = tabs_of(&ws, &pid(2));

    ws.select_project(&pid(2)).unwrap();

    assert_eq!(active_of(&ws, &pid(2)), Some(b[2].clone()));
    check_invariants(&ws);
}

#[test]
fn タブが_0_枚のプロジェクトを選んでもページは変わらない() {
    let mut ws = workspace(&[10, 0]);
    let a = tabs_of(&ws, &pid(1));
    ws.focus_terminal(&a[9]).unwrap();
    ws.set_display_mode(DisplayMode::Tiles);
    assert_eq!(page(&ws), 1);

    ws.select_project(&pid(2)).unwrap();

    assert_eq!(page(&ws), 1);
    assert_eq!(ws.snapshot().display.focused_terminal_id, None);
    assert_eq!(ws.visible_terminals(), vec![a[9].clone()]);
    check_invariants(&ws);
}

// ---------------------------------------------------------------------------
// ページ送り(FR-052、【仮定】3・4)

#[test]
fn 次のページで先頭のタイルにフォーカスが移る() {
    // A が 5 枚、B が 6 枚 → 2 ページ目の先頭は B の 5 枚目(全体の 10 枚目)
    let mut ws = workspace(&[5, 6]);
    ws.set_display_mode(DisplayMode::Tiles);
    let all = all_ids(&ws);

    assert!(ws.next_page());

    assert_eq!(page(&ws), 1);
    assert_eq!(selected(&ws), Some(pid(2)));
    assert_eq!(
        ws.snapshot().display.focused_terminal_id,
        Some(all[9].clone())
    );
    check_invariants(&ws);

    assert!(ws.previous_page());
    assert_eq!(page(&ws), 0);
    assert_eq!(selected(&ws), Some(pid(1)));
    assert_eq!(
        ws.snapshot().display.focused_terminal_id,
        Some(all[0].clone())
    );
    check_invariants(&ws);
}

#[test]
fn 最後のページの次と最初のページの前は何もしない() {
    let mut ws = workspace(&[10]);
    ws.set_display_mode(DisplayMode::Tiles);
    ws.focus_terminal(&tabs_of(&ws, &pid(1))[0]).unwrap();

    let before = ws.snapshot();
    assert!(!ws.previous_page());
    assert_eq!(ws.snapshot(), before);

    assert!(ws.next_page());
    let before = ws.snapshot();
    assert!(!ws.next_page(), "循環しない");
    assert_eq!(ws.snapshot(), before);
}

#[test]
fn タブが_0_枚のプロジェクトを選んでいてもページを送れる() {
    let mut ws = workspace(&[10, 0]);
    ws.set_display_mode(DisplayMode::Tiles);
    ws.focus_terminal(&tabs_of(&ws, &pid(1))[0]).unwrap();
    ws.select_project(&pid(2)).unwrap();

    assert!(ws.next_page());

    assert_eq!(page(&ws), 1);
    assert_eq!(
        selected(&ws),
        Some(pid(1)),
        "先頭のタイルの持ち主が選ばれる"
    );
    check_invariants(&ws);
}

// ---------------------------------------------------------------------------
// タブの追加・閉じる・削除・シェルの終了のあと(【仮定】6)

#[test]
fn 新しいタブを開くとそのタイルのページへ移る() {
    let mut ws = workspace(&[9]);
    ws.set_display_mode(DisplayMode::Tiles);
    assert_eq!(page(&ws), 0);

    let new = ws.open_terminal_in_selected().unwrap();

    assert_eq!(page(&ws), 1);
    assert_eq!(ws.snapshot().display.tile_ids, vec![new]);
    check_invariants(&ws);
}

#[test]
fn タブを閉じてページが減るとページを丸める() {
    let mut ws = workspace(&[10]);
    ws.set_display_mode(DisplayMode::Tiles);
    let a = tabs_of(&ws, &pid(1));
    assert_eq!(page(&ws), 1);

    ws.close_terminal(&a[9]).unwrap();

    let display = ws.snapshot().display;
    assert_eq!(display.page_count, 1);
    assert_eq!(display.page, 0);
    assert_eq!(display.focused_terminal_id, Some(a[8].clone()));
    check_invariants(&ws);
}

#[test]
fn フォーカスの無いまま最後のページが消えると前のページに丸める() {
    // A が 9 枚、B が 1 枚。B を選んで 2 ページ目を出してから、B の唯一のタブを閉じる(B は 0 枚)
    let mut ws = workspace(&[9, 1]);
    ws.set_display_mode(DisplayMode::Tiles);
    ws.select_project(&pid(2)).unwrap();
    assert_eq!(page(&ws), 1);
    let b1 = tabs_of(&ws, &pid(2))[0].clone();

    ws.close_terminal(&b1).unwrap();

    let display = ws.snapshot().display;
    assert_eq!(display.focused_terminal_id, None);
    assert_eq!(display.page_count, 1);
    assert_eq!(display.page, 0, "空のページを指さない");
    check_invariants(&ws);
}

#[test]
fn プロジェクトを消してもページが範囲に収まる() {
    let mut ws = workspace(&[9, 1]);
    ws.set_display_mode(DisplayMode::Tiles);
    ws.select_project(&pid(2)).unwrap();
    assert_eq!(page(&ws), 1);

    let outcome = ws.remove_project(&pid(2), &[], false).unwrap();
    assert!(matches!(outcome, RemoveOutcome::Removed { .. }));

    let display = ws.snapshot().display;
    assert_eq!(display.page_count, 1);
    assert_eq!(display.page, 0);
    check_invariants(&ws);
}

#[test]
fn シェルが終わってタブが消えてもページとフォーカスが実在のターミナルを指す() {
    let mut ws = workspace(&[10]);
    ws.set_display_mode(DisplayMode::Tiles);
    let a = tabs_of(&ws, &pid(1));
    ws.mark_running(&a[9]).unwrap();
    assert_eq!(page(&ws), 1);

    ws.mark_exited(
        &a[9],
        Some(ChildExit {
            exit_code: 0,
            signal: None,
        }),
    );

    assert!(!all_ids(&ws).contains(&a[9]));
    assert_eq!(page(&ws), 0);
    check_invariants(&ws);
}

// ---------------------------------------------------------------------------
// 拡大と復帰(FR-054、前提 (d)、【仮定】7)

#[test]
fn 拡大すると表示モードはタイルのまま拡大中になり見えるのはフォーカス中の_1_枚() {
    let mut ws = workspace(&[3, 2]);
    ws.set_display_mode(DisplayMode::Tiles);
    let focus = ws.active_terminal().unwrap();

    assert!(ws.toggle_maximize());

    let display = ws.snapshot().display;
    assert_eq!(display.mode, DisplayMode::Tiles);
    assert!(display.maximized);
    assert_eq!(ws.visible_terminals(), vec![focus]);
    check_invariants(&ws);
}

#[test]
fn もう一度で拡大が解け拡大前のページに戻る() {
    let mut ws = workspace(&[10]);
    ws.set_display_mode(DisplayMode::Tiles);
    assert_eq!(page(&ws), 1);
    ws.toggle_maximize();

    assert!(ws.toggle_maximize());

    let display = ws.snapshot().display;
    assert!(!display.maximized);
    assert_eq!(display.page, 1);
    assert_eq!(ws.visible_terminals(), display.tile_ids);
    check_invariants(&ws);
}

#[test]
fn 拡大中にタブを替えると拡大したままそのタブを出す() {
    let mut ws = workspace(&[3, 2]);
    ws.set_display_mode(DisplayMode::Tiles);
    ws.toggle_maximize();

    let previous = ws.select_previous_terminal().unwrap();
    assert!(ws.snapshot().display.maximized);
    assert_eq!(ws.visible_terminals(), vec![previous]);
    check_invariants(&ws);

    // カードを選んでも拡大したまま、そのプロジェクトの表示中のタブを出す
    ws.select_project(&pid(2)).unwrap();
    assert!(ws.snapshot().display.maximized);
    assert_eq!(
        ws.visible_terminals(),
        vec![active_of(&ws, &pid(2)).unwrap()]
    );
    check_invariants(&ws);
}

#[test]
fn 拡大中のターミナルが閉じると拡大が解ける() {
    let mut ws = workspace(&[3]);
    ws.set_display_mode(DisplayMode::Tiles);
    ws.toggle_maximize();
    let focus = ws.active_terminal().unwrap();

    ws.close_terminal(&focus).unwrap();

    assert!(!ws.snapshot().display.maximized);
    check_invariants(&ws);
}

#[test]
fn 拡大中にタブが_0_枚のプロジェクトを選ぶと拡大が解ける() {
    // 【仮定】拡大して出すタブが無いので、拡大を解く
    let mut ws = workspace(&[3, 0]);
    ws.set_display_mode(DisplayMode::Tiles);
    ws.toggle_maximize();

    ws.select_project(&pid(2)).unwrap();

    assert!(!ws.snapshot().display.maximized);
    check_invariants(&ws);
}

#[test]
fn タブ表示に切り替えると拡大が解ける() {
    let mut ws = workspace(&[3]);
    ws.set_display_mode(DisplayMode::Tiles);
    ws.toggle_maximize();

    assert!(ws.set_display_mode(DisplayMode::Tabs));
    assert!(!ws.snapshot().display.maximized);

    ws.set_display_mode(DisplayMode::Tiles);
    assert!(
        !ws.snapshot().display.maximized,
        "戻しても拡大中にはならない"
    );
    check_invariants(&ws);
}

#[test]
fn 拡大中はページ送りは何もしない() {
    // 【仮定】拡大中はページが見えないので送らない
    let mut ws = workspace(&[10]);
    ws.set_display_mode(DisplayMode::Tiles);
    ws.focus_terminal(&tabs_of(&ws, &pid(1))[0]).unwrap();
    ws.toggle_maximize();
    let before = ws.snapshot();

    assert!(!ws.next_page());
    assert_eq!(ws.snapshot(), before);
}

#[test]
fn タブ表示ではページ送りと拡大は何もしない() {
    let mut ws = workspace(&[10]);
    ws.focus_terminal(&tabs_of(&ws, &pid(1))[0]).unwrap();
    let before = ws.snapshot();

    assert!(!ws.next_page());
    assert!(!ws.previous_page());
    assert!(!ws.toggle_maximize());
    assert_eq!(ws.snapshot(), before);
}

// ---------------------------------------------------------------------------
// 表示中のターミナル(NFR-009。T48b の判定と画面の表示に使う)

#[test]
fn タブ表示で見えるのは選択中のプロジェクトの表示中のタブだけ() {
    let mut ws = workspace(&[3, 2]);
    let a = tabs_of(&ws, &pid(1));
    ws.activate_terminal(&a[1]).unwrap();

    assert_eq!(ws.visible_terminals(), vec![a[1].clone()]);

    ws.select_project(&pid(2)).unwrap();
    assert_eq!(
        ws.visible_terminals(),
        vec![active_of(&ws, &pid(2)).unwrap()]
    );
    check_invariants(&ws);
}

#[test]
fn タブ表示で選択中のプロジェクトが_0_枚なら何も見えない() {
    let mut ws = workspace(&[1, 0]);
    ws.select_project(&pid(2)).unwrap();
    assert!(ws.visible_terminals().is_empty());
}

#[test]
fn タイル表示で見えるのは表示中のページのタイルだけ() {
    let mut ws = workspace(&[6, 6]);
    ws.set_display_mode(DisplayMode::Tiles);
    let all = all_ids(&ws);

    assert_eq!(ws.visible_terminals(), all[..9].to_vec());
    ws.next_page();
    assert_eq!(ws.visible_terminals(), all[9..].to_vec());
    check_invariants(&ws);
}

// ---------------------------------------------------------------------------
// 表示モード(FR-048)

#[test]
fn 表示モードを変えると変わったと返し同じなら返さない() {
    let mut ws = workspace(&[2]);
    assert_eq!(ws.display_mode(), DisplayMode::Tabs);

    assert!(!ws.set_display_mode(DisplayMode::Tabs));
    assert!(ws.set_display_mode(DisplayMode::Tiles));
    assert_eq!(ws.display_mode(), DisplayMode::Tiles);
    assert!(!ws.set_display_mode(DisplayMode::Tiles));

    assert!(ws.toggle_display_mode());
    assert_eq!(ws.display_mode(), DisplayMode::Tabs);
    assert!(ws.toggle_display_mode());
    assert_eq!(ws.display_mode(), DisplayMode::Tiles);
}

#[test]
fn 起動時はタブ表示で始まる() {
    // 表示モードの保存と復元は T46c。それまでは起動のたびにタブ表示
    let ws = workspace(&[2]);
    let display = ws.snapshot().display;
    assert_eq!(display.mode, DisplayMode::Tabs);
    assert!(!display.maximized);
}

// ---------------------------------------------------------------------------
// 不変条件(操作の列を決まった順で流し、毎回確かめる)

#[test]
fn どの操作の列のあとも不変条件が成り立つ() {
    let mut ws = workspace(&[4, 0, 7, 2]);
    // 決定論的な疑似乱数(線形合同法)
    let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
    let mut next = |n: usize| {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        usize::try_from(seed >> 33).unwrap() % n
    };
    for _ in 0..600 {
        let all = all_ids(&ws);
        let projects: Vec<ProjectId> = ws.snapshot().projects.into_iter().map(|p| p.id).collect();
        match next(11) {
            0 => {
                ws.toggle_display_mode();
            }
            1 => {
                ws.next_page();
            }
            2 => {
                ws.previous_page();
            }
            3 => {
                ws.toggle_maximize();
            }
            4 if !all.is_empty() => {
                let id = all[next(all.len())].clone();
                ws.focus_terminal(&id).unwrap();
            }
            5 if !projects.is_empty() => {
                ws.select_project(&projects[next(projects.len())]).unwrap();
            }
            6 if !projects.is_empty() => {
                let _ = ws.open_terminal(&projects[next(projects.len())]);
            }
            7 | 8 if !all.is_empty() => {
                let id = all[next(all.len())].clone();
                ws.close_terminal(&id).unwrap();
            }
            9 => {
                ws.select_next_terminal();
            }
            10 if projects.len() > 1 && next(8) == 0 => {
                ws.remove_project(&projects[next(projects.len())], &[], true)
                    .unwrap();
            }
            _ => {}
        }
        check_invariants(&ws);
    }
}
