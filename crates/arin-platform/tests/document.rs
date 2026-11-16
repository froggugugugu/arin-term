//! 文書の読み書きの口の結合テスト(`arin_platform::fs::LocalDocuments`。ADR-015 の 5、計画書 T81c)。
//!
//! - フォルダは `std::env::temp_dir()` の実パス(`canonicalize`)の下に、プロセス ID と連番を含む名前で作り、抜けるときに消す
//! - 本物のホーム・状態フォルダ・利用者のファイルには触れない
//! - 拡張属性は `libc::setxattr` で付け、`libc::getxattr` で読む

use std::ffi::CString;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use arin_core::editor::{DOCUMENT_LIMIT_BYTES, ReadOnlyReason, digest};
use arin_core::ports::document::{DocumentAccessError, DocumentPort, ReplaceOutcome};
use arin_core::tree::exclusion::WORK_NAME_PREFIXES;
use arin_platform::fs::{LocalDocuments, SAVE_PREFIX, STAGING_PREFIX};

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// このテストだけが使う一時フォルダ(実パス)。抜けるときに権限を戻してから消す。
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let base = fs::canonicalize(std::env::temp_dir()).expect("一時領域の実パス");
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = base.join(format!(
            "arin-platform-document-{name}-{}-{n}",
            std::process::id()
        ));
        assert!(dir.starts_with(&base) && dir != base);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir(&dir).expect("一時フォルダを作れなかった");
        Self(dir)
    }

    fn at(&self, rel: &str) -> PathBuf {
        let p = self.0.join(rel);
        assert!(p.starts_with(&self.0));
        p
    }

    fn write(&self, rel: &str, bytes: &[u8]) -> PathBuf {
        let p = self.at(rel);
        fs::write(&p, bytes).expect("書けなかった");
        p
    }

    /// フォルダの中の名前(並べ替えたもの)。
    fn names(&self, rel: &str) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.at(rel))
            .expect("読めなかった")
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        // 書けなくしたフォルダ・ファイルを戻してから消す(この一時フォルダの中だけ)。
        let mut stack = vec![self.0.clone()];
        while let Some(dir) = stack.pop() {
            let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o755));
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if entry.file_type().is_ok_and(|t| t.is_dir()) {
                        stack.push(path);
                    }
                }
            }
        }
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn docs() -> LocalDocuments {
    LocalDocuments::new()
}

fn set_xattr(path: &Path, name: &str, value: &[u8]) {
    let path_c = CString::new(path.as_os_str().as_bytes()).unwrap();
    let name_c = CString::new(name).unwrap();
    // SAFETY: どちらも NUL で終わる有効な C 文字列、値はこの呼び出しの間生きている
    let rc = unsafe {
        libc::setxattr(
            path_c.as_ptr(),
            name_c.as_ptr(),
            value.as_ptr().cast(),
            value.len(),
            0,
            libc::XATTR_NOFOLLOW,
        )
    };
    assert_eq!(rc, 0, "拡張属性を付けられなかった");
}

fn get_xattr(path: &Path, name: &str) -> Option<Vec<u8>> {
    let path_c = CString::new(path.as_os_str().as_bytes()).unwrap();
    let name_c = CString::new(name).unwrap();
    let mut buf = vec![0u8; 256];
    // SAFETY: どちらも NUL で終わる有効な C 文字列、buf は書き込める領域
    let n = unsafe {
        libc::getxattr(
            path_c.as_ptr(),
            name_c.as_ptr(),
            buf.as_mut_ptr().cast(),
            buf.len(),
            0,
            libc::XATTR_NOFOLLOW,
        )
    };
    let n = usize::try_from(n).ok()?;
    buf.truncate(n);
    Some(buf)
}

