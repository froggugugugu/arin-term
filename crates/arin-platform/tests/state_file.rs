//! 状態ファイルの読み書きと退避(`arin_platform::storage::FsStateFile`、T31a)。
//!
//! システムの一時領域の下に、このテスト専用のフォルダを作ってその中だけで行う。
//! 本物の `~/Library/Application Support/Arin/` は使わない。

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use arin_core::ports::state_file::{STATE_FILE_NAME, StateFileError, StateFilePort, StateFileRead};
use arin_platform::storage::FsStateFile;

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
            "arin-platform-test-state-{name}-{}-{nanos}",
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
        // 権限を落としたフォルダがあっても消せるように戻してから消す。
        let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o700));
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn names_in(dir: &Path) -> BTreeSet<OsString> {
    fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect()
}

fn names(list: &[&str]) -> BTreeSet<OsString> {
    list.iter().map(OsString::from).collect()
}

fn mode_of(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn 無ければ無いと返す() {
    let dir = TempDir::new("missing");
    let state = FsStateFile::new(dir.path().join("Arin"));

    assert_eq!(state.read(), Ok(StateFileRead::Missing));
    assert!(
        !dir.path().join("Arin").exists(),
        "読むだけでフォルダを作った"
    );
}

#[test]
fn 書いた中身をそのまま読み戻せる() {
    let dir = TempDir::new("roundtrip");
    let state = FsStateFile::new(dir.path().to_path_buf());

    state.write(b"{\"schemaVersion\":1}").expect("書けるはず");

    assert_eq!(
        state.read(),
        Ok(StateFileRead::Contents(b"{\"schemaVersion\":1}".to_vec()))
    );
}

#[test]
fn 中身が_utf8_でなくてもそのまま返す() {
    let dir = TempDir::new("bytes");
    fs::write(dir.path().join(STATE_FILE_NAME), [0xff, 0xfe, 0x00]).unwrap();
    let state = FsStateFile::new(dir.path().to_path_buf());

    assert_eq!(
        state.read(),
        Ok(StateFileRead::Contents(vec![0xff, 0xfe, 0x00]))
    );
}

#[test]
fn 無い以外の理由で読めなければ_io_の失敗を返す() {
    let dir = TempDir::new("unreadable");
    fs::create_dir(dir.path().join(STATE_FILE_NAME)).unwrap();
    let state = FsStateFile::new(dir.path().to_path_buf());

    assert!(matches!(state.read(), Err(StateFileError::Io(_))));
}

#[test]
fn 書くと同じフォルダの一時ファイルから置き換える() {
    let dir = TempDir::new("replace");
    let target = dir.path().join(STATE_FILE_NAME);
    fs::write(&target, "old").unwrap();
    // 元のファイルに別の名前を付けておく。その場で書き換えたなら、この名前の中身も変わる。
    let witness = dir.path().join("witness");
    fs::hard_link(&target, &witness).unwrap();
    let old_inode = fs::metadata(&target).unwrap().ino();
    let state = FsStateFile::new(dir.path().to_path_buf());

    state.write(b"new").expect("書けるはず");

    assert_eq!(fs::read_to_string(&target).unwrap(), "new");
    assert_ne!(
        fs::metadata(&target).unwrap().ino(),
        old_inode,
        "別のファイルからの置き換えになっていない"
    );
    assert_eq!(
        fs::read_to_string(&witness).unwrap(),
        "old",
        "元のファイルをその場で書き換えた"
    );
    assert_eq!(
        names_in(dir.path()),
        names(&[STATE_FILE_NAME, "witness"]),
        "一時ファイルが残った"
    );
}

#[test]
fn 状態フォルダは_0700_ファイルは_0600() {
    let dir = TempDir::new("mode");
    let state_dir = dir.path().join("Arin");
    let state = FsStateFile::new(state_dir.clone());

    state.write(b"{}").expect("書けるはず");

    assert_eq!(mode_of(&state_dir), 0o700);
    assert_eq!(mode_of(&state_dir.join(STATE_FILE_NAME)), 0o600);
}

#[test]
fn 既存の状態フォルダとファイルの権限も_0700_と_0600_にする() {
    let dir = TempDir::new("mode-existing");
    let state_dir = dir.path().join("Arin");
    fs::create_dir(&state_dir).unwrap();
    fs::set_permissions(&state_dir, fs::Permissions::from_mode(0o755)).unwrap();
    let target = state_dir.join(STATE_FILE_NAME);
    fs::write(&target, "old").unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).unwrap();
    let state = FsStateFile::new(state_dir.clone());

    state.write(b"new").expect("書けるはず");

    assert_eq!(mode_of(&state_dir), 0o700);
    assert_eq!(mode_of(&target), 0o600);
}

#[test]
fn 状態フォルダの親が無ければ作らずに失敗する() {
    let dir = TempDir::new("no-parent");
    let parent = dir.path().join("not-here");
    let state = FsStateFile::new(parent.join("Arin"));

    assert!(matches!(state.write(b"{}"), Err(StateFileError::Io(_))));
    assert!(!parent.exists(), "親のフォルダを勝手に作った");
}

