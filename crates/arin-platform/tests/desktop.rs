//! デスクトップ連携の結合テスト。
//!
//! - ペーストボードの文字列(`MacPasteboard`、T31c。FR-064)とファイルの URL(`MacFilePasteboard`、T40d。FR-027)
//! - Finder で表示・既定のアプリで開く(`MacDesktop`、T40d。FR-018)
//! - フォルダ選択のダイアログ(T33c)
//!
//! ユーザーのクリップボード(一般のペーストボード)には触れない。テストごとに名前付きのペーストボードを作り、
//! 書いて読み戻し、終わったら解放する。Finder とアプリは自動テストでは開かない(開くテストは `#[ignore]`)。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use arin_core::ports::desktop::{DesktopError, DesktopPort};
use arin_core::ports::pasteboard::{FilePasteboardPort, PasteboardPort};
use arin_platform::desktop::{MacDesktop, MacFilePasteboard, MacPasteboard};

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

// ---- ファイルの URL のペーストボード(T40d。FR-027) ----
//
// 文字列と同じく、テストごとに名前付きのペーストボードを使い、終わったら解放する。
// 書くパスは存在しなくてよい(ファイルの URL はパスから組み立てるだけで、ファイルには触れない)。

/// テスト用の名前付きペーストボード(ファイルの URL 用と、同じ名前の文字列用)。抜けるときに解放する。
struct TestFileBoard {
    files: MacFilePasteboard,
    text: MacPasteboard,
}

impl TestFileBoard {
    fn new(name: &str) -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let name = format!(
            "com.froggugugugu.arin.test.files.{name}.{}.{n}",
            std::process::id()
        );
        Self {
            files: MacFilePasteboard::named(name.clone()),
            text: MacPasteboard::named(name),
        }
    }
}

impl Drop for TestFileBoard {
    fn drop(&mut self) {
        self.files.release();
    }
}

#[test]
fn ファイルのペーストボードの名前付きは一般のペーストボードと別() {
    let board = TestFileBoard::new("separate");
    assert!(!board.files.is_general());
    assert!(MacFilePasteboard::general().is_general());
}

#[test]
fn 名前付きのペーストボードにファイルの_url_を書いて読み戻せる() {
    let board = TestFileBoard::new("file-roundtrip");
    let path = Path::new("/Users/someone/Projects/日本語 フォルダ/a b.ts");

    board.files.write_file_urls(&[path]).expect("書けるはず");

    assert_eq!(
        board.files.read_file_urls().expect("読めるはず"),
        vec![path.to_path_buf()]
    );
}

#[test]
fn 複数のファイルの_url_を順のまま読み戻せる() {
    let board = TestFileBoard::new("file-order");
    let paths = [
        Path::new("/tmp/arin-test/z.txt"),
        Path::new("/tmp/arin-test/a.txt"),
        Path::new("/tmp/arin-test/m フォルダ"),
    ];

    board.files.write_file_urls(&paths).unwrap();

    let expected: Vec<PathBuf> = paths.iter().map(|p| p.to_path_buf()).collect();
    assert_eq!(board.files.read_file_urls().unwrap(), expected);
}

#[test]
fn 本物のフォルダの_url_を書いて読み戻すと元のパスに戻る() {
    let board = TestFileBoard::new("file-dir");
    let dir = TempDir::new("pasteboard-dir");
    let folder = dir.0.join("フォルダ");
    std::fs::create_dir(&folder).unwrap();

    board.files.write_file_urls(&[folder.as_path()]).unwrap();

    assert_eq!(board.files.read_file_urls().unwrap(), vec![folder]);
}

#[test]
fn 書き直すと前のファイルの_url_を置き換える() {
    let board = TestFileBoard::new("file-overwrite");
    board
        .files
        .write_file_urls(&[Path::new("/tmp/arin-test/first")])
        .unwrap();
    board
        .files
        .write_file_urls(&[Path::new("/tmp/arin-test/second")])
        .unwrap();

    assert_eq!(
        board.files.read_file_urls().unwrap(),
        vec![PathBuf::from("/tmp/arin-test/second")]
    );
}

#[test]
fn 文字列だけのときはファイルが無いと返す() {
    let board = TestFileBoard::new("text-only");
    // ファイルの URL に見える文字列でも、文字列として置かれたものはファイルとみなさない
    board
        .text
        .write_text("file:///tmp/arin-test/a.txt")
        .unwrap();

    assert_eq!(board.files.read_file_urls().unwrap(), Vec::<PathBuf>::new());
}

#[test]
fn 空のペーストボードはファイルが無いと返す() {
    let board = TestFileBoard::new("empty");
    assert_eq!(board.files.read_file_urls().unwrap(), Vec::<PathBuf>::new());
}

#[test]
fn 書くと変更番号が変わる() {
    let board = TestFileBoard::new("change-count");
    let before = board.files.change_count();

    let written = board
        .files
        .write_file_urls(&[Path::new("/tmp/arin-test/a.txt")])
        .unwrap();

    assert_ne!(written, before);
    assert_eq!(board.files.change_count(), written);

    // 別のものが書かれたら(Finder でのコピーなど)、また変わる
    board.text.write_text("other").unwrap();
    assert_ne!(board.files.change_count(), written);
}