#[test]
fn 読むと中身と版と読み取り専用でないことを返す() {
    let t = TempDir::new("read");
    let path = t.write("README.md", b"# hello\n");
    let meta = fs::metadata(&path).unwrap();

    let read = docs().read(&path).expect("読めなかった");

    assert_eq!(read.bytes, b"# hello\n");
    assert_eq!(read.read_only, None);
    assert_eq!(read.stamp.stat.ino, meta.ino());
    assert_eq!(read.stamp.stat.dev, meta.dev() as u64);
    assert_eq!(read.stamp.stat.size, 8);
    assert_eq!(
        read.stamp.stat.mtime_ns,
        meta.mtime() * 1_000_000_000 + meta.mtime_nsec()
    );
    assert_eq!(read.stamp.digest, digest(b"# hello\n"));
    // stat も同じ版を返す。
    assert_eq!(docs().stat(&path).unwrap(), read.stamp);
}

#[test]
fn リンクは開かない() {
    let t = TempDir::new("link");
    let target = t.write("real.md", b"x");
    let link = t.at("link.md");
    symlink(&target, &link).unwrap();

    assert!(matches!(
        docs().read(&link),
        Err(DocumentAccessError::Link(_))
    ));
    assert!(matches!(
        docs().replace(&link, b"y", None),
        Err(DocumentAccessError::Link(_))
    ));
    assert_eq!(fs::read(&target).unwrap(), b"x", "リンクの先を書き換えた");
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn フォルダとふつうでないものは開かない() {
    let t = TempDir::new("kind");
    fs::create_dir(t.at("dir.md")).unwrap();
    assert!(matches!(
        docs().read(&t.at("dir.md")),
        Err(DocumentAccessError::Directory(_))
    ));
}

#[test]
fn 途中のフォルダがリンクなら失敗() {
    let t = TempDir::new("mid-link");
    fs::create_dir(t.at("real")).unwrap();
    t.write("real/a.md", b"x");
    symlink(t.at("real"), t.at("via")).unwrap();

    assert!(matches!(
        docs().read(&t.at("via/a.md")),
        Err(DocumentAccessError::Io(_))
    ));
    assert!(docs().replace(&t.at("via/a.md"), b"y", None).is_err());
    assert!(docs().create_new(&t.at("via/new.md"), b"y").is_err());
    assert_eq!(fs::read(t.at("real/a.md")).unwrap(), b"x");
    assert_eq!(t.names("real"), vec!["a.md"]);
}

#[test]
fn ハードリンクは読み取り専用() {
    let t = TempDir::new("hard");
    let path = t.write("a.md", b"x");
    fs::hard_link(&path, t.at("b.md")).unwrap();

    let read = docs().read(&path).unwrap();
    assert_eq!(read.read_only, Some(ReadOnlyReason::HardLink));

    // 置き換えない(リンクが切れるため)。
    assert_eq!(
        docs().replace(&path, b"y", Some(&read.stamp)),
        Err(DocumentAccessError::ReadOnly(ReadOnlyReason::HardLink))
    );
    assert_eq!(fs::read(t.at("b.md")).unwrap(), b"x");
    assert_eq!(t.names(""), vec!["a.md", "b.md"]);
}

#[test]
fn 書けないファイルは読み取り専用() {
    let t = TempDir::new("ro-file");
    let path = t.write("a.md", b"x");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();

    let read = docs().read(&path).unwrap();
    assert_eq!(read.read_only, Some(ReadOnlyReason::NotWritable));
    assert_eq!(
        docs().replace(&path, b"y", Some(&read.stamp)),
        Err(DocumentAccessError::ReadOnly(ReadOnlyReason::NotWritable))
    );
    assert_eq!(fs::read(&path).unwrap(), b"x");
}

#[test]
fn 書けないフォルダのファイルは読み取り専用() {
    let t = TempDir::new("ro-dir-read");
    fs::create_dir(t.at("d")).unwrap();
    let path = t.write("d/a.md", b"x");
    fs::set_permissions(t.at("d"), fs::Permissions::from_mode(0o555)).unwrap();

    assert_eq!(
        docs().read(&path).unwrap().read_only,
        Some(ReadOnlyReason::FolderNotWritable)
    );
}

#[test]
fn 置き換えると中身が変わり権限と拡張属性が残る() {
    let t = TempDir::new("replace");
    let path = t.write("a.md", b"old");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    set_xattr(&path, "com.example.arin-test", b"tag");
    let read = docs().read(&path).unwrap();

    let outcome = docs()
        .replace(&path, b"new content", Some(&read.stamp))
        .expect("置き換えられなかった");

    let ReplaceOutcome::Replaced(stamp) = outcome else {
        panic!("置き換えなかった: {outcome:?}");
    };
    assert_eq!(fs::read(&path).unwrap(), b"new content");
    let meta = fs::metadata(&path).unwrap();
    assert_eq!(meta.permissions().mode() & 0o7777, 0o640);
    assert_eq!(
        get_xattr(&path, "com.example.arin-test").as_deref(),
        Some(&b"tag"[..])
    );
    // 新しい版は置き換えたあとのファイルのもの。
    assert_eq!(stamp.stat.ino, meta.ino());
    assert_eq!(stamp.stat.size, 11);
    assert_eq!(stamp.digest, digest(b"new content"));
    assert_eq!(docs().stat(&path).unwrap(), stamp);
}

#[test]
fn 版が違えば置き換えず何も変えない() {
    let t = TempDir::new("changed");
    let path = t.write("a.md", b"mine");
    let read = docs().read(&path).unwrap();
    // 外で書き換えられた(大きさも変わる)。
    fs::write(&path, b"theirs!").unwrap();

    let outcome = docs().replace(&path, b"edited", Some(&read.stamp)).unwrap();

    let ReplaceOutcome::Changed(current) = outcome else {
        panic!("置き換えた: {outcome:?}");
    };
    assert_eq!(current.digest, digest(b"theirs!"));
    assert_eq!(fs::read(&path).unwrap(), b"theirs!");
    assert_eq!(t.names(""), vec!["a.md"], "一時ファイルを作った");

    // 上書きを選んだら(版を渡さない)置き換える。
    assert!(matches!(
        docs().replace(&path, b"edited", None).unwrap(),
        ReplaceOutcome::Replaced(_)
    ));
    assert_eq!(fs::read(&path).unwrap(), b"edited");
}

#[test]
fn 時刻だけ変わったなら同じ中身として置き換える() {
    let t = TempDir::new("touched");
    let path = t.write("a.md", b"same");
    let read = docs().read(&path).unwrap();
    // 時刻だけを替える(touch)。
    let times = [
        libc::timespec {
            tv_sec: 1_000_000,
            tv_nsec: 0,
        },
        libc::timespec {
            tv_sec: 1_000_000,
            tv_nsec: 0,
        },
    ];
    let path_c = CString::new(path.as_os_str().as_bytes()).unwrap();
    // SAFETY: NUL で終わる有効な C 文字列、times は 2 つの要素を持つ
    assert_eq!(
        unsafe { libc::utimensat(libc::AT_FDCWD, path_c.as_ptr(), times.as_ptr(), 0) },
        0
    );
    assert!(!read.stamp.same_stat(&docs().stat(&path).unwrap().stat));

    let outcome = docs().replace(&path, b"edited", Some(&read.stamp)).unwrap();
    assert!(
        matches!(outcome, ReplaceOutcome::Replaced(_)),
        "{outcome:?}"
    );
    assert_eq!(fs::read(&path).unwrap(), b"edited");
}

#[test]
fn 消えていれば見つからない() {
    let t = TempDir::new("missing");
    let path = t.at("gone.md");

    assert!(matches!(
        docs().read(&path),
        Err(DocumentAccessError::NotFound(_))
    ));
    assert!(matches!(
        docs().stat(&path),
        Err(DocumentAccessError::NotFound(_))
    ));
    assert_eq!(
        docs().replace(&path, b"x", None),
        Ok(ReplaceOutcome::Missing)
    );
    assert!(t.names("").is_empty(), "消えたものを作った");
}

#[test]
fn 作るときは既にあれば上書きしない() {
    let t = TempDir::new("create");
    let path = t.at("new.md");

    let stamp = docs().create_new(&path, b"hello").expect("作れなかった");
    assert_eq!(fs::read(&path).unwrap(), b"hello");
    assert_eq!(stamp.digest, digest(b"hello"));
    assert_eq!(docs().stat(&path).unwrap(), stamp);

    assert!(matches!(
        docs().create_new(&path, b"again"),
        Err(DocumentAccessError::AlreadyExists(_))
    ));
    assert_eq!(fs::read(&path).unwrap(), b"hello");
    assert_eq!(t.names(""), vec!["new.md"]);
}

#[test]
fn 置き換えのあと一時ファイルが残らない() {
    let t = TempDir::new("no-leftover");
    let path = t.write("a.md", b"1");
    let mut stamp = docs().read(&path).unwrap().stamp;
    for n in 2..5u8 {
        let ReplaceOutcome::Replaced(next) =
            docs().replace(&path, &[b'0' + n], Some(&stamp)).unwrap()
        else {
            panic!("置き換えなかった");
        };
        stamp = next;
    }
    assert_eq!(fs::read(&path).unwrap(), b"4");
    assert_eq!(t.names(""), vec!["a.md"]);
}

#[test]
fn 書けないフォルダでは失敗し元の中身のまま() {
    let t = TempDir::new("ro-dir");
    fs::create_dir(t.at("d")).unwrap();
    let path = t.write("d/a.md", b"orig");
    let stamp = docs().read(&path).unwrap().stamp;
    fs::set_permissions(t.at("d"), fs::Permissions::from_mode(0o555)).unwrap();

    assert!(docs().replace(&path, b"new", Some(&stamp)).is_err());
    assert!(docs().create_new(&t.at("d/b.md"), b"new").is_err());

    assert_eq!(fs::read(&path).unwrap(), b"orig");
    assert_eq!(t.names("d"), vec!["a.md"]);
}

#[test]
fn 大きすぎるものは読まず書かない() {
    let t = TempDir::new("large");
    let limit = usize::try_from(DOCUMENT_LIMIT_BYTES).unwrap();
    let path = t.write("big.md", &vec![b'a'; limit + 1]);
    assert_eq!(
        docs().read(&path),
        Err(DocumentAccessError::TooLarge {
            size: DOCUMENT_LIMIT_BYTES + 1
        })
    );

    let small = t.write("small.md", b"x");
    assert_eq!(
        docs().replace(&small, &vec![b'a'; limit + 1], None),
        Err(DocumentAccessError::TooLarge {
            size: DOCUMENT_LIMIT_BYTES + 1
        })
    );
    assert_eq!(fs::read(&small).unwrap(), b"x");
    assert_eq!(t.names(""), vec!["big.md", "small.md"]);
}

/// 知らされた残りの場所。
static REPORTED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

fn record(path: &Path) {
    REPORTED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(path.to_path_buf());
}

#[test]
fn うまくいった保存は残りを知らせない() {
    let t = TempDir::new("report");
    let path = t.write("a.md", b"x");
    let documents = LocalDocuments::with_leftover_report(record);
    documents.replace(&path, b"y", None).unwrap();
    let reported = REPORTED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    assert!(
        reported.iter().all(|p| !p.starts_with(&t.0)),
        "{reported:?}"
    );
}

#[test]
fn 作業用の名前の頭は除外の規則と同じ() {
    assert!(WORK_NAME_PREFIXES.contains(&SAVE_PREFIX));
    assert!(WORK_NAME_PREFIXES.contains(&STAGING_PREFIX));
    assert_eq!(SAVE_PREFIX, ".arin-save-");
}
