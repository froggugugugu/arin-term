//! 起動スモークの印のファイルを書く処理(`arin_platform::storage::write_ready_marker`)。
//!
//! システムの一時領域の下に、このテスト専用のフォルダを作ってその中だけで行う。

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use arin_platform::storage::write_ready_marker;

/// このテストだけが使う一時フォルダ。抜けるときに自分が作ったフォルダだけを消す。
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("時計が 1970 年より前")
            .as_nanos();
        let base = std::env::temp_dir();
        let dir = base.join(format!(
            "arin-platform-test-{name}-{}-{nanos}",
            std::process::id()
        ));
        assert!(dir.starts_with(&base), "一時領域の外を使おうとした");
        fs::create_dir(&dir).expect("一時フォルダを作れなかった");
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn 印のファイルに中身を書く() {
    let dir = TempDir::new("write");
    let marker = dir.path().join("smoke-ready");

    write_ready_marker(&marker, "123\n").expect("書けるはず");

    assert_eq!(fs::read_to_string(&marker).unwrap(), "123\n");
}

#[test]
fn すでに印があれば置き換える() {
    let dir = TempDir::new("replace");
    let marker = dir.path().join("smoke-ready");
    fs::write(&marker, "old").unwrap();

    write_ready_marker(&marker, "new").expect("書けるはず");

    assert_eq!(fs::read_to_string(&marker).unwrap(), "new");
}

#[test]
fn 書いた後に途中のファイルを残さない() {
    let dir = TempDir::new("no-temp");
    let marker = dir.path().join("smoke-ready");

    write_ready_marker(&marker, "1").unwrap();

    let names: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, vec![std::ffi::OsString::from("smoke-ready")]);
}

#[test]
fn フォルダが無ければ作らずに失敗する() {
    let dir = TempDir::new("missing");
    let missing = dir.path().join("not-here");
    let marker = missing.join("smoke-ready");

    assert!(write_ready_marker(&marker, "1").is_err());
    assert!(!missing.exists(), "フォルダを勝手に作った");
}
