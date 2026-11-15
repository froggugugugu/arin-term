//! ペーストボードの結合テスト(`arin_platform::desktop::MacPasteboard`、T31c)。FR-064。
//!
//! ユーザーのクリップボード(一般のペーストボード)には触れない。テストごとに名前付きのペーストボードを作り、
//! 書いて読み戻し、終わったら解放する。

use std::sync::atomic::{AtomicUsize, Ordering};

use arin_core::ports::pasteboard::PasteboardPort;
use arin_platform::desktop::MacPasteboard;

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// テスト用の名前付きペーストボード。抜けるときに解放する。
struct TestBoard(MacPasteboard);

impl TestBoard {
    fn new(name: &str) -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        Self(MacPasteboard::named(format!(
            "com.froggugugugu.arin.test.{name}.{}.{n}",
            std::process::id()
        )))
    }
}

impl Drop for TestBoard {
    fn drop(&mut self) {
        self.0.release();
    }
}

#[test]
fn テスト用の名前付きペーストボードに書いた文字列を読み戻せる() {
    let board = TestBoard::new("roundtrip");
    let text = "/Users/someone/Projects/日本語 フォルダ";
    board.0.write_text(text).expect("書けるはず");
    assert_eq!(board.0.read_text(), Some(text.to_owned()));
}

#[test]
fn 書き直すと前の中身を置き換える() {
    let board = TestBoard::new("overwrite");
    board.0.write_text("first").unwrap();
    board.0.write_text("second").unwrap();
    assert_eq!(board.0.read_text(), Some("second".to_owned()));
}

#[test]
fn 名前付きのペーストボードは一般のペーストボードと別() {
    let board = TestBoard::new("separate");
    assert!(!board.0.is_general());
    assert!(MacPasteboard::general().is_general());
}

#[test]
fn スレッドをまたいで書ける() {
    let board = TestBoard::new("thread");
    std::thread::scope(|s| {
        s.spawn(|| board.0.write_text("from thread").unwrap());
    });
    assert_eq!(board.0.read_text(), Some("from thread".to_owned()));
}
