//! ファイル監視の結合テスト(`arin_platform::watch::FsEventsWatcher`)。
//! FR-014(AC-T02 の土台)、PRD C7、設計書 §1.3 #7、§5.4「ファイル監視」。T40b。
//!
//! - フォルダは `std::env::temp_dir()` の下に、プロセス ID と連番を含む名前で作り、抜けるときに消す
//! - macOS の一時フォルダは `/var` → `/private/var` のシンボリックリンク。監視は実パス(`/private/var/...`)で届く
//! - FSEvents はプロセス全体で共有の仕組みなので、テストは共通のロックで直列にする。どの待ちにも上限時間を付ける
//! - 監視を張った直後の取りこぼしを避けるため、先に「目印」のファイルを作って届くのを待ってから本題の変更をする

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use arin_core::ports::watch::{ActiveWatch, WatchBatch, WatchSink, WatcherPort};
use arin_platform::watch::FsEventsWatcher;

/// 届くのを待つ上限。AC は 2 秒以内。待ちは長めに取り、2 秒以内かは経過時間で別に確かめる。
const WAIT: Duration = Duration::from_secs(10);
/// 「届かない」ことを確かめるときに待つ時間(待ち時間 0.2 秒の数倍)。
const QUIET: Duration = Duration::from_millis(1500);
/// AC-T02 の上限。
const AC_LIMIT: Duration = Duration::from_secs(2);

static NEXT: AtomicUsize = AtomicUsize::new(0);
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(PoisonError::into_inner)
}

/// このテストだけが使う一時フォルダ。抜けるときに自分が作ったフォルダだけを消す。
struct TempDir {
    /// `temp_dir()` のままの形(`/var/folders/...`)。
    given: PathBuf,
    /// 実パス(`/private/var/folders/...`)。
    real: PathBuf,
}

impl TempDir {
    fn new(name: &str) -> Self {
        let base = std::env::temp_dir();
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = base.join(format!(
            "arin-platform-watch-{name}-{}-{n}",
            std::process::id()
        ));
        assert!(dir.starts_with(&base), "一時領域の外を使おうとした");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir(&dir).expect("一時フォルダを作れなかった");
        let real = fs::canonicalize(&dir).expect("一時フォルダの実パスを取れなかった");
        Self { given: dir, real }
    }

    fn path(&self) -> &Path {
        &self.real
    }

    fn mkdir(&self, rel: &str) -> PathBuf {
        let p = self.real.join(rel);
        fs::create_dir_all(&p).expect("フォルダを作れなかった");
        p
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.real);
    }
}

/// 受け口に届いた束をテストのスレッドへ流す。
fn channel_sink() -> (WatchSink, Receiver<WatchBatch>) {
    let (tx, rx) = mpsc::channel();
    let sink: WatchSink = Box::new(move |batch| {
        let _ = tx.send(batch);
    });
    (sink, rx)
}

fn watch(root: &Path, sink: WatchSink) -> Box<dyn ActiveWatch> {
    FsEventsWatcher::new()
        .watch(root, sink)
        .expect("監視を張れなかった")
}

/// `want` のパスがすべて届くまで待ち、届いたパスの全部と、すべて揃うまでの時間を返す。
fn wait_for_paths(rx: &Receiver<WatchBatch>, want: &[&Path]) -> (BTreeSet<PathBuf>, Duration) {
    let start = Instant::now();
    let mut seen = BTreeSet::new();
    loop {
        if want.iter().all(|p| seen.contains(*p)) {
            return (seen, start.elapsed());
        }
        let left = WAIT.checked_sub(start.elapsed()).unwrap_or_default();
        match rx.recv_timeout(left) {
            Ok(batch) => seen.extend(batch.paths),
            Err(RecvTimeoutError::Timeout) => {
                panic!("{WAIT:?} 待っても {want:?} が揃わなかった。届いたもの: {seen:?}")
            }
            Err(RecvTimeoutError::Disconnected) => {
                panic!("受け口が手放された。届いたもの: {seen:?}")
            }
        }
    }
}

/// 監視が動き出したことを、目印のファイルを作って確かめる。
fn warm_up(rx: &Receiver<WatchBatch>, root: &Path, name: &str) {
    let marker = root.join(name);
    fs::write(&marker, b"m").expect("目印を作れなかった");
    wait_for_paths(rx, &[&marker]);
}

