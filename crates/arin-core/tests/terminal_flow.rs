//! 出力の流量制御(`FlowControl`)とまとめ(`OutputBatch`)の規則。設計書 §1.3 #3、§7.1「ターミナルの入出力」、NFR-004。
//!
//! 未処理の量 = PTY から読んだが、画面が取り込み済みと返していない量。上限を超えたら止め、下限まで減ったら再開する。

use arin_core::terminal::{
    FlowControl, OUTPUT_BATCH_MAX_BYTES, OUTPUT_HIGH_WATER, OUTPUT_LOW_WATER, OutputBatch,
};

#[test]
fn 未処理の量が上限を超えるまでは止めない() {
    let mut flow = FlowControl::new(100, 20);

    flow.on_read(60);
    flow.on_read(40);

    assert_eq!(flow.unacked(), 100);
    assert!(!flow.is_paused(), "上限ちょうどで止めた");
}

#[test]
fn 未処理の量が上限を超えたら止める() {
    let mut flow = FlowControl::new(100, 20);

    flow.on_read(101);

    assert!(flow.is_paused());
}

#[test]
fn 止めたあとは下限まで減るまで再開しない() {
    let mut flow = FlowControl::new(100, 20);
    flow.on_read(150);

    assert!(!flow.on_acked(100), "下限より上で再開した");
    assert_eq!(flow.unacked(), 50);
    assert!(flow.is_paused());

    assert!(flow.on_acked(30), "下限ちょうどで再開しなかった");
    assert_eq!(flow.unacked(), 20);
    assert!(!flow.is_paused());
}

#[test]
fn 止めていないときの処理済みの通知は再開を知らせない() {
    let mut flow = FlowControl::new(100, 20);
    flow.on_read(50);

    assert!(!flow.on_acked(50));
    assert_eq!(flow.unacked(), 0);
}

#[test]
fn 処理済みの通知が未処理の量を超えても_0_に留めて再開する() {
    let mut flow = FlowControl::new(100, 20);
    flow.on_read(150);

    assert!(flow.on_acked(u64::MAX));

    assert_eq!(flow.unacked(), 0);
    assert!(!flow.is_paused());
}

#[test]
fn 未処理の量は上限なく足しても溢れない() {
    let mut flow = FlowControl::new(100, 20);
    flow.on_read(usize::MAX);
    flow.on_read(usize::MAX);

    assert_eq!(flow.unacked(), u64::MAX);
    assert!(flow.is_paused());
}

#[test]
fn 付け替えで未処理の量を捨てて再開する() {
    let mut flow = FlowControl::new(100, 20);
    flow.on_read(150);

    flow.reset();

    assert_eq!(flow.unacked(), 0);
    assert!(!flow.is_paused());
}

#[test]
#[should_panic(expected = "下限")]
fn 下限が上限以上なら作れない() {
    let _ = FlowControl::new(20, 20);
}

#[test]
fn 既定のしきい値は下限が上限より小さく_まとめの上限より大きい() {
    const { assert!(OUTPUT_LOW_WATER < OUTPUT_HIGH_WATER) };
    const { assert!(OUTPUT_BATCH_MAX_BYTES as u64 <= OUTPUT_LOW_WATER) };
    let flow = FlowControl::default();
    assert_eq!(flow.unacked(), 0);
    assert!(!flow.is_paused());
}

#[test]
fn まとめは届いた順につなぎ_取り出すと空になる() {
    let mut batch = OutputBatch::new(8);
    assert!(batch.is_empty());

    batch.push(b"ab");
    batch.push(b"cd");

    assert!(!batch.is_empty());
    assert!(!batch.is_full());
    assert_eq!(batch.take(), b"abcd");
    assert!(batch.is_empty());
    assert_eq!(batch.take(), b"");
}

#[test]
fn まとめは上限のバイト数に達したら満杯になる() {
    let mut batch = OutputBatch::new(4);

    batch.push(b"abc");
    assert!(!batch.is_full());
    batch.push(b"defg");

    assert!(batch.is_full());
    // 上限を超えた分も切らずに 1 つで渡す(1 回の読み取りを分けない)。
    assert_eq!(batch.take(), b"abcdefg");
}

#[test]
fn 既定のまとめの上限を使える() {
    let mut batch = OutputBatch::default();
    batch.push(&vec![0; OUTPUT_BATCH_MAX_BYTES - 1]);
    assert!(!batch.is_full());
    batch.push(b"x");
    assert!(batch.is_full());
}