#[test]
fn 一時ファイルの段階で失敗しても元のファイルが残る() {
    let dir = TempDir::new("temp-fails");
    let target = dir.path().join(STATE_FILE_NAME);
    fs::write(&target, "old").unwrap();
    // 一時ファイルが作れないよう、一時ファイルの名前の場所に中身の入ったフォルダを置く。
    let blocker = dir.path().join(format!("{STATE_FILE_NAME}.tmp"));
    fs::create_dir(&blocker).unwrap();
    fs::write(blocker.join("keep"), "x").unwrap();
    let state = FsStateFile::new(dir.path().to_path_buf());

    assert!(matches!(state.write(b"new"), Err(StateFileError::Io(_))));

    assert_eq!(fs::read_to_string(&target).unwrap(), "old");
    assert_eq!(fs::read_to_string(blocker.join("keep")).unwrap(), "x");
}

#[test]
fn 前回の一時ファイルが残っていても書ける() {
    let dir = TempDir::new("stale-temp");
    fs::write(dir.path().join(format!("{STATE_FILE_NAME}.tmp")), "half").unwrap();
    let state = FsStateFile::new(dir.path().to_path_buf());

    state.write(b"new").expect("書けるはず");

    assert_eq!(
        fs::read_to_string(dir.path().join(STATE_FILE_NAME)).unwrap(),
        "new"
    );
    assert_eq!(names_in(dir.path()), names(&[STATE_FILE_NAME]));
}

#[test]
fn 一時ファイルの名前のシンボリックリンクの先に書かない() {
    let dir = TempDir::new("temp-symlink");
    let state_dir = dir.path().join("Arin");
    fs::create_dir(&state_dir).unwrap();
    let outside = dir.path().join("outside");
    fs::write(&outside, "outside").unwrap();
    std::os::unix::fs::symlink(&outside, state_dir.join(format!("{STATE_FILE_NAME}.tmp"))).unwrap();
    let state = FsStateFile::new(state_dir.clone());

    state.write(b"new").expect("書けるはず");

    assert_eq!(fs::read_to_string(&outside).unwrap(), "outside");
    assert_eq!(
        fs::read_to_string(state_dir.join(STATE_FILE_NAME)).unwrap(),
        "new"
    );
}

#[test]
fn 退避は改名だけで中身を変えない() {
    let dir = TempDir::new("move-aside");
    let target = dir.path().join(STATE_FILE_NAME);
    let broken = [b'{', 0xff, b'x'];
    fs::write(&target, broken).unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
    let old_inode = fs::metadata(&target).unwrap().ino();
    let state = FsStateFile::new(dir.path().to_path_buf());
    let backup_name = "state.json.broken-20261115T093000Z";

    state.move_aside(backup_name).expect("退避できるはず");

    let backup = dir.path().join(backup_name);
    assert_eq!(fs::read(&backup).unwrap(), broken);
    assert_eq!(fs::metadata(&backup).unwrap().ino(), old_inode);
    assert_eq!(state.read(), Ok(StateFileRead::Missing));
    assert_eq!(names_in(dir.path()), names(&[backup_name]));
}

#[test]
fn 退避先と同じ名前があれば上書きせず失敗する() {
    let dir = TempDir::new("move-aside-exists");
    let target = dir.path().join(STATE_FILE_NAME);
    fs::write(&target, "broken").unwrap();
    let backup_name = "state.json.broken-20261115T093000Z";
    let backup = dir.path().join(backup_name);
    fs::write(&backup, "earlier").unwrap();
    let state = FsStateFile::new(dir.path().to_path_buf());

    assert!(matches!(
        state.move_aside(backup_name),
        Err(StateFileError::BackupExists(_))
    ));

    assert_eq!(fs::read_to_string(&backup).unwrap(), "earlier");
    assert_eq!(fs::read_to_string(&target).unwrap(), "broken");
}

#[test]
fn 状態ファイルが無ければ退避は失敗し何も作らない() {
    let dir = TempDir::new("move-aside-missing");
    let state = FsStateFile::new(dir.path().to_path_buf());

    assert!(matches!(
        state.move_aside("state.json.broken-20261115T093000Z"),
        Err(StateFileError::Io(_))
    ));
    assert!(names_in(dir.path()).is_empty());
}

#[test]
fn 渡したフォルダの外に書かない() {
    let dir = TempDir::new("outside");
    let state_dir = dir.path().join("Arin");
    fs::create_dir(&state_dir).unwrap();
    let target = state_dir.join(STATE_FILE_NAME);
    fs::write(&target, "broken").unwrap();
    let state = FsStateFile::new(state_dir.clone());
    let absolute = dir.path().join("abs").display().to_string();

    for name in [
        "",
        ".",
        "..",
        "../escaped",
        "sub/name",
        "/tmp/arin-escaped",
        absolute.as_str(),
        "a\0b",
    ] {
        assert!(
            matches!(
                state.move_aside(name),
                Err(StateFileError::InvalidBackupName(_))
            ),
            "{name:?} を断らなかった"
        );
    }

    state.write(b"{}").expect("書けるはず");

    assert_eq!(names_in(dir.path()), names(&["Arin"]));
    assert_eq!(names_in(&state_dir), names(&[STATE_FILE_NAME]));
}
