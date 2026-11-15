//! タブの規則(`TerminalStore`)。FR-039、FR-044、C-21、設計書 §6.1・§15.2 #3、計画書 M2 の【仮定】5・9・11・17。

use arin_core::ports::PtySize;
use arin_core::project::ProjectId;
use arin_core::terminal::{
    MAX_COLS, MAX_ROWS, TerminalError, TerminalId, TerminalStore, validate_size,
};

fn project(n: u8) -> ProjectId {
    ProjectId::from_random_bytes([n; 16])
}

fn ids(store: &TerminalStore, project: &ProjectId) -> Vec<TerminalId> {
    store.tabs(project).iter().map(|t| t.id.clone()).collect()
}

/// プロジェクト `p` にタブを `count` 枚開き、開いた順の ID を返す。
fn open(store: &mut TerminalStore, p: &ProjectId, count: usize) -> Vec<TerminalId> {
    (0..count).map(|_| store.add(p)).collect()
}

#[test]
fn プロジェクトごとにタブを持ち_追加すると末尾に増えて表示中になる() {
    let (a, b) = (project(1), project(2));
    let mut store = TerminalStore::default();

    let a1 = store.add(&a);
    assert_eq!(store.active(&a), Some(&a1));
    let b1 = store.add(&b);
    let a2 = store.add(&a);

    assert_eq!(ids(&store, &a), vec![a1, a2.clone()]);
    assert_eq!(ids(&store, &b), vec![b1.clone()]);
    assert_eq!(store.active(&a), Some(&a2));
    assert_eq!(
        store.active(&b),
        Some(&b1),
        "別のプロジェクトの表示中が変わった"
    );
    assert_eq!(store.tab(&a2).map(|t| &t.project), Some(&a));
    assert_eq!(store.iter().count(), 3);
}

#[test]
fn 閉じると右隣_無ければ左が表示中になる() {
    let a = project(1);
    let mut store = TerminalStore::default();
    let t = open(&mut store, &a, 4);

    // 表示中(末尾)でないタブを閉じても表示中は変わらない。
    store.close(&t[1]).unwrap();
    assert_eq!(store.active(&a), Some(&t[3]));

    // 表示中を閉じると右隣。
    store.activate(&t[0]).unwrap();
    let closed = store.close(&t[0]).unwrap();
    assert_eq!(closed.id, t[0]);
    assert_eq!(closed.project, a);
    assert_eq!(store.active(&a), Some(&t[2]));

    // 右隣が無ければ左。
    store.activate(&t[3]).unwrap();
    store.close(&t[3]).unwrap();
    assert_eq!(store.active(&a), Some(&t[2]));
    assert_eq!(ids(&store, &a), vec![t[2].clone()]);
}

#[test]
fn 最後の_1_枚を閉じると_0_枚になり自動で開かない() {
    let a = project(1);
    let mut store = TerminalStore::default();
    let t = store.add(&a);

    store.close(&t).unwrap();

    assert!(store.tabs(&a).is_empty());
    assert_eq!(store.active(&a), None);
    assert_eq!(store.iter().count(), 0);
    assert_eq!(store.close(&t), Err(TerminalError::NotFound(t.clone())));
}

#[test]
fn _0_枚から追加できる() {
    let a = project(1);
    let mut store = TerminalStore::default();
    assert!(store.tabs(&a).is_empty());
    assert_eq!(store.active(&a), None);

    let t1 = store.add(&a);
    store.close(&t1).unwrap();
    let t2 = store.add(&a);

    assert_eq!(ids(&store, &a), vec![t2.clone()]);
    assert_eq!(store.active(&a), Some(&t2));
}

#[test]
fn 前後のタブの切替は端で反対側に回り込む() {
    let a = project(1);
    let mut store = TerminalStore::default();
    let t = open(&mut store, &a, 3);

    // 表示中は末尾(t[2])。次は先頭に回り込む。
    assert_eq!(store.select_next(&a), Some(t[0].clone()));
    assert_eq!(store.select_next(&a), Some(t[1].clone()));
    assert_eq!(store.select_previous(&a), Some(t[0].clone()));
    // 先頭の前は末尾に回り込む。
    assert_eq!(store.select_previous(&a), Some(t[2].clone()));
    assert_eq!(store.active(&a), Some(&t[2]));
}