/// 届いた束をすべて取り出す(待たない)。
fn drain(rx: &Receiver<WatchBatch>) -> Vec<WatchBatch> {
    rx.try_iter().collect()
}

#[test]
fn ファイルを作ると_2_秒以内にそのパスが届く() {
    let _g = serial();
    let tmp = TempDir::new("create");
    let (sink, rx) = channel_sink();
    let _watch = watch(tmp.path(), sink);
    warm_up(&rx, tmp.path(), ".warm");

    let file = tmp.path().join("new.txt");
    let start = Instant::now();
    fs::write(&file, b"hello").expect("ファイルを作れなかった");
    wait_for_paths(&rx, &[&file]);
    let elapsed = start.elapsed();
    assert!(
        elapsed < AC_LIMIT,
        "届くまでに {elapsed:?} かかった(上限 {AC_LIMIT:?})"
    );
}

#[test]
fn 下のフォルダの変更も変わったファイルのパスで届く() {
    let _g = serial();
    let tmp = TempDir::new("nested");
    let sub = tmp.mkdir("src/deep");
    let (sink, rx) = channel_sink();
    let _watch = watch(tmp.path(), sink);
    warm_up(&rx, tmp.path(), ".warm");

    let file = sub.join("a.ts");
    fs::write(&file, b"x").expect("ファイルを作れなかった");
    // フォルダ単位ではなく、変わった項目そのもののパスが届く(T38c は親を変わったフォルダとして扱う)
    wait_for_paths(&rx, &[&file]);
}

#[test]
fn 削除と名称変更も届く() {
    let _g = serial();
    let tmp = TempDir::new("remove-rename");
    let gone = tmp.path().join("gone.txt");
    let before = tmp.path().join("before.txt");
    let after = tmp.path().join("after.txt");
    let dir = tmp.mkdir("olddir");
    let dir_after = tmp.path().join("newdir");
    fs::write(&gone, b"g").expect("ファイルを作れなかった");
    fs::write(&before, b"b").expect("ファイルを作れなかった");
    let (sink, rx) = channel_sink();
    let _watch = watch(tmp.path(), sink);
    warm_up(&rx, tmp.path(), ".warm");

    fs::remove_file(&gone).expect("削除できなかった");
    wait_for_paths(&rx, &[&gone]);

    fs::rename(&before, &after).expect("名称変更できなかった");
    // 名称変更は元の名前と新しい名前の両方が届く
    wait_for_paths(&rx, &[&before, &after]);

    fs::rename(&dir, &dir_after).expect("フォルダの名称変更ができなかった");
    wait_for_paths(&rx, &[&dir, &dir_after]);
}

#[test]
fn 監視をやめると届かなくなる() {
    let _g = serial();
    let tmp = TempDir::new("stop");
    let (sink, rx) = channel_sink();
    let watch = watch(tmp.path(), sink);
    warm_up(&rx, tmp.path(), ".warm");

    drop(watch);
    let _ = drain(&rx);
    fs::write(tmp.path().join("after-stop.txt"), b"x").expect("ファイルを作れなかった");
    // 監視を捨てると受け口(と束ねるスレッド)が手放され、送り口が閉じる
    match rx.recv_timeout(QUIET) {
        Err(RecvTimeoutError::Disconnected) => {}
        Ok(batch) => panic!("監視をやめたあとに届いた: {batch:?}"),
        Err(RecvTimeoutError::Timeout) => panic!("監視をやめても受け口が手放されなかった"),
    }
}

#[test]
fn 監視を捨てると受け口が手放されスレッドが残らない() {
    let _g = serial();
    let tmp = TempDir::new("release");
    let held = Arc::new(());
    let (tx, rx) = mpsc::channel();
    let in_sink = Arc::clone(&held);
    let sink: WatchSink = Box::new(move |batch| {
        let _ = &in_sink;
        let _ = tx.send(batch);
    });
    let watch = watch(tmp.path(), sink);
    warm_up(&rx, tmp.path(), ".warm");
    assert_eq!(Arc::strong_count(&held), 2);

    // 捨てる処理が上限時間の中で終わること(監視のスレッドを待って止まらない)
    let (done_tx, done_rx) = mpsc::channel();
    thread::spawn(move || {
        drop(watch);
        let _ = done_tx.send(());
    });
    done_rx
        .recv_timeout(WAIT)
        .expect("監視を捨てる処理が終わらなかった");
    // 受け口は束ねるスレッドだけが持つ。数が戻れば、そのスレッドは終わっている(join 済み)
    assert_eq!(Arc::strong_count(&held), 1, "受け口が手放されていない");
}

