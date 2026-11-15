//! ルートフォルダの口の結合テスト(`arin_platform::watch::FsRootLocator`、`arin_platform::bookmark::MacBookmarks`)。
//! FR-007・FR-010、設計書 §1.4 #12・#13、§7.1「ルートフォルダへの追従」。T31b。
//!
//! - フォルダは `std::env::temp_dir()` の下に、プロセス ID と連番を含む名前で作り、抜けるときに消す
//! - macOS の一時フォルダは `/var` → `/private/var` のシンボリックリンクなので、基準のパスは実パスにそろえる
//! - 本物の `~/Library/Application Support/Arin/` は作らない・触らない

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use arin_core::ports::{BookmarkPort, RootError, RootLocatorPort};
use arin_platform::bookmark::MacBookmarks;
use arin_platform::watch::FsRootLocator;

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// このテストだけが使う一時フォルダ。抜けるときに自分が作ったフォルダだけを消す。
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let base = std::env::temp_dir();
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = base.join(format!(
            "arin-platform-root-{name}-{}-{n}",
            std::process::id()
        ));
        assert!(dir.starts_with(&base), "一時領域の外を使おうとした");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir(&dir).expect("一時フォルダを作れなかった");
        Self(fs::canonicalize(&dir).expect("一時フォルダの実パスを取れなかった"))
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn mkdir(&self, rel: &str) -> PathBuf {
        let p = self.0.join(rel);
        fs::create_dir_all(&p).expect("フォルダを作れなかった");
        p
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn locator() -> FsRootLocator {
    FsRootLocator::new()
}

// ---- canonical_dir ----

#[test]
fn 登録するフォルダは正規化したパスになる() {
    let tmp = TempDir::new("canon");
    let dir = tmp.mkdir("Proj");
    assert_eq!(locator().canonical_dir(&dir), Ok(dir.clone()));
    // 「.」「..」を含んでも同じ
    let messy = tmp.path().join("Proj").join(".").join("..").join("Proj");
    assert_eq!(locator().canonical_dir(&messy), Ok(dir));
}

#[test]
fn シンボリックリンクを経由しても同じパスになる() {
    let tmp = TempDir::new("link");
    let dir = tmp.mkdir("Real");
    let link = tmp.path().join("alias");
    symlink(&dir, &link).expect("リンクを作れなかった");
    assert_eq!(locator().canonical_dir(&link), Ok(dir));
}

#[test]
fn 大文字小文字の違うパスはディスク上の表記にそろう() {
    let tmp = TempDir::new("case");
    let dir = tmp.mkdir("MixedCase");
    // APFS(既定)は大文字・小文字を区別しない。区別するボリュームなら別のパスとして無いことになる
    let lower = tmp.path().join("mixedcase");
    let upper = tmp.path().join("MIXEDCASE");
    if !lower.exists() {
        eprintln!("大文字・小文字を区別するボリュームなので確かめない");
        return;
    }
    assert_eq!(locator().canonical_dir(&lower), Ok(dir.clone()));
    assert_eq!(locator().canonical_dir(&upper), Ok(dir.clone()));
    // 一時フォルダ自身の名前の表記が違っても、全体がディスク上の表記にそろう
    let name = tmp
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_uppercase();
    let parent_upper = tmp.path().parent().unwrap().join(name).join("mixedcase");
    assert_eq!(locator().canonical_dir(&parent_upper), Ok(dir));
}

#[test]
fn フォルダでないものは登録できない() {
    let tmp = TempDir::new("notdir");
    let file = tmp.path().join("file.txt");
    fs::write(&file, b"x").unwrap();
    assert!(matches!(
        locator().canonical_dir(&file),
        Err(RootError::NotADirectory(_))
    ));
    // ファイルを指すシンボリックリンクも同じ
    let link = tmp.path().join("file-link");
    symlink(&file, &link).unwrap();
    assert!(matches!(
        locator().canonical_dir(&link),
        Err(RootError::NotADirectory(_))
    ));
}

#[test]
fn 無いパスは見つからない() {
    let tmp = TempDir::new("missing");
    assert!(matches!(
        locator().canonical_dir(&tmp.path().join("nope")),
        Err(RootError::NotFound(_))
    ));
    // 先が無いシンボリックリンクも同じ
    let link = tmp.path().join("dangling");
    symlink(tmp.path().join("gone"), &link).unwrap();
    assert!(matches!(
        locator().canonical_dir(&link),
        Err(RootError::NotFound(_))
    ));
}

// ---- open / current_path ----

#[test]
fn 開いたフォルダは改名と移動のあとも現在のパスが取れる() {
    let tmp = TempDir::new("follow");
    let dir = tmp.mkdir("before");
    let root = locator().open(&dir).expect("開けるはず");
    assert_eq!(root.current_path(), Ok(dir.clone()));

    let renamed = tmp.path().join("after");
    fs::rename(&dir, &renamed).unwrap();
    assert_eq!(root.current_path(), Ok(renamed.clone()));

    let other_parent = tmp.mkdir("elsewhere");
    let moved = other_parent.join("after");
    fs::rename(&renamed, &moved).unwrap();
    assert_eq!(root.current_path(), Ok(moved));
}

#[test]
fn 開いたフォルダの現在のパスはディスク上の表記になる() {
    let tmp = TempDir::new("opencase");
    let dir = tmp.mkdir("CaseRoot");
    let lower = tmp.path().join("caseroot");
    if !lower.exists() {
        eprintln!("大文字・小文字を区別するボリュームなので確かめない");
        return;
    }
    let root = locator().open(&lower).expect("開けるはず");
    assert_eq!(root.current_path(), Ok(dir));
}

#[test]
fn 削除後は現在のパスが取れない() {
    let tmp = TempDir::new("delete");
    let dir = tmp.mkdir("doomed");
    let root = locator().open(&dir).expect("開けるはず");
    fs::remove_dir(&dir).unwrap();
    let got = root.current_path();
    assert!(matches!(got, Err(RootError::NotFound(_))), "{got:?}");
}

#[test]
fn 削除後に同じパスへ別のフォルダを作っても現在のパスは取れない() {
    let tmp = TempDir::new("recreate");
    let dir = tmp.mkdir("again");
    let root = locator().open(&dir).expect("開けるはず");
    fs::remove_dir(&dir).unwrap();
    fs::create_dir(&dir).unwrap();
    let got = root.current_path();
    assert!(matches!(got, Err(RootError::NotFound(_))), "{got:?}");
}

#[test]
fn 削除された中身のあるフォルダも現在のパスは取れない() {
    let tmp = TempDir::new("deltree");
    let dir = tmp.mkdir("tree/sub");
    fs::write(dir.join("f"), b"x").unwrap();
    let root = locator()
        .open(&tmp.path().join("tree"))
        .expect("開けるはず");
    fs::remove_dir_all(tmp.path().join("tree")).unwrap();
    let got = root.current_path();
    assert!(matches!(got, Err(RootError::NotFound(_))), "{got:?}");
}

#[test]
fn フォルダでないものや無いものは開けない() {
    let tmp = TempDir::new("openbad");
    let file = tmp.path().join("f");
    fs::write(&file, b"x").unwrap();
    assert!(matches!(
        locator().open(&file),
        Err(RootError::NotADirectory(_))
    ));
    assert!(matches!(
        locator().open(&tmp.path().join("nope")),
        Err(RootError::NotFound(_))
    ));
}

#[test]
fn 開いたフォルダはスレッドをまたいで使える() {
    let tmp = TempDir::new("thread");
    let dir = tmp.mkdir("t");
    let root = locator().open(&dir).unwrap();
    let got = std::thread::spawn(move || root.current_path())
        .join()
        .unwrap();
    assert_eq!(got, Ok(dir));
}

// ---- BookmarkPort ----

#[test]
fn ブックマークの文字列は解決すると同じパスになる() {
    let tmp = TempDir::new("bm-same");
    let dir = tmp.mkdir("proj");
    let b = MacBookmarks::new();
    let text = b.create(&dir).expect("作れるはず");
    assert!(!text.is_empty());
    assert!(text.is_ascii(), "base64 の文字列");
    let r = b.resolve(&text).expect("解決できるはず");
    assert_eq!(r.path, dir);
    assert!(!r.stale);
}

#[test]
fn ブックマークは改名後に新しいパスへ解決される() {
    let tmp = TempDir::new("bm-rename");
    let dir = tmp.mkdir("old");
    let b = MacBookmarks::new();
    let text = b.create(&dir).unwrap();
    let renamed = tmp.path().join("new");
    fs::rename(&dir, &renamed).unwrap();
    let r = b.resolve(&text).unwrap();
    assert_eq!(r.path, renamed);
    assert!(r.stale);
}

#[test]
fn ブックマークは移動後に新しいパスへ解決される() {
    let tmp = TempDir::new("bm-move");
    let dir = tmp.mkdir("a/proj");
    let b = MacBookmarks::new();
    let text = b.create(&dir).unwrap();
    let moved = tmp.mkdir("b").join("proj");
    fs::rename(&dir, &moved).unwrap();
    let r = b.resolve(&text).unwrap();
    assert_eq!(r.path, moved);
    assert!(r.stale);
}

#[test]
fn 削除したフォルダのブックマークは解決できない() {
    let tmp = TempDir::new("bm-delete");
    let dir = tmp.mkdir("gone");
    let b = MacBookmarks::new();
    let text = b.create(&dir).unwrap();
    fs::remove_dir(&dir).unwrap();
    assert!(matches!(b.resolve(&text), Err(RootError::NotFound(_))));
}

#[test]
fn 無いフォルダのブックマークは作れない() {
    let tmp = TempDir::new("bm-missing");
    assert!(matches!(
        MacBookmarks::new().create(&tmp.path().join("nope")),
        Err(RootError::NotFound(_))
    ));
}

#[test]
fn 壊れたブックマークの文字列は解決できない() {
    let b = MacBookmarks::new();
    for text in ["", "!!!not base64!!!", "AAAA"] {
        assert!(
            matches!(b.resolve(text), Err(RootError::Bookmark(_))),
            "{text:?}"
        );
    }
}