#[test]
fn タブが_1_枚なら前後の切替は何もしない() {
    let (a, empty) = (project(1), project(2));
    let mut store = TerminalStore::default();
    let t = store.add(&a);

    assert_eq!(store.select_next(&a), Some(t.clone()));
    assert_eq!(store.select_previous(&a), Some(t.clone()));
    assert_eq!(store.active(&a), Some(&t));

    // 0 枚(タブを持ったことが無いプロジェクトも)は何もしない。
    assert_eq!(store.select_next(&empty), None);
    assert_eq!(store.select_previous(&empty), None);
    assert!(store.tabs(&empty).is_empty());
}

#[test]
fn ターミナルの_id_は起動中に使い回さない() {
    let (a, b) = (project(1), project(2));
    let mut store = TerminalStore::default();
    let mut seen = Vec::new();

    let first = store.add(&a);
    assert_eq!(first, TerminalId::new("terminal-1"), "M1 と同じ形の ID");
    seen.push(first.clone());
    store.close(&first).unwrap();
    for _ in 0..3 {
        let t = store.add(&b);
        seen.push(t.clone());
        store.close(&t).unwrap();
    }
    seen.push(store.add(&a));
    store.remove_project(&a);
    seen.push(store.add(&a));
    seen.push(store.add(&b));

    let mut unique = seen.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), seen.len(), "ID を使い回した: {seen:?}");
}

#[test]
fn タブの表示名はプロジェクト内の通し番号() {
    let (a, b) = (project(1), project(2));
    let mut store = TerminalStore::default();
    let a1 = store.add(&a);
    let a2 = store.add(&a);
    let b1 = store.add(&b);

    assert_eq!(store.tab(&a1).unwrap().label, "ターミナル 1");
    assert_eq!(store.tab(&a2).unwrap().label, "ターミナル 2");
    assert_eq!(store.tab(&b1).unwrap().label, "ターミナル 1");
    assert_eq!(store.tab(&a2).unwrap().number, 2);

    // 閉じた番号は使い回さない(同じ名前のタブが続けて現れて取り違えないように)。
    store.close(&a2).unwrap();
    let a3 = store.add(&a);
    assert_eq!(store.tab(&a3).unwrap().label, "ターミナル 3");
    // 0 枚になっても番号は続く。
    store.close(&a1).unwrap();
    store.close(&a3).unwrap();
    let a4 = store.add(&a);
    assert_eq!(store.tab(&a4).unwrap().label, "ターミナル 4");
    assert_eq!(store.tab(&TerminalId::new("nope")), None);
}

#[test]
fn プロジェクトのタブをまとめて外せる() {
    let (a, b) = (project(1), project(2));
    let mut store = TerminalStore::default();
    let ta = open(&mut store, &a, 3);
    let tb = store.add(&b);

    let removed = store.remove_project(&a);

    assert_eq!(removed, ta, "タブの順で返す");
    assert!(store.tabs(&a).is_empty());
    assert_eq!(store.active(&a), None);
    assert_eq!(ids(&store, &b), vec![tb.clone()]);
    assert_eq!(store.active(&b), Some(&tb));
    assert!(store.remove_project(&a).is_empty(), "2 回目は何も外さない");

    // 外したあとに開くと番号は 1 から。
    let again = store.add(&a);
    assert_eq!(store.tab(&again).unwrap().label, "ターミナル 1");
}

#[test]
fn 表示中のタブを選べる_無い_id_は_not_found() {
    let (a, b) = (project(1), project(2));
    let mut store = TerminalStore::default();
    let t = open(&mut store, &a, 2);
    let tb = store.add(&b);

    store.activate(&t[0]).unwrap();

    assert_eq!(store.active(&a), Some(&t[0]));
    assert_eq!(store.active(&b), Some(&tb));
    let missing = TerminalId::new("terminal-99");
    assert_eq!(
        store.activate(&missing),
        Err(TerminalError::NotFound(missing))
    );
}

#[test]
fn 寸法は行_1000_桁_2000_まで_超えたら_invalid_size() {
    assert_eq!((MAX_ROWS, MAX_COLS), (1_000, 2_000));
    assert_eq!(
        validate_size(1_000, 2_000),
        Ok(PtySize {
            rows: 1_000,
            cols: 2_000
        })
    );
    for (rows, cols) in [(1_001, 80), (24, 2_001), (u16::MAX, u16::MAX)] {
        assert_eq!(
            validate_size(rows, cols),
            Err(TerminalError::InvalidSize { rows, cols })
        );
    }
    let message = TerminalError::InvalidSize {
        rows: 1_001,
        cols: 80,
    }
    .to_string();
    assert!(
        message.contains("1000") && message.contains("2000"),
        "{message}"
    );
}