#[test]
fn 監視しているフォルダの外の変更は届かない() {
    let _g = serial();
    let tmp = TempDir::new("outside");
    let root = tmp.mkdir("root");
    let outside = tmp.mkdir("outside");
    // 名前の頭が同じ兄弟のフォルダ(`root` と `root-sibling`)も外として扱う
    let sibling = tmp.mkdir("root-sibling");
    let (sink, rx) = channel_sink();
    let _watch = watch(&root, sink);
    warm_up(&rx, &root, ".warm");

    fs::write(outside.join("x.txt"), b"x").expect("ファイルを作れなかった");
    fs::write(sibling.join("y.txt"), b"y").expect("ファイルを作れなかった");
    fs::write(tmp.path().join("z.txt"), b"z").expect("ファイルを作れなかった");
    // 外の変更のあとに中で目印を作り、それが届くまでに外のパスが混じらないことを確かめる
    let marker = root.join(".after");
    fs::write(&marker, b"m").expect("目印を作れなかった");
    let (seen, _) = wait_for_paths(&rx, &[&marker]);
    thread::sleep(Duration::from_millis(500));
    let mut all = seen;
    for batch in drain(&rx) {
        all.extend(batch.paths);
    }
    let outside_paths: Vec<_> = all.iter().filter(|p| !p.starts_with(&root)).collect();
    assert!(
        outside_paths.is_empty(),
        "外のパスが届いた: {outside_paths:?}"
    );
}

#[test]
fn 一時フォルダのリンクを経由して張っても実パスで届く() {
    let _g = serial();
    let tmp = TempDir::new("realpath");
    // `temp_dir()` のままの形(`/var/...`)で張る。macOS では実パスと違う形になる
    let (sink, rx) = channel_sink();
    let _watch = FsEventsWatcher::new()
        .watch(&tmp.given, sink)
        .expect("監視を張れなかった");
    let marker = tmp.path().join(".warm");
    fs::write(&marker, b"m").expect("目印を作れなかった");
    let (seen, _) = wait_for_paths(&rx, &[&marker]);
    assert!(
        seen.iter().all(|p| p.starts_with(tmp.path())),
        "実パスの形でないものが届いた: {seen:?}(実パス {:?})",
        tmp.path()
    );
}

#[test]
fn 監視中のルートを改名しても落ちずにやめられる() {
    let _g = serial();
    let tmp = TempDir::new("rename-root");
    let root = tmp.mkdir("root");
    let moved = tmp.path().join("moved");
    let (sink, rx) = channel_sink();
    let watch = watch(&root, sink);
    warm_up(&rx, &root, ".warm");

    fs::rename(&root, &moved).expect("ルートを改名できなかった");
    // 改名したあとの中の変更(届いても届かなくてもよい。落ちないこと)
    fs::write(moved.join("inside.txt"), b"x").expect("ファイルを作れなかった");
    thread::sleep(Duration::from_millis(500));
    let _ = drain(&rx);

    let (done_tx, done_rx) = mpsc::channel();
    thread::spawn(move || {
        drop(watch);
        let _ = done_tx.send(());
    });
    done_rx
        .recv_timeout(WAIT)
        .expect("改名したあとに監視をやめられなかった");
    match rx.recv_timeout(QUIET) {
        Err(RecvTimeoutError::Disconnected) => {}
        Ok(batch) => panic!("監視をやめたあとに届いた: {batch:?}"),
        Err(RecvTimeoutError::Timeout) => panic!("監視をやめても受け口が手放されなかった"),
    }
}

#[test]
fn 無いフォルダやファイルには監視を張れない() {
    let _g = serial();
    let tmp = TempDir::new("invalid");
    let file = tmp.path().join("file.txt");
    fs::write(&file, b"x").expect("ファイルを作れなかった");
    let (sink, _rx) = channel_sink();
    assert!(
        FsEventsWatcher::new()
            .watch(&tmp.path().join("missing"), sink)
            .is_err()
    );
    let (sink, _rx) = channel_sink();
    assert!(FsEventsWatcher::new().watch(&file, sink).is_err());
}
