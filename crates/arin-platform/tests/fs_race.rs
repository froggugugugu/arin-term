//! 実ファイルでの同時操作の結合テスト(計画書 T44a)。FR-030(上書きしない)、FR-036(実行時の判定。AC-T24)、
//! FR-037(AC-T25 の土台: 監視がまとめて知らせる)、FR-035(ルートの外に出ない)、PRD §8「同時操作」。
//!
//! 手順(`arin_core::fileops::service::FileOperationService`)と実ファイルの口(`arin_platform::fs::LocalFileSystem`)を
//! つないで動かし、操作の途中で「外から」対象を消す・同名を作る・リンクに差し替える。
//!
//! - フォルダは `std::env::temp_dir()` の実パスの下に作り、抜けるときに消す。補助は使うパスが一時領域の下にあることを
//!   確かめてから使う(設計書 §10.2)。本物のホームやプロジェクトのフォルダには触れない
//! - **本物のゴミ箱は使わない**。`FileSystemPort` を包み、`trash` だけを一時フォルダの中の「偽のゴミ箱フォルダ」への
//!   移動に差し替える(本物のゴミ箱は T40c の `tests/trash.rs` だけ)
//! - 「外から」の変更は、包んだ口の各操作の直前(ゴミ箱は直後)に 1 回だけ呼ぶ関数で差し込む(決定的に再現する)。
//!   スレッドを並べて競わせるテストは、何百回か繰り返して不変条件(両方が成功しない・中身が失われない)を確かめる
//! - テストは共通のロックで直列にする(FSEvents はプロセス全体で共有の仕組みのため)。どの待ちにも上限時間を付ける

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Barrier, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use arin_core::fileops::clipboard::FileClipboard;
use arin_core::fileops::service::FileOperationService;
use arin_core::fileops::{ConflictResolution, FileOpOutcome, NewEntryKind, NotAllowedReason};
use arin_core::paths::RelPath;
use arin_core::ports::fs::{DirEntry, EntryKind, FileSystemPort, FsError};
use arin_core::ports::pasteboard::{FilePasteboardPort, PasteboardError};
use arin_core::ports::watch::{WatchBatch, WatchSink, WatcherPort};
use arin_core::tree::changes::summarize_changes;
use arin_core::tree::exclusion::ExclusionRule;
use arin_platform::fs::{LocalFileSystem, STAGING_PREFIX};
use arin_platform::watch::FsEventsWatcher;

/// 1 回の待ちの上限。
const WAIT: Duration = Duration::from_secs(10);
/// 競わせる回数。
const RACE_ROUNDS: usize = 300;
/// コピーの途中に割り込む回数(1 回ごとにフォルダを写すので少なめ)。
const COPY_RACE_ROUNDS: usize = 100;
/// コピーの途中に割り込むときの、元のフォルダの中のファイルの数(写すのに時間がかかるように)。
const COPY_RACE_FILES: usize = 30;

static NEXT: AtomicUsize = AtomicUsize::new(0);
static SERIAL: Mutex<()> = Mutex::new(());

/// テストを直列にする。前のテストが失敗してロックが壊れていても続ける。
fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(PoisonError::into_inner)
}

// ---------------------------------------------------------------------------
// 一時フォルダ
// ---------------------------------------------------------------------------

/// システムの一時領域の実パス。
fn temp_base() -> PathBuf {
    fs::canonicalize(std::env::temp_dir()).expect("一時領域の実パスを取れなかった")
}

/// このテストだけが使う一時フォルダ。中に `root`(プロジェクトのルート)・`outside`(ルートの外)・
/// `trash`(偽のゴミ箱)を作る。抜けるときに自分が作ったフォルダだけを消す。
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let base = temp_base();
        let dir = base.join(format!(
            "arin-platform-race-{name}-{}-{n}",
            std::process::id()
        ));
        assert!(
            dir.starts_with(&base) && dir != base,
            "一時領域の外を使おうとした"
        );
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir(&dir).expect("一時フォルダを作れなかった");
        for sub in ["root", "outside", "trash"] {
            fs::create_dir(dir.join(sub)).expect("一時フォルダの中を作れなかった");
        }
        Self(dir)
    }

    /// プロジェクトのルート(実パス)。
    fn root(&self) -> PathBuf {
        self.0.join("root")
    }

    /// ルートの外のフォルダ。
    fn outside(&self) -> PathBuf {
        self.0.join("outside")
    }

    /// 偽のゴミ箱のフォルダ(ルートの外)。
    fn trash_dir(&self) -> PathBuf {
        self.0.join("trash")
    }

    /// ルートの中のパス。`..`・`.`・絶対パスは panic する。
    fn at(&self, rel: &str) -> PathBuf {
        let rel_path = Path::new(rel);
        assert!(
            rel_path
                .components()
                .all(|c| matches!(c, Component::Normal(_))),
            "ルートの中を指していない: {rel}"
        );
        self.root().join(rel_path)
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
        restore_permissions(&self.0);
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// 権限を落とした項目があっても消せるように戻す(リンクはたどらない)。
fn restore_permissions(dir: &Path) {
    let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(t) if t.is_dir() => restore_permissions(&path),
                Ok(t) if t.is_file() => {
                    let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
                }
                _ => {}
            }
        }
    }
}

