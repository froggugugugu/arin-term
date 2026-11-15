//! ファイルシステムの口の結合テスト(`arin_platform::fs::LocalFileSystem`、T40a)。
//! FR-013、FR-025〜027、FR-029、FR-030、FR-036、設計書 §1.4 #14、§5.4「上書きしない移動・コピー」、
//! 計画書 M3 の【仮定】14・15・24。
//!
//! - フォルダは `std::env::temp_dir()` の下に、プロセス ID と連番を含む名前で作り、抜けるときに消す
//! - 補助は、使うパスがシステムの一時領域の下にあることを確かめてから使う(設計書 §10.2)
//! - 本物のホームやプロジェクトのフォルダには触れない。ファイルの操作は同期の呼び出しだけで、待ちは無い

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use arin_core::ports::fs::{DirEntry, EntryKind, FileSystemPort, FsError};
use arin_platform::fs::LocalFileSystem;

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// システムの一時領域の実パス。
fn temp_base() -> PathBuf {
    fs::canonicalize(std::env::temp_dir()).expect("一時領域の実パスを取れなかった")
}

/// テストで使うパスが一時領域の下にあるかを確かめる。下にあれば実パスにそろえて返す。
///
/// 最後の名前は実パスにしない(リンク自体や、大文字・小文字だけが違う名前を指せるように)。親は在る祖先までを
/// 実パスにし、残りの名前をつなぐ。`..` や `.` を含むものは拒否する。
fn ensure_under_temp(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err(format!("{}: 絶対パスでない", path.display()));
    }
    if path
        .components()
        .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err(format!("{}: `..` か `.` を含む", path.display()));
    }
    let Some(last) = path.file_name() else {
        return Err(format!("{}: 名前が無い", path.display()));
    };
    let mut existing = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut rest = vec![last.to_owned()];
    while fs::symlink_metadata(&existing).is_err() {
        let Some(name) = existing.file_name() else {
            return Err(format!("{}: 在る祖先が無い", path.display()));
        };
        rest.push(name.to_owned());
        existing.pop();
    }
    let mut real = fs::canonicalize(&existing).map_err(|e| format!("{}: {e}", path.display()))?;
    for name in rest.iter().rev() {
        real.push(name);
    }
    let base = temp_base();
    if real == base || !real.starts_with(&base) {
        return Err(format!("{}: 一時領域の下にない", path.display()));
    }
    Ok(real)
}

/// このテストだけが使う一時フォルダ。抜けるときに自分が作ったフォルダだけを消す。
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = temp_base().join(format!(
            "arin-platform-fs-{name}-{}-{n}",
            std::process::id()
        ));
        let dir = ensure_under_temp(&dir).expect("一時領域の外を使おうとした");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir(&dir).expect("一時フォルダを作れなかった");
        Self(dir)
    }

    /// 一時フォルダの中のパス。外に出るものは panic する。
    fn at(&self, rel: &str) -> PathBuf {
        let p = ensure_under_temp(&self.0.join(rel)).expect("一時領域の外を使おうとした");
        assert!(p.starts_with(&self.0), "テストのフォルダの外を使おうとした");
        p
    }

    fn write(&self, rel: &str, contents: &str) -> PathBuf {
        let p = self.at(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).expect("フォルダを作れなかった");
        }
        fs::write(&p, contents).expect("ファイルを書けなかった");
        p
    }

    fn mkdir(&self, rel: &str) -> PathBuf {
        let p = self.at(rel);
        fs::create_dir_all(&p).expect("フォルダを作れなかった");
        p
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        // 権限を落とした項目があっても消せるように戻してから消す
        restore_permissions(&self.0);
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn restore_permissions(dir: &Path) {
    let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                restore_permissions(&p);
            } else if entry.file_type().is_ok_and(|t| t.is_file()) {
                let _ = fs::set_permissions(&p, fs::Permissions::from_mode(0o600));
            }
        }
    }
}

fn local() -> LocalFileSystem {
    LocalFileSystem::new()
}

fn entry(name: &str, kind: EntryKind) -> DirEntry {
    DirEntry {
        name: name.to_owned(),
        kind,
    }
}

