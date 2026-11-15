//! macOS のブックマークの結合テスト(`arin_platform::bookmark`)。FR-007、設計書 §1.4 #12。
//!
//! - フォルダは `std::env::temp_dir()` の下に、プロセス ID と連番を含む名前で作り、抜けるときに消す
//! - macOS の一時フォルダは `/var` → `/private/var` のシンボリックリンクなので、パスは `canonicalize` した値で比べる
//! - 本物の `~/Library/Application Support/Arin/` は作らない・触らない

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use arin_platform::bookmark::{
    BookmarkError, Resolved, create_bookmark, decode, encode, resolve_bookmark,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// このテストだけが使う一時フォルダ。抜けるときに自分が作ったフォルダだけを消す。
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let base = std::env::temp_dir();
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = base.join(format!(
            "arin-platform-bookmark-{name}-{}-{n}",
            std::process::id()
        ));
        assert!(dir.starts_with(&base), "一時領域の外を使おうとした");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir(&dir).expect("一時フォルダを作れなかった");
        // 解決で返るのは実パス(/private/var/...)なので、基準を実パスにそろえる
        Self(fs::canonicalize(&dir).expect("一時フォルダの実パスを取れなかった"))
    }

    fn path(&self) -> &Path {
        &self.0
    }

    /// 中にフォルダを作り、そのパスを返す。
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

fn resolved(data: &[u8]) -> Resolved {
    resolve_bookmark(data).expect("解決できるはず")
}

#[test]
fn created_bookmark_resolves_to_the_same_path_and_is_not_stale() {
    let tmp = TempDir::new("same");
    let dir = tmp.mkdir("proj");

    let data = create_bookmark(&dir).expect("作れるはず");
    assert!(!data.is_empty());

    let r = resolved(&data);
    assert_eq!(r.path, dir);
    assert!(!r.stale);
}

#[test]
fn rename_is_followed_with_stale_and_recreating_clears_stale() {
    let tmp = TempDir::new("rename");
    let dir = tmp.mkdir("proj");
    let data = create_bookmark(&dir).unwrap();

    let renamed = tmp.path().join("proj-renamed");
    fs::rename(&dir, &renamed).unwrap();

    let r = resolved(&data);
    assert_eq!(r.path, renamed);
    assert!(r.stale, "改名後は古いと知らせるはず");

    // 返ったパスから作り直すと古くない
    let again = create_bookmark(&r.path).unwrap();
    let r2 = resolved(&again);
    assert_eq!(r2.path, renamed);
    assert!(!r2.stale);
}

#[test]
fn move_to_another_parent_is_followed() {
    let tmp = TempDir::new("move");
    let dir = tmp.mkdir("a/proj");
    tmp.mkdir("b");
    let data = create_bookmark(&dir).unwrap();

    let moved = tmp.path().join("b/proj-moved");
    fs::rename(&dir, &moved).unwrap();

    let r = resolved(&data);
    assert_eq!(r.path, moved);
    assert!(r.stale);
}

#[test]
fn deleted_folder_is_not_found() {
    let tmp = TempDir::new("deleted");
    let dir = tmp.mkdir("proj");
    let data = create_bookmark(&dir).unwrap();
    fs::remove_dir(&dir).unwrap();

    assert_eq!(resolve_bookmark(&data), Err(BookmarkError::NotFound));
}

#[test]
fn creating_from_missing_path_is_not_found() {
    let tmp = TempDir::new("missing");
    let missing = tmp.path().join("no-such");

    assert_eq!(create_bookmark(&missing), Err(BookmarkError::NotFound));
}

#[test]
fn empty_or_garbage_bytes_are_corrupt() {
    assert_eq!(resolve_bookmark(b""), Err(BookmarkError::Corrupt));
    assert_eq!(
        resolve_bookmark(b"not a bookmark"),
        Err(BookmarkError::Corrupt)
    );
}

#[test]
fn string_roundtrip_keeps_bytes_and_still_resolves() {
    let tmp = TempDir::new("string");
    let dir = tmp.mkdir("proj");
    let data = create_bookmark(&dir).unwrap();

    let text = encode(&data);
    assert!(text.is_ascii());
    let back = decode(&text).expect("戻せるはず");
    assert_eq!(back, data);
    assert_eq!(resolved(&back).path, dir);
}

#[test]
fn invalid_string_is_corrupt() {
    assert_eq!(
        decode("これは base64 ではない"),
        Err(BookmarkError::Corrupt)
    );
    assert_eq!(decode("abc"), Err(BookmarkError::Corrupt)); // 長さが足りない
    assert_eq!(decode("YWJj\n"), Err(BookmarkError::Corrupt)); // 改行を含む
}

#[test]
fn works_the_same_from_another_thread() {
    let tmp = TempDir::new("thread");
    let dir = tmp.mkdir("proj");
    let missing = tmp.path().join("no-such");

    let (data, r, from_missing, garbage) = {
        let dir = dir.clone();
        thread::spawn(move || {
            let data = create_bookmark(&dir).unwrap();
            let r = resolve_bookmark(&data).unwrap();
            (
                data,
                r,
                create_bookmark(&missing),
                resolve_bookmark(b"garbage"),
            )
        })
        .join()
        .expect("別スレッドで panic した")
    };

    assert_eq!(
        r,
        Resolved {
            path: dir.clone(),
            stale: false
        }
    );
    assert_eq!(from_missing, Err(BookmarkError::NotFound));
    assert_eq!(garbage, Err(BookmarkError::Corrupt));
    // 別スレッドで作ったものをこのスレッドで解決しても同じ
    assert_eq!(
        resolved(&data),
        Resolved {
            path: dir,
            stale: false
        }
    );
}
