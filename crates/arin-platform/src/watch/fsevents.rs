//! ツリーのファイル監視(FSEvents。設計書 §1.3 #7、§5.4「ファイル監視」、FR-014、PRD C7、計画書 T40b)。
//!
//! - FSEvents は `notify` クレート(ADR-007)経由で使う。`notify` はファイル単位のイベント
//!   (`kFSEventStreamCreateFlagFileEvents`)で張るので、届くのは変わった項目そのもののパス。
//!   パスは FSEvents が返す実パス(シンボリックリンクを解いた形。macOS の一時フォルダなら `/private/var/...`)
//! - `notify` の FSEvents は待ち時間 0 で届けるので、束ねるスレッドで [`LATENCY`] の間まとめてから受け口へ渡す
//! - FSEvents のコールバック(`notify` のスレッド)は、チャンネルへ送るだけで戻る。受け口を呼ぶのは束ねるスレッド
//! - 返した [`ActiveWatch`] を捨てると、`notify` のスレッドを止めて join し、束ねるスレッドも join する。
//!   捨てる処理が戻ったあと、受け口は呼ばれない
//! - 除外の判定と「変わったフォルダ」へのまとめは、呼び出し側(T42b)が `arin-core` の `tree`(T38c)で当てる

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use arin_core::ports::watch::{ActiveWatch, WatchBatch, WatchError, WatchSink, WatcherPort};
use notify::{Config, Event, FsEventWatcher, RecursiveMode, Watcher};

/// 最初の変更が届いてから受け口へ渡すまでにまとめる時間(設計書 §7.1「外部でのファイル変更」の 0.2 秒程度)。
pub const LATENCY: Duration = Duration::from_millis(200);

/// FSEvents でフォルダを監視する。状態を持たない。
#[derive(Debug, Clone, Copy, Default)]
pub struct FsEventsWatcher;

impl FsEventsWatcher {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl WatcherPort for FsEventsWatcher {
    fn watch(&self, root: &Path, sink: WatchSink) -> Result<Box<dyn ActiveWatch>, WatchError> {
        // FSEvents は実パスで知らせるので、張る前に実パスにそろえる(外のパスを捨てる判定にも使う)
        let real =
            fs::canonicalize(root).map_err(|e| WatchError(format!("{}: {e}", root.display())))?;
        let metadata =
            fs::metadata(&real).map_err(|e| WatchError(format!("{}: {e}", real.display())))?;
        if !metadata.is_dir() {
            return Err(WatchError(format!("{}: フォルダでない", real.display())));
        }

        let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
        // FSEvents のコールバックから呼ばれる。チャンネルへ送るだけで、待たない(送れなければ捨てる)
        let mut watcher = FsEventWatcher::new(
            move |event| {
                let _ = tx.send(event);
            },
            Config::default(),
        )
        .map_err(|e| WatchError(format!("{}: {e}", real.display())))?;
        watcher
            .watch(&real, RecursiveMode::Recursive)
            .map_err(|e| WatchError(format!("{}: {e}", real.display())))?;

        let stopped = Arc::new(AtomicBool::new(false));
        let batcher = {
            let stopped = Arc::clone(&stopped);
            let root = real.clone();
            thread::Builder::new()
                .name("arin-watch".to_string())
                .spawn(move || run_batcher(&rx, &root, &stopped, &sink))
                .map_err(|e| WatchError(format!("監視のスレッドを作れなかった: {e}")))?
        };

        Ok(Box::new(FsEventsWatch {
            watcher: Some(watcher),
            stopped,
            batcher: Some(batcher),
        }))
    }
}

/// 張った監視。捨てると止まる。
struct FsEventsWatch {
    watcher: Option<FsEventWatcher>,
    stopped: Arc<AtomicBool>,
    batcher: Option<JoinHandle<()>>,
}

impl ActiveWatch for FsEventsWatch {}

impl Drop for FsEventsWatch {
    fn drop(&mut self) {
        // 先に印を立て、束ねるスレッドがこのあと受け口を呼ばないようにする
        self.stopped.store(true, Ordering::SeqCst);
        // notify の Drop は FSEvents の流れを止め、そのスレッドを join する。
        // 流れを解放すると送り口も手放されるので、束ねるスレッドの受信が切れて終わる
        drop(self.watcher.take());
        if let Some(handle) = self.batcher.take() {
            // 受け口の中から監視を捨てたとき(束ねるスレッド自身)は、自分を待てないので待たない。
            // その場合も送り口は閉じているので、受け口から戻ればスレッドは終わる
            if handle.thread().id() != thread::current().id() {
                let _ = handle.join();
            }
        }
    }
}

/// 届いた変更を [`LATENCY`] の間まとめて受け口へ渡す。送り口が閉じたら(監視を捨てたら)終わる。
fn run_batcher(
    rx: &Receiver<notify::Result<Event>>,
    root: &Path,
    stopped: &AtomicBool,
    sink: &WatchSink,
) {
    // 最初の 1 件が来るまで待つ。送り口が閉じたら終わる
    while let Ok(first) = rx.recv() {
        let mut pending = Pending::default();
        pending.add(first, root);
        let deadline = Instant::now() + LATENCY;
        let mut closed = false;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            match rx.recv_timeout(left) {
                Ok(event) => pending.add(event, root),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => {
                    closed = true;
                    break;
                }
            }
        }
        // 監視を捨てたあとは、まとめかけの分も渡さない
        if closed || stopped.load(Ordering::SeqCst) {
            return;
        }
        if let Some(batch) = pending.into_batch() {
            sink(batch);
        }
    }
}

/// まとめかけの変更。パスは重複を除いて名前の順に持つ(渡す順を決定的にするため)。
#[derive(Default)]
struct Pending {
    paths: BTreeSet<PathBuf>,
    rescan: bool,
}

impl Pending {
    fn add(&mut self, event: notify::Result<Event>, root: &Path) {
        match event {
            Ok(event) => {
                if event.need_rescan() {
                    self.rescan = true;
                }
                // notify もルートの下だけを渡すが、念のためルートの外は捨てる
                self.paths
                    .extend(event.paths.into_iter().filter(|p| p.starts_with(root)));
            }
            // 個々の変更が分からないので、全部読み直してもらう
            Err(_) => self.rescan = true,
        }
    }

    fn into_batch(self) -> Option<WatchBatch> {
        if self.paths.is_empty() && !self.rescan {
            return None;
        }
        Some(WatchBatch {
            paths: self.paths.into_iter().collect(),
            rescan: self.rescan,
        })
    }
}