fn sorted(mut entries: Vec<DirEntry>) -> Vec<DirEntry> {
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

/// フォルダの中の名前の一覧(並べ替え済み)。
fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("フォルダを読めなかった")
        .map(|e| {
            e.expect("項目を読めなかった")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).expect("ファイルを読めなかった")
}

// ---- 読み込み ----

#[test]
fn フォルダの中身を名前と種類で返す() {
    let tmp = TempDir::new("list");
    tmp.write("a.ts", "a");
    tmp.mkdir("src");
    tmp.write("日本語 名前.md", "j");
    let got = local().read_dir(&tmp.at("")).expect("読めなかった");
    assert_eq!(
        sorted(got),
        vec![
            entry("a.ts", EntryKind::File),
            entry("src", EntryKind::Directory),
            entry("日本語 名前.md", EntryKind::File),
        ]
    );
}

#[test]
fn ファイルへのリンクとフォルダへのリンクを見分ける() {
    let tmp = TempDir::new("links");
    let file = tmp.write("target.txt", "t");
    let dir = tmp.mkdir("target-dir");
    symlink(&file, tmp.at("file-link")).expect("リンクを作れなかった");
    symlink(&dir, tmp.at("dir-link")).expect("リンクを作れなかった");
    let got = sorted(local().read_dir(&tmp.at("")).expect("読めなかった"));
    assert_eq!(
        got,
        vec![
            entry("dir-link", EntryKind::DirectoryLink),
            entry("file-link", EntryKind::FileLink),
            entry("target-dir", EntryKind::Directory),
            entry("target.txt", EntryKind::File),
        ]
    );
    // 項目の種類もリンクをたどらずに返す
    let fs = local();
    assert_eq!(
        fs.entry_kind(&tmp.at("dir-link")),
        Ok(EntryKind::DirectoryLink)
    );
    assert_eq!(fs.entry_kind(&tmp.at("file-link")), Ok(EntryKind::FileLink));
    assert_eq!(
        fs.entry_kind(&tmp.at("target-dir")),
        Ok(EntryKind::Directory)
    );
    assert_eq!(fs.entry_kind(&tmp.at("target.txt")), Ok(EntryKind::File));
}

#[test]
fn 壊れたリンクも項目として返す() {
    let tmp = TempDir::new("broken");
    symlink(tmp.at("nowhere"), tmp.at("broken")).expect("リンクを作れなかった");
    let got = local().read_dir(&tmp.at("")).expect("読めなかった");
    assert_eq!(got, vec![entry("broken", EntryKind::Other)]);
    assert_eq!(local().entry_kind(&tmp.at("broken")), Ok(EntryKind::Other));
}

#[test]
fn 無いフォルダとフォルダでないものの読み込みは種類の分かる失敗を返す() {
    let tmp = TempDir::new("list-err");
    let file = tmp.write("a.txt", "a");
    assert!(matches!(
        local().read_dir(&tmp.at("missing")),
        Err(FsError::NotFound(_))
    ));
    assert!(matches!(
        local().read_dir(&file),
        Err(FsError::NotADirectory(_))
    ));
    assert!(matches!(
        local().entry_kind(&tmp.at("missing")),
        Err(FsError::NotFound(_))
    ));
}

#[test]
fn フォルダの実パスはリンクを解決しファイルなら失敗する() {
    let tmp = TempDir::new("real");
    let dir = tmp.mkdir("Real");
    symlink(&dir, tmp.at("alias")).expect("リンクを作れなかった");
    let file = tmp.write("a.txt", "a");
    assert_eq!(local().real_dir(&tmp.at("alias")), Ok(dir.clone()));
    assert_eq!(local().real_dir(&dir), Ok(dir));
    assert!(matches!(
        local().real_dir(&file),
        Err(FsError::NotADirectory(_))
    ));
    assert!(matches!(
        local().real_dir(&tmp.at("missing")),
        Err(FsError::NotFound(_))
    ));
}

// ---- 作成 ----

#[test]
fn 新規作成は空のファイルを作る() {
    let tmp = TempDir::new("create");
    let path = tmp.at("a.ts");
    local().create_file(&path).expect("作れなかった");
    assert_eq!(read(&path), "");
}

#[test]
fn 新規作成は既にあれば失敗し中身を変えない() {
    let tmp = TempDir::new("create-exists");
    let path = tmp.write("a.ts", "大事な中身");
    let dir = tmp.mkdir("src");
    assert!(matches!(
        local().create_file(&path),
        Err(FsError::AlreadyExists(_))
    ));
    assert_eq!(read(&path), "大事な中身");
    // フォルダがある名前にも作らない
    assert!(matches!(
        local().create_file(&dir),
        Err(FsError::AlreadyExists(_))
    ));
    assert!(dir.is_dir());
    // リンク(壊れていても)がある名前にも作らず、リンクの先にも作らない
    let link = tmp.at("link");
    symlink(tmp.at("target"), &link).expect("リンクを作れなかった");
    assert!(matches!(
        local().create_file(&link),
        Err(FsError::AlreadyExists(_))
    ));
    assert!(fs::symlink_metadata(tmp.at("target")).is_err());
}

#[test]
fn 新規作成は親が無ければ見つからないを返す() {
    let tmp = TempDir::new("create-noparent");
    assert!(matches!(
        local().create_file(&tmp.at("missing/a.ts")),
        Err(FsError::NotFound(_))
    ));
    assert!(matches!(
        local().create_dir(&tmp.at("missing/sub")),
        Err(FsError::NotFound(_))
    ));
    assert_eq!(names_in(&tmp.at("")), Vec::<String>::new());
}

#[test]
fn フォルダの作成は既にあれば失敗する() {
    let tmp = TempDir::new("mkdir");
    let path = tmp.at("src");
    local().create_dir(&path).expect("作れなかった");
    assert!(path.is_dir());
    tmp.write("src/keep.txt", "k");
    assert!(matches!(
        local().create_dir(&path),
        Err(FsError::AlreadyExists(_))
    ));
    assert_eq!(read(&tmp.at("src/keep.txt")), "k");
    // ファイルがある名前にも作らない
    let file = tmp.write("a.txt", "a");
    assert!(matches!(
        local().create_dir(&file),
        Err(FsError::AlreadyExists(_))
    ));
    assert_eq!(read(&file), "a");
}

// ---- 移動 ----

#[test]
fn 移動でファイルの名前が変わる() {
    let tmp = TempDir::new("rename");
    let from = tmp.write("a.ts", "a");
    let to = tmp.at("b.ts");
    local().move_entry(&from, &to).expect("移せなかった");
    assert!(fs::symlink_metadata(&from).is_err());
    assert_eq!(read(&to), "a");
}

#[test]
fn 移動は宛先に同名があれば失敗しどちらも変えない() {
    let tmp = TempDir::new("move-exists");
    let from = tmp.write("a.md", "元");
    let to = tmp.write("docs/a.md", "宛先");
    assert!(matches!(
        local().move_entry(&from, &to),
        Err(FsError::AlreadyExists(_))
    ));
    assert_eq!(read(&from), "元");
    assert_eq!(read(&to), "宛先");
    // フォルダどうしでも同じ
    let dir_from = tmp.mkdir("x");
    tmp.write("x/1.txt", "1");
    let dir_to = tmp.mkdir("y");
    tmp.write("y/2.txt", "2");
    assert!(matches!(
        local().move_entry(&dir_from, &dir_to),
        Err(FsError::AlreadyExists(_))
    ));
    assert_eq!(names_in(&dir_from), vec!["1.txt"]);
    assert_eq!(names_in(&dir_to), vec!["2.txt"]);
    // 空のフォルダの上にも移さない(rename(2) は空のフォルダを置き換えるため)
    let empty = tmp.mkdir("empty");
    assert!(matches!(
        local().move_entry(&dir_from, &empty),
        Err(FsError::AlreadyExists(_))
    ));
    assert_eq!(names_in(&dir_from), vec!["1.txt"]);
    assert!(empty.is_dir());
}

#[test]
fn 移動はフォルダごと動く() {
    let tmp = TempDir::new("move-dir");
    let from = tmp.mkdir("src");
    tmp.write("src/a.ts", "a");
    tmp.write("src/nested/b.ts", "b");
    tmp.mkdir("dest");
    let to = tmp.at("dest/src");
    local().move_entry(&from, &to).expect("移せなかった");
    assert!(fs::symlink_metadata(&from).is_err());
    assert_eq!(read(&tmp.at("dest/src/a.ts")), "a");
    assert_eq!(read(&tmp.at("dest/src/nested/b.ts")), "b");
}

#[test]
fn 移動はリンクの先ではなくリンク自体を動かす() {
    let tmp = TempDir::new("move-link");
    let target = tmp.mkdir("target");
    let link = tmp.at("link");
    symlink(&target, &link).expect("リンクを作れなかった");
    let to = tmp.at("moved-link");
    local().move_entry(&link, &to).expect("移せなかった");
    assert!(target.is_dir());
    assert_eq!(fs::read_link(&to).expect("リンクでない"), target);
}

#[test]
fn 移動の元が無ければ見つからないを返す() {
    let tmp = TempDir::new("move-missing");
    assert!(matches!(
        local().move_entry(&tmp.at("missing"), &tmp.at("b")),
        Err(FsError::NotFound(_))
    ));
    assert_eq!(names_in(&tmp.at("")), Vec::<String>::new());
}

#[test]
fn 大文字小文字だけを変える名称変更ができる() {
    let tmp = TempDir::new("case");
    let from = tmp.write("a.ts", "a");
    let ino = fs::metadata(&from).expect("調べられなかった").ino();
    let to = tmp.at("A.ts");
    local()
        .move_entry(&from, &to)
        .expect("名前を変えられなかった");
    // ディスク上の表記が変わり、項目は同じまま
    assert_eq!(names_in(&tmp.at("")), vec!["A.ts"]);
    assert_eq!(read(&to), "a");
    assert_eq!(fs::metadata(&to).expect("調べられなかった").ino(), ino);
    // フォルダでも同じ
    let dir = tmp.mkdir("docs");
    tmp.write("docs/x.md", "x");
    local()
        .move_entry(&dir, &tmp.at("Docs"))
        .expect("名前を変えられなかった");
    assert_eq!(names_in(&tmp.at("")), vec!["A.ts", "Docs"]);
    assert_eq!(read(&tmp.at("Docs/x.md")), "x");
}

#[test]
fn 大文字小文字だけが違う別の項目への移動は失敗する() {
    // ハードリンクで「同じ項目だが名前が大文字小文字だけ違うのではない」ものは、名称変更扱いにしない
    let tmp = TempDir::new("case-other");
    let from = tmp.write("a.ts", "元");
    let other = tmp.write("b.ts", "別");
    assert!(matches!(
        local().move_entry(&from, &other),
        Err(FsError::AlreadyExists(_))
    ));
    let hard = tmp.at("c.ts");
    fs::hard_link(&from, &hard).expect("ハードリンクを作れなかった");
    assert!(matches!(
        local().move_entry(&from, &hard),
        Err(FsError::AlreadyExists(_))
    ));
    assert_eq!(names_in(&tmp.at("")), vec!["a.ts", "b.ts", "c.ts"]);
    assert_eq!(read(&other), "別");
}

// ---- コピー ----

#[test]
fn コピーは元を残し同じ中身を作る() {
    let tmp = TempDir::new("copy");
    let from = tmp.write("a.md", "中身");
    let to = tmp.at("b.md");
    local().copy_entry(&from, &to).expect("写せなかった");
    assert_eq!(read(&from), "中身");
    assert_eq!(read(&to), "中身");
    assert_ne!(
        fs::metadata(&from).expect("調べられなかった").ino(),
        fs::metadata(&to).expect("調べられなかった").ino()
    );
    // 作業用の名前を残さない
    assert_eq!(names_in(&tmp.at("")), vec!["a.md", "b.md"]);
}

#[test]
fn コピーは宛先に同名があれば失敗しどちらも変えない() {
    let tmp = TempDir::new("copy-exists");
    let from = tmp.write("a.md", "元");
    let to = tmp.write("docs/a.md", "宛先");
    assert!(matches!(
        local().copy_entry(&from, &to),
        Err(FsError::AlreadyExists(_))
    ));
    assert_eq!(read(&from), "元");
    assert_eq!(read(&to), "宛先");
    assert_eq!(names_in(&tmp.at("docs")), vec!["a.md"]);
    // フォルダを既にあるフォルダ(空でも)へ写さない。中へ混ぜることもしない
    let dir_from = tmp.mkdir("x");
    tmp.write("x/1.txt", "1");
    let empty = tmp.mkdir("empty");
    assert!(matches!(
        local().copy_entry(&dir_from, &empty),
        Err(FsError::AlreadyExists(_))
    ));
    assert_eq!(names_in(&empty), Vec::<String>::new());
    // リンクがある名前にも写さず、リンクの先も変えない
    let target = tmp.write("target.txt", "先");
    let link = tmp.at("link.txt");
    symlink(&target, &link).expect("リンクを作れなかった");
    assert!(matches!(
        local().copy_entry(&from, &link),
        Err(FsError::AlreadyExists(_))
    ));
    assert_eq!(read(&target), "先");
    assert_eq!(
        names_in(&tmp.at("")),
        vec!["a.md", "docs", "empty", "link.txt", "target.txt", "x"]
    );
}

#[test]
fn フォルダのコピーは中身ごと複製される() {
    let tmp = TempDir::new("copy-dir");
    let from = tmp.mkdir("src");
    tmp.write("src/a.ts", "a");
    tmp.write("src/nested/deep/b.ts", "b");
    tmp.mkdir("src/empty");
    symlink("a.ts", tmp.at("src/link")).expect("リンクを作れなかった");
    tmp.mkdir("dest");
    let to = tmp.at("dest/src");
    local().copy_entry(&from, &to).expect("写せなかった");
    assert_eq!(read(&tmp.at("dest/src/a.ts")), "a");
    assert_eq!(read(&tmp.at("dest/src/nested/deep/b.ts")), "b");
    assert!(tmp.at("dest/src/empty").is_dir());
    // リンクはリンクのまま写す(先をたどって中身を写さない)
    assert_eq!(
        fs::read_link(tmp.at("dest/src/link")).expect("リンクでない"),
        PathBuf::from("a.ts")
    );
    // 元は変わらない
    assert_eq!(names_in(&from), vec!["a.ts", "empty", "link", "nested"]);
    assert_eq!(names_in(&tmp.at("dest")), vec!["src"]);
}

#[test]
fn リンクのコピーはリンク自体を写す() {
    let tmp = TempDir::new("copy-link");
    let target = tmp.mkdir("target");
    tmp.write("target/a.txt", "a");
    let link = tmp.at("link");
    symlink(&target, &link).expect("リンクを作れなかった");
    let to = tmp.at("copied-link");
    local().copy_entry(&link, &to).expect("写せなかった");
    assert_eq!(fs::read_link(&to).expect("リンクでない"), target);
}

#[test]
fn コピーの元が無ければ見つからないを返す() {
    let tmp = TempDir::new("copy-missing");
    assert!(matches!(
        local().copy_entry(&tmp.at("missing"), &tmp.at("b")),
        Err(FsError::NotFound(_))
    ));
    assert_eq!(names_in(&tmp.at("")), Vec::<String>::new());
}

#[test]
fn フォルダを自分の中へは写さない() {
    let tmp = TempDir::new("copy-into-self");
    let from = tmp.mkdir("src");
    tmp.write("src/a.ts", "a");
    assert!(matches!(
        local().copy_entry(&from, &tmp.at("src/src")),
        Err(FsError::Io(_))
    ));
    assert_eq!(names_in(&from), vec!["a.ts"]);
}

#[test]
fn コピーが途中で失敗したら作りかけを残さない() {
    let tmp = TempDir::new("copy-partial");
    let from = tmp.mkdir("src");
    tmp.write("src/a.txt", "a");
    let secret = tmp.write("src/z-unreadable.txt", "z");
    fs::set_permissions(&secret, fs::Permissions::from_mode(0o000))
        .expect("権限を変えられなかった");
    tmp.mkdir("dest");
    let result = local().copy_entry(&from, &tmp.at("dest/src"));
    fs::set_permissions(&secret, fs::Permissions::from_mode(0o600)).expect("権限を戻せなかった");
    assert!(result.is_err(), "読めない項目があるのに写せた: {result:?}");
    assert_eq!(names_in(&tmp.at("dest")), Vec::<String>::new());
    assert_eq!(names_in(&from), vec!["a.txt", "z-unreadable.txt"]);
}

// ---- ゴミ箱(T40c で実装する) ----

#[test]
fn ゴミ箱はまだ未実装で何も変えない() {
    let tmp = TempDir::new("trash");
    let path = tmp.write("a.txt", "a");
    assert!(matches!(local().trash(&path), Err(FsError::Io(_))));
    assert_eq!(read(&path), "a");
}

// ---- テストの補助 ----

#[test]
fn テスト用のパスが一時領域の外なら補助が拒否する() {
    let base = temp_base();
    assert!(ensure_under_temp(&base.join("arin-ok")).is_ok());
    // 一時領域そのもの、外、`..` で外へ出るもの、相対パスは拒否する(どれもファイルには触れない)
    assert!(ensure_under_temp(&base).is_err());
    assert!(ensure_under_temp(Path::new("/")).is_err());
    assert!(ensure_under_temp(Path::new("/Users")).is_err());
    if let Some(home) = std::env::var_os("HOME") {
        assert!(ensure_under_temp(&PathBuf::from(home).join("arin-test")).is_err());
    }
    assert!(ensure_under_temp(&base.join("..").join("etc")).is_err());
    assert!(ensure_under_temp(Path::new("relative/path")).is_err());
    // 一時領域の中から外を指すリンクの下も拒否する
    let tmp = TempDir::new("guard");
    let outside = tmp.at("outside");
    symlink("/usr", &outside).expect("リンクを作れなかった");
    assert!(ensure_under_temp(&outside.join("bin")).is_err());
}