/// フォルダの中を、相対パス → 中身(ファイル)か `<dir>`・`<link>` の一覧にする(リンクはたどらない)。
fn snapshot(dir: &Path) -> BTreeSet<(String, String)> {
    fn walk(base: &Path, dir: &Path, out: &mut BTreeSet<(String, String)>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let rel = path
                .strip_prefix(base)
                .expect("中のパス")
                .to_string_lossy()
                .into_owned();
            let file_type = entry.file_type().expect("種類を調べられなかった");
            if file_type.is_symlink() {
                let target = fs::read_link(&path).expect("リンクを読めなかった");
                out.insert((rel, format!("<link {}>", target.display())));
            } else if file_type.is_dir() {
                out.insert((rel, "<dir>".to_owned()));
                walk(base, &path, out);
            } else {
                let contents = fs::read_to_string(&path).unwrap_or_else(|_| "<unreadable>".into());
                out.insert((rel, contents));
            }
        }
    }
    let mut out = BTreeSet::new();
    walk(dir, dir, &mut out);
    out
}

/// フォルダの中に、コピーの作業用フォルダ(`.arin-copy-…`)が残っていないこと(下の階層も含める)。
fn assert_no_staging(dir: &Path) {
    let left: Vec<_> = snapshot(dir)
        .into_iter()
        .filter(|(rel, _)| {
            Path::new(rel)
                .components()
                .any(|c| c.as_os_str().to_string_lossy().starts_with(STAGING_PREFIX))
        })
        .collect();
    assert!(left.is_empty(), "作業用フォルダが残った: {left:?}");
}

fn rel(text: &str) -> RelPath {
    RelPath::parse(text).expect("正しい相対パス")
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{} を読めなかった: {e}", path.display()))
}

fn done(select: &str) -> FileOpOutcome {
    FileOpOutcome::Done {
        select: Some(rel(select)),
    }
}

/// 単一の項目の衝突(数は 1)。
fn conflict(name: &str) -> FileOpOutcome {
    FileOpOutcome::Conflict {
        name: name.to_owned(),
        total: 1,
    }
}

// ---------------------------------------------------------------------------
// 包んだファイルシステム(外からの変更の差し込みと、偽のゴミ箱)
// ---------------------------------------------------------------------------

/// 1 回だけ呼ぶ、外からの変更。
type Hook = Box<dyn FnOnce() + Send>;

/// 実ファイルの口を包み、各操作の直前(ゴミ箱は直後)に外からの変更を 1 回だけ差し込む。
/// `trash` は偽のゴミ箱フォルダへの移動にする(本物のゴミ箱を使わない)。
struct RacyFs {
    inner: LocalFileSystem,
    trash_dir: PathBuf,
    trash_seq: AtomicUsize,
    before_create: Mutex<Option<Hook>>,
    before_move: Mutex<Option<Hook>>,
    before_copy: Mutex<Option<Hook>>,
    after_trash: Mutex<Option<Hook>>,
}

impl RacyFs {
    fn new(tmp: &TempDir) -> Self {
        Self {
            inner: LocalFileSystem::new(),
            trash_dir: tmp.trash_dir(),
            trash_seq: AtomicUsize::new(0),
            before_create: Mutex::new(None),
            before_move: Mutex::new(None),
            before_copy: Mutex::new(None),
            after_trash: Mutex::new(None),
        }
    }

    fn before_create(self, hook: impl FnOnce() + Send + 'static) -> Self {
        *lock(&self.before_create) = Some(Box::new(hook));
        self
    }

    fn before_move(self, hook: impl FnOnce() + Send + 'static) -> Self {
        *lock(&self.before_move) = Some(Box::new(hook));
        self
    }

    fn before_copy(self, hook: impl FnOnce() + Send + 'static) -> Self {
        *lock(&self.before_copy) = Some(Box::new(hook));
        self
    }