#[test]
fn 相対パスはペーストボードに書かずに失敗を返す() {
    let board = TestFileBoard::new("relative");
    board.text.write_text("keep").unwrap();
    let before = board.files.change_count();

    let result = board.files.write_file_urls(&[Path::new("src/a.ts")]);

    assert!(result.is_err());
    assert_eq!(board.files.change_count(), before);
    assert_eq!(board.text.read_text(), Some("keep".to_owned()));
}

#[test]
fn ファイルのペーストボードはスレッドをまたいで読み書きできる() {
    let board = TestFileBoard::new("file-thread");
    std::thread::scope(|s| {
        s.spawn(|| {
            board
                .files
                .write_file_urls(&[Path::new("/tmp/arin-test/from-thread")])
                .unwrap()
        });
    });
    assert_eq!(
        board.files.read_file_urls().unwrap(),
        vec![PathBuf::from("/tmp/arin-test/from-thread")]
    );
}

// ---- Finder で表示・既定のアプリで開く(T40d。FR-018) ----
//
// 自動テストでは Finder もアプリも開かない。OS に頼む前に失敗する経路(無いパス・相対パス)だけを確かめる。
// 本当に開く確認は `#[ignore]` のテストで、人間が明示したときだけ動かす。

/// システムの一時領域の下に、このテストだけのフォルダを作る(抜けるときに消す)。
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "arin-test-desktop-{name}-{}-{n}",
            std::process::id()
        ));
        std::fs::create_dir(&dir).expect("一時フォルダを作れるはず");
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn 無いパスの_finder_で表示と開くは失敗を返す() {
    let dir = TempDir::new("missing");
    let missing = dir.0.join("無い ファイル.txt");
    let desktop = MacDesktop::new();

    assert!(matches!(
        desktop.reveal_in_finder(&missing),
        Err(DesktopError::NotFound(_))
    ));
    assert!(matches!(
        desktop.open_with_default_app(&missing),
        Err(DesktopError::NotFound(_))
    ));
}

#[test]
fn 相対パスの_finder_で表示と開くは_os_に渡さず失敗を返す() {
    let desktop = MacDesktop::new();

    assert!(matches!(
        desktop.reveal_in_finder(Path::new("Cargo.toml")),
        Err(DesktopError::Failed(_))
    ));
    assert!(matches!(
        desktop.open_with_default_app(Path::new("Cargo.toml")),
        Err(DesktopError::Failed(_))
    ));
}

#[test]
fn 先が無いシンボリックリンクは開けないが_finder_で表示する対象にはなる() {
    let dir = TempDir::new("broken-link");
    let link = dir.0.join("link");
    std::os::unix::fs::symlink(dir.0.join("nowhere"), &link).unwrap();

    // 開くは先をたどるので、先が無ければ OS に頼まずに見つからない
    assert!(matches!(
        MacDesktop::new().open_with_default_app(&link),
        Err(DesktopError::NotFound(_))
    ));
    // Finder で表示はリンクそのものを選ぶ。OS へは頼まず、頼む直前の検査を通ることだけを確かめる
    assert_eq!(arin_platform::desktop::check_revealable(&link), Ok(()));
}

#[test]
#[ignore = "Finder のウィンドウを本当に開く。人間が明示したときだけ: cargo test -p arin-platform --test desktop -- --ignored"]
fn 手動_finder_で一時ファイルを選んで表示する() {
    let dir = TempDir::new("reveal");
    let file = dir.0.join("Arin で表示 確認.txt");
    std::fs::write(&file, "T40d").unwrap();

    MacDesktop::new()
        .reveal_in_finder(&file)
        .expect("表示できるはず");
    // 目で確かめる時間を取る(消す前に Finder が選ぶため)
    std::thread::sleep(std::time::Duration::from_secs(3));
}

#[test]
#[ignore = "既定のアプリを本当に起動する。人間が明示したときだけ: cargo test -p arin-platform --test desktop -- --ignored"]
fn 手動_既定のアプリで一時ファイルを開く() {
    let dir = TempDir::new("open");
    let file = dir.0.join("Arin で開く 確認.txt");
    std::fs::write(&file, "T40d").unwrap();

    MacDesktop::new()
        .open_with_default_app(&file)
        .expect("開けるはず");
    std::thread::sleep(std::time::Duration::from_secs(3));
}

/// フォルダ選択のダイアログ(T33c)。ダイアログそのものは GUI を出すので自動テストでは開かない。
/// メインスレッドの外からは開かずに失敗することだけを確かめる(別のスレッドを明示的に起こして呼ぶ)。
#[test]
fn フォルダ選択のダイアログはメインスレッドの外では開かずに失敗する() {
    let result = std::thread::spawn(|| arin_platform::desktop::choose_folder("追加"))
        .join()
        .expect("スレッドが panic した");

    assert_eq!(result, Err(arin_platform::desktop::NotMainThread));
}