    fn after_trash(self, hook: impl FnOnce() + Send + 'static) -> Self {
        *lock(&self.after_trash) = Some(Box::new(hook));
        self
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// 差し込みがあれば 1 回だけ呼ぶ。
fn fire(slot: &Mutex<Option<Hook>>) {
    let hook = lock(slot).take();
    if let Some(hook) = hook {
        hook();
    }
}

impl FileSystemPort for RacyFs {
    fn read_dir(&self, dir: &Path) -> Result<Vec<DirEntry>, FsError> {
        self.inner.read_dir(dir)
    }

    fn entry_kind(&self, path: &Path) -> Result<EntryKind, FsError> {
        self.inner.entry_kind(path)
    }

    fn real_dir(&self, dir: &Path) -> Result<PathBuf, FsError> {
        self.inner.real_dir(dir)
    }

    fn create_file(&self, path: &Path) -> Result<(), FsError> {
        fire(&self.before_create);
        self.inner.create_file(path)
    }

    fn create_dir(&self, path: &Path) -> Result<(), FsError> {
        fire(&self.before_create);
        self.inner.create_dir(path)
    }

    fn move_entry(&self, from: &Path, to: &Path) -> Result<(), FsError> {
        fire(&self.before_move);
        self.inner.move_entry(from, to)
    }

    fn copy_entry(&self, from: &Path, to: &Path) -> Result<(), FsError> {
        fire(&self.before_copy);
        self.inner.copy_entry(from, to)
    }

    fn same_volume(&self, path: &Path, dir: &Path) -> Result<bool, FsError> {
        self.inner.same_volume(path, dir)
    }

    fn trash(&self, path: &Path) -> Result<Option<PathBuf>, FsError> {
        let n = self.trash_seq.fetch_add(1, Ordering::Relaxed);
        let name = path
            .file_name()
            .ok_or_else(|| FsError::Io("名前が無い".to_owned()))?;
        let dest = self
            .trash_dir
            .join(format!("{n}-{}", name.to_string_lossy()));
        // 上書きしない移動で入れる(偽のゴミ箱でも中身を失わない)
        self.inner.move_entry(path, &dest)?;
        fire(&self.after_trash);
        Ok(Some(dest))
    }
}

// ---------------------------------------------------------------------------
// 実行時の判定(FR-036。操作の直前の外からの変更)
// ---------------------------------------------------------------------------

#[test]
fn 名称変更の直前に外から消すと何も変わらず見つからない() {
    let _g = serial();
    let tmp = TempDir::new("rename-gone");
    let target = tmp.write("src/a.ts", "a");
    tmp.write("src/b.ts", "b");
    let fs_port = RacyFs::new(&tmp).before_move(move || {
        fs::remove_file(&target).expect("外から消せなかった");
    });

    let outcome =
        FileOperationService::new(&fs_port).rename(&tmp.root(), &rel("src/a.ts"), "c.ts", None);

    assert_eq!(outcome, FileOpOutcome::NotFound);
    assert_eq!(
        snapshot(&tmp.root()),
        BTreeSet::from([
            ("src".to_owned(), "<dir>".to_owned()),
            ("src/b.ts".to_owned(), "b".to_owned()),
        ]),
        "消えた項目のほかは何も変わらない(新しい名前も作らない)"
    );
}

#[test]
fn メニューを開いたあとに外から消された対象の名称変更は見つからない() {
    let _g = serial();
    let tmp = TempDir::new("rename-gone-before");
    let target = tmp.write("a.ts", "a");
    let fs_port = RacyFs::new(&tmp);
    // メニューを開いた時点ではあり、確定の前に外から消された(AC-T24)
    fs::remove_file(&target).expect("外から消せなかった");

    let outcome =
        FileOperationService::new(&fs_port).rename(&tmp.root(), &rel("a.ts"), "b.ts", None);

    assert_eq!(outcome, FileOpOutcome::NotFound);
    assert!(snapshot(&tmp.root()).is_empty(), "何も作らない");
}

#[test]
fn 名称変更の直前に外で同名ができると衝突になり両方残る() {
    let _g = serial();
    let tmp = TempDir::new("rename-taken");
    tmp.write("a.ts", "mine");
    let taken = tmp.at("b.ts");
    let fs_port = RacyFs::new(&tmp).before_move(move || {
        fs::write(&taken, "external").expect("外から作れなかった");
    });

    let outcome =
        FileOperationService::new(&fs_port).rename(&tmp.root(), &rel("a.ts"), "b.ts", None);

    assert_eq!(outcome, conflict("b.ts"));
    assert_eq!(read(&tmp.at("a.ts")), "mine");
    assert_eq!(read(&tmp.at("b.ts")), "external");
}

#[test]
fn 移動の直前に外から同名を作ると衝突になり両方残る() {
    let _g = serial();
    let tmp = TempDir::new("move-taken");
    tmp.write("src/a.ts", "mine");
    tmp.mkdir("dst");
    let taken = tmp.at("dst/a.ts");
    let fs_port = RacyFs::new(&tmp).before_move(move || {
        fs::write(&taken, "external").expect("外から作れなかった");
    });

    let outcome = FileOperationService::new(&fs_port).move_entry(
        &tmp.root(),
        &rel("src/a.ts"),
        &rel("dst"),
        None,
    );

    assert_eq!(outcome, conflict("a.ts"));
    assert_eq!(read(&tmp.at("src/a.ts")), "mine");
    assert_eq!(read(&tmp.at("dst/a.ts")), "external");
}

#[test]
fn 移動の直前に外から元を消すと何も変わらず見つからない() {
    let _g = serial();
    let tmp = TempDir::new("move-gone");
    let source = tmp.write("src/a.ts", "mine");
    tmp.mkdir("dst");
    let fs_port = RacyFs::new(&tmp).before_move(move || {
        fs::remove_file(&source).expect("外から消せなかった");
    });

    let outcome = FileOperationService::new(&fs_port).move_entry(
        &tmp.root(),
        &rel("src/a.ts"),
        &rel("dst"),
        None,
    );

    assert_eq!(outcome, FileOpOutcome::NotFound);
    assert_eq!(
        snapshot(&tmp.root()),
        BTreeSet::from([
            ("dst".to_owned(), "<dir>".to_owned()),
            ("src".to_owned(), "<dir>".to_owned()),
        ])
    );
}

#[test]
fn コピーの直前に宛先に同名ができても上書きしない() {
    let _g = serial();
    let tmp = TempDir::new("copy-taken");
    tmp.write("src/a.ts", "mine");
    tmp.mkdir("dst");
    let taken = tmp.at("dst/a.ts");
    let fs_port = RacyFs::new(&tmp).before_copy(move || {
        fs::write(&taken, "external").expect("外から作れなかった");
    });

    let outcome = FileOperationService::new(&fs_port).copy_entry(
        &tmp.root(),
        &rel("src/a.ts"),
        &rel("dst"),
        None,
    );

    assert_eq!(outcome, conflict("a.ts"));
    assert_eq!(read(&tmp.at("src/a.ts")), "mine");
    assert_eq!(read(&tmp.at("dst/a.ts")), "external");
    assert_no_staging(&tmp.root());
}

#[test]
fn 置き換えの途中で同名がまたできたら衝突になり既存は偽のゴミ箱に残る() {
    let _g = serial();
    let tmp = TempDir::new("replace-retaken");
    tmp.write("src/a.ts", "mine");
    tmp.write("dst/a.ts", "old");
    let taken = tmp.at("dst/a.ts");
    // 既存をゴミ箱に入れた直後に、外からまた同じ名前で作られた
    let fs_port = RacyFs::new(&tmp).after_trash(move || {
        fs::write(&taken, "newer").expect("外から作れなかった");
    });

    let outcome = FileOperationService::new(&fs_port).move_entry(
        &tmp.root(),
        &rel("src/a.ts"),
        &rel("dst"),
        Some(ConflictResolution::Replace),
    );

    assert_eq!(outcome, conflict("a.ts"));
    assert_eq!(read(&tmp.at("src/a.ts")), "mine", "元は動いていない");
    assert_eq!(
        read(&tmp.at("dst/a.ts")),
        "newer",
        "外の新しい中身を上書きしない"
    );
    let trashed: Vec<String> = snapshot(&tmp.trash_dir())
        .into_iter()
        .map(|(_, c)| c)
        .collect();
    assert_eq!(trashed, ["old"], "既存は偽のゴミ箱にあり、戻せる");
}

// ---------------------------------------------------------------------------
// シンボリックリンクの差し替え(FR-035。ルートの外に書かない)
// ---------------------------------------------------------------------------

/// ルートの中のフォルダ `dir` を退け、同じ名前でルートの外を指すリンクに差し替える(外からの差し替え)。
fn swap_dir_with_link(dir: PathBuf, outside: PathBuf) -> impl FnOnce() + Send + 'static {
    move || {
        let aside = dir.with_file_name(format!(
            "{}-aside",
            dir.file_name().expect("名前").to_string_lossy()
        ));
        fs::rename(&dir, &aside).expect("外から退けられなかった");
        symlink(&outside, &dir).expect("外からリンクを作れなかった");
    }
}

#[test]
fn 作成の直前に親フォルダをルートの外へのリンクに差し替えても外に作らない() {
    let _g = serial();
    let tmp = TempDir::new("swap-create");
    tmp.mkdir("sub");
    let fs_port = RacyFs::new(&tmp).before_create(swap_dir_with_link(tmp.at("sub"), tmp.outside()));

    let outcome = FileOperationService::new(&fs_port).create(
        &tmp.root(),
        &rel("sub"),
        "new.ts",
        NewEntryKind::File,
        None,
    );

    assert_eq!(
        snapshot(&tmp.outside()),
        BTreeSet::new(),
        "ルートの外に作った(応答: {outcome:?})"
    );
}

#[test]
fn 移動の直前に宛先をルートの外へのリンクに差し替えても外へ移さない() {
    let _g = serial();
    let tmp = TempDir::new("swap-move");
    tmp.write("a.ts", "mine");
    tmp.mkdir("dst");
    let fs_port = RacyFs::new(&tmp).before_move(swap_dir_with_link(tmp.at("dst"), tmp.outside()));

    let outcome = FileOperationService::new(&fs_port).move_entry(
        &tmp.root(),
        &rel("a.ts"),
        &rel("dst"),
        None,
    );

    assert_eq!(
        snapshot(&tmp.outside()),
        BTreeSet::new(),
        "ルートの外へ移した(応答: {outcome:?})"
    );
    assert_eq!(read(&tmp.at("a.ts")), "mine", "元はルートの中に残る");
}

#[test]
fn コピーの直前に宛先をルートの外へのリンクに差し替えても外へ写さない() {
    let _g = serial();
    let tmp = TempDir::new("swap-copy");
    tmp.write("a.ts", "mine");
    tmp.mkdir("dst");
    let fs_port = RacyFs::new(&tmp).before_copy(swap_dir_with_link(tmp.at("dst"), tmp.outside()));

    let outcome = FileOperationService::new(&fs_port).copy_entry(
        &tmp.root(),
        &rel("a.ts"),
        &rel("dst"),
        None,
    );

    assert_eq!(
        snapshot(&tmp.outside()),
        BTreeSet::new(),
        "ルートの外へ写した(応答: {outcome:?})"
    );
}

#[test]
fn 移動の直前に元の親をルートの外へのリンクに差し替えても外の項目を動かさない() {
    let _g = serial();
    let tmp = TempDir::new("swap-move-source");
    tmp.write("src/a.ts", "mine");
    tmp.mkdir("dst");
    // 外に同じ名前の項目を置く(リンクをたどると、これがルートの中へ移ってしまう)
    fs::write(tmp.outside().join("a.ts"), "outside").expect("外のファイルを作れなかった");
    let fs_port = RacyFs::new(&tmp).before_move(swap_dir_with_link(tmp.at("src"), tmp.outside()));

    let outcome = FileOperationService::new(&fs_port).move_entry(
        &tmp.root(),
        &rel("src/a.ts"),
        &rel("dst"),
        None,
    );

    assert_eq!(
        snapshot(&tmp.outside()),
        BTreeSet::from([("a.ts".to_owned(), "outside".to_owned())]),
        "ルートの外の項目を動かした(応答: {outcome:?})"
    );
    assert!(
        fs::symlink_metadata(tmp.at("dst/a.ts")).is_err(),
        "宛先に何も置かない(応答: {outcome:?})"
    );
    assert_eq!(
        read(&tmp.at("src-aside/a.ts")),
        "mine",
        "元は退けた先に残る"
    );
}

#[test]
fn 名称変更の直前に対象をリンクに差し替えてもリンクの先を動かさない() {
    let _g = serial();
    let tmp = TempDir::new("swap-target");
    let target = tmp.write("a.ts", "mine");
    let outside_file = tmp.outside().join("secret.txt");
    fs::write(&outside_file, "secret").expect("外のファイルを作れなかった");
    let link_to = outside_file.clone();
    let fs_port = RacyFs::new(&tmp).before_move(move || {
        fs::remove_file(&target).expect("外から消せなかった");
        symlink(&link_to, &target).expect("外からリンクを作れなかった");
    });

    let outcome =
        FileOperationService::new(&fs_port).rename(&tmp.root(), &rel("a.ts"), "b.ts", None);

    // リンク自体の名前が変わる(リンクの先はたどらない)。外のファイルはそのまま
    assert_eq!(outcome, done("b.ts"));
    assert_eq!(read(&outside_file), "secret");
    assert_eq!(
        fs::read_link(tmp.at("b.ts")).expect("名前を変えたのはリンク自体"),
        outside_file
    );
    assert_eq!(
        snapshot(&tmp.outside()),
        BTreeSet::from([("secret.txt".to_owned(), "secret".to_owned())])
    );
}

#[test]
fn ルートの外を指すリンクのフォルダへは作らず移さない() {
    let _g = serial();
    let tmp = TempDir::new("link-dir");
    tmp.write("a.ts", "mine");
    symlink(tmp.outside(), tmp.at("out")).expect("リンクを作れなかった");
    let fs_port = RacyFs::new(&tmp);
    let service = FileOperationService::new(&fs_port);
    let outside_root = NotAllowedReason::OutsideRoot;

    assert_eq!(
        service.create(&tmp.root(), &rel("out"), "x.ts", NewEntryKind::File, None),
        FileOpOutcome::NotAllowed {
            reason: outside_root
        }
    );
    assert_eq!(
        service.move_entry(&tmp.root(), &rel("a.ts"), &rel("out"), None),
        FileOpOutcome::NotAllowed {
            reason: outside_root
        }
    );
    assert_eq!(
        service.copy_entry(&tmp.root(), &rel("a.ts"), &rel("out"), None),
        FileOpOutcome::NotAllowed {
            reason: outside_root
        }
    );
    assert!(snapshot(&tmp.outside()).is_empty());
}

// ---------------------------------------------------------------------------
// 同時に 2 つの操作が同じ名前を作る(スレッドを並べて競わせる。FR-030)
// ---------------------------------------------------------------------------

/// 2 つのスレッドで同時に `op(0)`・`op(1)` を動かし、応答を返す。
fn race2<F>(op: F) -> [FileOpOutcome; 2]
where
    F: Fn(usize) -> FileOpOutcome + Sync,
{
    let barrier = Barrier::new(2);
    thread::scope(|s| {
        let handles = [0, 1].map(|i| {
            let barrier = &barrier;
            let op = &op;
            s.spawn(move || {
                barrier.wait();
                op(i)
            })
        });
        handles.map(|h| h.join().expect("操作のスレッドが panic した"))
    })
}

/// 2 つの応答のうち、ちょうど 1 つが済み、もう 1 つが `name` の衝突であること。済んだ方の番号を返す。
fn exactly_one_done(
    outcomes: &[FileOpOutcome; 2],
    select: &str,
    name: &str,
    round: usize,
) -> usize {
    match (&outcomes[0], &outcomes[1]) {
        (a, b) if *a == done(select) && *b == conflict(name) => 0,
        (a, b) if *a == conflict(name) && *b == done(select) => 1,
        other => panic!("{round} 回目: 片方だけが済むはずが {other:?}"),
    }
}

#[test]
fn 同時に同じ名前のファイルを作ると片方だけが作り片方は衝突になる() {
    let _g = serial();
    let tmp = TempDir::new("race-create");
    let fs_port = LocalFileSystem::new();
    let service = FileOperationService::new(&fs_port);
    for round in 0..RACE_ROUNDS {
        let dir = format!("r{round}");
        tmp.mkdir(&dir);
        let outcomes =
            race2(|_| service.create(&tmp.root(), &rel(&dir), "same.ts", NewEntryKind::File, None));
        exactly_one_done(&outcomes, &format!("{dir}/same.ts"), "same.ts", round);
        assert_eq!(
            snapshot(&tmp.at(&dir)),
            BTreeSet::from([("same.ts".to_owned(), String::new())])
        );
    }
}

#[test]
fn 同時に別の項目を同じ名前へ変えると片方だけが変わり両方の中身が残る() {
    let _g = serial();
    let tmp = TempDir::new("race-rename");
    let fs_port = LocalFileSystem::new();
    let service = FileOperationService::new(&fs_port);
    for round in 0..RACE_ROUNDS {
        let dir = format!("r{round}");
        tmp.write(&format!("{dir}/x0.ts"), "zero");
        tmp.write(&format!("{dir}/x1.ts"), "one");
        let outcomes = race2(|i| {
            service.rename(
                &tmp.root(),
                &rel(&format!("{dir}/x{i}.ts")),
                "same.ts",
                None,
            )
        });
        let winner = exactly_one_done(&outcomes, &format!("{dir}/same.ts"), "same.ts", round);
        // 勝った方の中身が新しい名前にあり、負けた方は元の名前のまま残る
        let (won_contents, lost_name, lost_contents) = if winner == 0 {
            ("zero", "x1.ts", "one")
        } else {
            ("one", "x0.ts", "zero")
        };
        assert_eq!(
            snapshot(&tmp.at(&dir)),
            BTreeSet::from([
                ("same.ts".to_owned(), won_contents.to_owned()),
                (lost_name.to_owned(), lost_contents.to_owned()),
            ]),
            "{round} 回目"
        );
    }
}

#[test]
fn 同時に同じ名前の項目を同じフォルダへ写すと片方だけが写り上書きしない() {
    let _g = serial();
    let tmp = TempDir::new("race-copy");
    let fs_port = LocalFileSystem::new();
    let service = FileOperationService::new(&fs_port);
    for round in 0..RACE_ROUNDS {
        let dir = format!("r{round}");
        tmp.write(&format!("{dir}/p0/same.ts"), "zero");
        tmp.write(&format!("{dir}/p1/same.ts"), "one");
        tmp.mkdir(&format!("{dir}/dst"));
        let outcomes = race2(|i| {
            service.copy_entry(
                &tmp.root(),
                &rel(&format!("{dir}/p{i}/same.ts")),
                &rel(&format!("{dir}/dst")),
                None,
            )
        });
        let winner = exactly_one_done(&outcomes, &format!("{dir}/dst/same.ts"), "same.ts", round);
        let expected = if winner == 0 { "zero" } else { "one" };
        assert_eq!(
            snapshot(&tmp.at(&format!("{dir}/dst"))),
            BTreeSet::from([("same.ts".to_owned(), expected.to_owned())]),
            "{round} 回目: 作業用フォルダが残らず、写ったのは勝った方だけ"
        );
        assert_eq!(read(&tmp.at(&format!("{dir}/p0/same.ts"))), "zero");
        assert_eq!(read(&tmp.at(&format!("{dir}/p1/same.ts"))), "one");
    }
}

#[test]
fn 同時に両方残すで作ると別々の名前になるか衝突になり上書きしない() {
    let _g = serial();
    let tmp = TempDir::new("race-keep-both");
    let fs_port = LocalFileSystem::new();
    let service = FileOperationService::new(&fs_port);
    for round in 0..RACE_ROUNDS {
        let dir = format!("r{round}");
        tmp.write(&format!("{dir}/same.ts"), "existing");
        tmp.write(&format!("{dir}/src/x0/same.ts"), "zero");
        tmp.write(&format!("{dir}/src/x1/same.ts"), "one");
        let outcomes = race2(|i| {
            service.move_entry(
                &tmp.root(),
                &rel(&format!("{dir}/src/x{i}/same.ts")),
                &rel(&dir),
                Some(ConflictResolution::KeepBoth),
            )
        });
        let mut placed = Vec::new();
        for (i, outcome) in outcomes.iter().enumerate() {
            match outcome {
                FileOpOutcome::Done {
                    select: Some(select),
                } => placed.push((i, select.clone())),
                // 番号付きの名前も同時に取られた(もう一度確かめてもらう)
                FileOpOutcome::Conflict { name, .. } if name == "same.ts" => {}
                other => panic!("{round} 回目: 思わぬ応答 {other:?}"),
            }
        }
        if placed.len() == 2 {
            assert_ne!(
                placed[0].1, placed[1].1,
                "{round} 回目: 同じ名前に 2 つ置いた"
            );
        }
        // どの中身も失われていない
        let contents: Vec<String> = snapshot(&tmp.at(&dir))
            .into_iter()
            .filter(|(_, c)| c != "<dir>")
            .map(|(_, c)| c)
            .collect();
        let mut contents = contents;
        contents.sort();
        assert_eq!(contents, ["existing", "one", "zero"], "{round} 回目");
        assert_eq!(read(&tmp.at(&format!("{dir}/same.ts"))), "existing");
        for (i, select) in placed {
            let expected = if i == 0 { "zero" } else { "one" };
            assert_eq!(read(&tmp.at(select.as_str())), expected, "{round} 回目");
        }
    }
}

#[test]
fn コピーの途中で宛先に同名ができても上書きせず作業用フォルダも残らない() {
    let _g = serial();
    let tmp = TempDir::new("race-copy-midway");
    let fs_port = LocalFileSystem::new();
    let service = FileOperationService::new(&fs_port);
    let mut window_hits = 0;
    let mut copy_won = 0;
    for round in 0..COPY_RACE_ROUNDS {
        let dir = format!("r{round}");
        for f in 0..COPY_RACE_FILES {
            tmp.write(&format!("{dir}/data/f{f}.txt"), "copied");
        }
        let dst = tmp.mkdir(&format!("{dir}/dst"));
        let target = dst.join("data");
        let barrier = Barrier::new(2);
        let (outcome, external) = thread::scope(|s| {
            let copier = s.spawn(|| {
                barrier.wait();
                service.copy_entry(
                    &tmp.root(),
                    &rel(&format!("{dir}/data")),
                    &rel(&format!("{dir}/dst")),
                    None,
                )
            });
            let intruder = s.spawn(|| {
                barrier.wait();
                // 作業用フォルダが現れた(=先の確認を通り、写している途中)ときに、宛先の名前を外から作る
                let deadline = Instant::now() + WAIT;
                let mut saw_staging = false;
                while Instant::now() < deadline {
                    let staging = fs::read_dir(&dst).map(|entries| {
                        entries
                            .flatten()
                            .any(|e| e.file_name().to_string_lossy().starts_with(STAGING_PREFIX))
                    });
                    if matches!(staging, Ok(true)) {
                        saw_staging = true;
                        break;
                    }
                    if fs::symlink_metadata(&target).is_ok() {
                        break;
                    }
                    thread::yield_now();
                }
                let made = match fs::create_dir(&target) {
                    Ok(()) => {
                        fs::write(target.join("external.txt"), "external")
                            .expect("外の中身を書けなかった");
                        true
                    }
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => false,
                    Err(e) => panic!("外から作れなかった: {e}"),
                };
                (saw_staging, made)
            });
            (
                copier.join().expect("コピーのスレッドが panic した"),
                intruder.join().expect("割り込みのスレッドが panic した"),
            )
        });
        let (saw_staging, made) = external;
        if saw_staging {
            window_hits += 1;
        }
        let inside = snapshot(&target);
        match (&outcome, made) {
            (FileOpOutcome::Done { .. }, false) => {
                copy_won += 1;
                assert_eq!(
                    inside.len(),
                    COPY_RACE_FILES,
                    "{round} 回目: 写した中身が欠けた"
                );
                assert!(inside.iter().all(|(_, c)| c == "copied"), "{round} 回目");
            }
            (FileOpOutcome::Conflict { name, .. }, true) if name == "data" => {
                assert_eq!(
                    inside,
                    BTreeSet::from([("external.txt".to_owned(), "external".to_owned())]),
                    "{round} 回目: 外が作ったフォルダに写した物が混ざった"
                );
            }
            other => panic!("{round} 回目: 両方が成功した・どちらも成功しなかった: {other:?}"),
        }
        assert_no_staging(&dst);
    }
    eprintln!(
        "コピーの途中への割り込み: {COPY_RACE_ROUNDS} 回のうち作業用フォルダが見えた {window_hits} 回、コピーが勝った {copy_won} 回"
    );
}

// ---------------------------------------------------------------------------
// 複数の項目のペーストの事前の検査(R2。実ファイルでの名前の正規化)
// ---------------------------------------------------------------------------

/// メモリの中のファイルのペーストボード(Finder でコピーしたことにする。本物のペーストボードに触れない)。
#[derive(Default)]
struct MemoryPasteboard(Mutex<(Vec<PathBuf>, i64)>);

impl FilePasteboardPort for MemoryPasteboard {
    fn write_file_urls(&self, paths: &[&Path]) -> Result<i64, PasteboardError> {
        let mut inner = lock(&self.0);
        inner.0 = paths.iter().map(|p| p.to_path_buf()).collect();
        inner.1 += 1;
        Ok(inner.1)
    }

    fn read_file_urls(&self) -> Result<Vec<PathBuf>, PasteboardError> {
        Ok(lock(&self.0).0.clone())
    }

    fn change_count(&self) -> i64 {
        lock(&self.0).1
    }
}

#[test]
fn 正規化だけが違う名前も事前の検査で衝突として数え何も置かない() {
    let _g = serial();
    let tmp = TempDir::new("paste-nfd");
    let nfc = "\u{e9}.txt"; // é(合成済み)
    let nfd = "e\u{301}.txt"; // e + 結合アクセント
    fs::write(tmp.outside().join("b.txt"), "b").expect("作れなかった");
    fs::write(tmp.outside().join(nfc), "新しい").expect("作れなかった");
    tmp.write(&format!("dst/{nfd}"), "既存");
    if fs::symlink_metadata(tmp.at("dst").join(nfc)).is_err() {
        eprintln!("このボリュームは正規化の違いを区別するので確かめない");
        return;
    }
    let before = snapshot(&tmp.at("dst"));
    let fs_port = RacyFs::new(&tmp);
    let board = MemoryPasteboard::default();
    board
        .write_file_urls(&[&tmp.outside().join("b.txt"), &tmp.outside().join(nfc)])
        .expect("書けた");
    let mut clipboard = FileClipboard::new();

    let outcome = FileOperationService::new(&fs_port).paste(
        &board,
        &mut clipboard,
        &|_| None,
        &tmp.root(),
        &rel("dst"),
        None,
    );

    assert_eq!(outcome, conflict(nfc));
    assert_eq!(snapshot(&tmp.at("dst")), before, "1 件目だけ置いた");
}

// ---------------------------------------------------------------------------
// 作業用フォルダの片付け
// ---------------------------------------------------------------------------

#[test]
fn コピーが失敗しても作業用フォルダが残らない() {
    let _g = serial();
    let tmp = TempDir::new("copy-fail");
    tmp.write("data/ok.txt", "ok");
    let unreadable = tmp.write("data/secret.txt", "secret");
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000))
        .expect("権限を落とせなかった");
    tmp.mkdir("dst");
    let fs_port = RacyFs::new(&tmp);

    let outcome = FileOperationService::new(&fs_port).copy_entry(
        &tmp.root(),
        &rel("data"),
        &rel("dst"),
        None,
    );

    assert!(
        matches!(outcome, FileOpOutcome::Failed { .. }),
        "読めないファイルを含むフォルダは写せない: {outcome:?}"
    );
    assert_no_staging(&tmp.at("dst"));
    assert!(
        fs::symlink_metadata(tmp.at("dst/data")).is_err(),
        "作りかけを宛先の名前に置かない"
    );
}

// ---------------------------------------------------------------------------
// 監視(FR-037 の土台。AC-T25 の「100 ファイルの追加」)
// ---------------------------------------------------------------------------

#[test]
fn 外から_100_ファイルを足すと監視がまとめて知らせる() {
    let _g = serial();
    let tmp = TempDir::new("watch-100");
    let src = tmp.mkdir("src");
    tmp.write("src/a.ts", "a");
    let (tx, rx) = mpsc::channel::<WatchBatch>();
    let sink: WatchSink = Box::new(move |batch| {
        let _ = tx.send(batch);
    });
    let _watch = FsEventsWatcher::new()
        .watch(&tmp.root(), sink)
        .expect("監視を張れなかった");

    // 監視が動き出したことを目印で確かめる
    let marker = tmp.root().join(".warm");
    fs::write(&marker, "m").expect("目印を作れなかった");
    let deadline = Instant::now() + WAIT;
    'warm: loop {
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(batch) if batch.paths.contains(&marker) => break 'warm,
            Ok(_) => {}
            Err(RecvTimeoutError::Timeout) => panic!("{WAIT:?} 待っても目印が届かなかった"),
            Err(RecvTimeoutError::Disconnected) => panic!("受け口が手放された"),
        }
    }
    // 目印の束の後に届いたものを捨てる(束ねる時間の数倍待つ)
    thread::sleep(Duration::from_millis(600));
    let _ = rx.try_iter().count();

    let added: Vec<PathBuf> = (0..100)
        .map(|i| src.join(format!("new-{i:03}.ts")))
        .collect();
    for path in &added {
        fs::write(path, "n").expect("外からファイルを作れなかった");
    }

    let rule = ExclusionRule::new(false);
    let mut seen = BTreeSet::new();
    let mut batches = 0;
    let mut dirs = BTreeSet::new();
    let deadline = Instant::now() + WAIT;
    while !added.iter().all(|p| seen.contains(p)) {
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(batch) => {
                batches += 1;
                let summary = summarize_changes(&tmp.root(), &batch, &rule);
                assert!(
                    !summary.full_reload,
                    "100 件は一覧で届く(上限の 1,000 件より少ない)"
                );
                dirs.extend(summary.dirs.into_iter().map(|d| d.as_str().to_owned()));
                seen.extend(batch.paths);
            }
            Err(RecvTimeoutError::Timeout) => panic!(
                "{WAIT:?} 待っても 100 件が揃わなかった({} 件届いた)",
                added.iter().filter(|p| seen.contains(*p)).count()
            ),
            Err(RecvTimeoutError::Disconnected) => panic!("受け口が手放された"),
        }
    }
    assert_eq!(
        dirs,
        BTreeSet::from(["src".to_owned()]),
        "変わったフォルダは src だけ"
    );
    assert!(
        batches <= 5,
        "1 件ずつではなくまとめて届く(届いた束の数: {batches})"
    );
}
