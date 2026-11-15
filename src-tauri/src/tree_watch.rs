//! ツリーの監視の張り替えと `tree-changed` の送り出し(計画書 T42b、FR-014、FR-002・FR-007 のツリーの部分、
//! 設計書 §1.3 #7、§7.1「プロジェクトの切替」「ルートフォルダへの追従」「外部でのファイル変更」「スレッドの分担」)。
//!
//! - 監視は選択中のプロジェクトのルートに 1 本だけ張る(NFR-003)。ほかのプロジェクトは選んだときに画面が読み直す
//! - 写しを送るたびに `lib.rs` の受け口が [`TreeWatch::refresh`] を呼ぶ(待たない。知らせを送るだけ)。
//!   張り替えは専用のスレッド 1 本(`arin-tree-watch`)が行い、いまの監視はこのスレッドだけが持つ。
//!   スレッドは選択中のプロジェクト・そのルートのパス・見つからない・隠しファイル表示を読み([`watch_target`])、
//!   前回と比べて変わっていれば、古い監視を捨ててから新しいルートに張る(切替・再リンク・見回りでのルートの追従・
//!   見つからない・削除・0 件)。隠しファイル表示だけが変わったときは張り替えず、除外の規則だけを差し替える
//! - `Workspace` のロックは、いまの状態を読むあいだだけ取る(`SessionRegistry` の読み取り)。張る・捨てるのはロックの外
//! - 受け口(監視のスレッドで呼ばれる)は、束をまとめて送るだけで戻る。ロックは取らない(規則は原子的な真偽値)
//! - 届くパスは実パス(`/private/var/...`)なので、まとめに使うルートも張る前に実パスにそろえる(`FileSystemPort::real_dir`)
//! - コピーの作業用フォルダ(`arin_platform::fs::STAGING_PREFIX`)の中と、その名前の出入りは知らせない
//!   (知らせるとコピーのたびにツリーが 2 回読み直される)
//! - イベントの中身(`TreeChange`)はプロジェクトの ID と相対パスだけ。絶対パスを載せない(設計書 §12)
//! - 張れなかったとき(フォルダが無いなど)は落とさず、標準エラーに `[arin:watch]` で 1 行出す。
//!   同じ対象には張り直さない(対象が変わったら、たとえば見つからない → 見つかったときに張り直す)
//! - 止めるとき([`TreeWatch::stop`]。アプリの終了時に `lifecycle.rs` が呼ぶ)は、スレッドが抜けるまで待つ。
//!   抜けるときに監視を捨てる(FSEvents のスレッドも止まる)
//! - 9 枚の検証画面では作らない(`lib.rs`)

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};

use arin_core::ports::fs::FileSystemPort;
use arin_core::ports::watch::{ActiveWatch, WatchBatch, WatcherPort};
use arin_core::project::ProjectId;
use arin_core::tree::TreeChange;
use arin_core::tree::changes::{ChangeBatch, summarize_changes};
use arin_core::tree::exclusion::ExclusionRule;
use arin_platform::fs::STAGING_PREFIX;

use crate::sessions::SessionRegistry;

/// 標準エラーの頭の文字列(監視を張れなかった)。
pub const WATCH_LOG_PREFIX: &str = "[arin:watch]";

/// まとめた変更の送り先(本物は `tree-changed` の emit。監視のスレッドから呼ばれる)。
pub type TreeChangeSink = Arc<dyn Fn(TreeChange) + Send + Sync>;

/// 監視する対象。選択中のプロジェクトがあり、見つかっているときだけある。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchTarget {
    pub project_id: ProjectId,
    /// 登録したルートのパス(`Workspace` が持つもの。実パスとは限らない)。
    pub root: PathBuf,
    pub show_hidden: bool,
}

/// いまの監視の対象を読む。選択中が無い・見つからない・ルートを引けないときは `None`。
#[must_use]
pub fn watch_target(sessions: &SessionRegistry) -> Option<WatchTarget> {
    let snapshot = sessions.snapshot();
    let id = snapshot.selected_project_id?;
    let card = snapshot.projects.into_iter().find(|p| p.id == id)?;
    if card.missing {
        return None;
    }
    let root = sessions.project_root_path(&id).ok()?;
    Some(WatchTarget {
        project_id: id,
        root,
        show_hidden: card.show_hidden,
    })
}

/// 監視の束を変わったフォルダにまとめる。`real_root` は監視が届けるパスと同じ形(実パス)のルート。
/// コピーの作業用フォルダの中とその名前の出入りは先に捨てる。
#[must_use]
pub fn summarize(real_root: &Path, batch: &WatchBatch, rule: &ExclusionRule) -> ChangeBatch {
    let paths = batch
        .paths
        .iter()
        .filter(|path| !in_staging(real_root, path))
        .cloned()
        .collect();
    let batch = WatchBatch {
        paths,
        rescan: batch.rescan,
    };
    summarize_changes(real_root, &batch, rule)
}

/// ルートの下で、どれかの要素がコピーの作業用フォルダの名前か。
fn in_staging(real_root: &Path, path: &Path) -> bool {
    path.strip_prefix(real_root).is_ok_and(|rest| {
        rest.components().any(|c| {
            c.as_os_str()
                .to_str()
                .is_some_and(|name| name.starts_with(STAGING_PREFIX))
        })
    })
}

/// いまの監視(張り替えのスレッドだけが持つ)。
struct Current {
    project_id: ProjectId,
    root: PathBuf,
    /// 受け口が読む隠しファイル表示。張り替えずに差し替える。
    show_hidden: Arc<AtomicBool>,
    /// 張った監視。張れなかったら `None`(同じ対象には張り直さない)。捨てると止まる。
    _watch: Option<Box<dyn ActiveWatch>>,
}

/// 張り替えの手順。張り替えのスレッドが持つ(状態を共有しない)。
struct Rewatcher {
    sessions: SessionRegistry,
    watcher: Arc<dyn WatcherPort>,
    fs: Arc<dyn FileSystemPort>,
    emit: TreeChangeSink,
    current: Option<Current>,
}

impl Rewatcher {
    fn apply(&mut self) {
        let target = watch_target(&self.sessions);
        if let (Some(current), Some(target)) = (&self.current, &target)
            && current.project_id == target.project_id
            && current.root == target.root
        {
            current
                .show_hidden
                .store(target.show_hidden, Ordering::SeqCst);
            return;
        }
        // 先に古い監視を捨てる(捨て終わったあと、古い監視の受け口は呼ばれない)。
        self.current = None;
        if let Some(target) = target {
            self.current = Some(self.start(target));
        }
    }

    fn start(&self, target: WatchTarget) -> Current {
        let show_hidden = Arc::new(AtomicBool::new(target.show_hidden));
        let watch = match self.fs.real_dir(&target.root) {
            Ok(real) => {
                let sink = sink_for(
                    target.project_id.clone(),
                    real.clone(),
                    Arc::clone(&show_hidden),
                    Arc::clone(&self.emit),
                );
                self.watcher.watch(&real, sink).map_err(|e| e.to_string())
            }
            Err(e) => Err(format!("ルートの実パスを取れなかった: {e}")),
        };
        let watch = watch
            .inspect_err(|e| {
                eprintln!(
                    "{WATCH_LOG_PREFIX} プロジェクト {} のツリーを監視できない: {e}",
                    target.project_id.as_str()
                );
            })
            .ok();
        Current {
            project_id: target.project_id,
            root: target.root,
            show_hidden,
            _watch: watch,
        }
    }
}

/// 監視の受け口。束をまとめ、送るものがあれば送る。ロックを取らない。
fn sink_for(
    project_id: ProjectId,
    real_root: PathBuf,
    show_hidden: Arc<AtomicBool>,
    emit: TreeChangeSink,
) -> arin_core::ports::watch::WatchSink {
    Box::new(move |batch: WatchBatch| {
        let rule = ExclusionRule::new(show_hidden.load(Ordering::SeqCst));
        let changes = summarize(&real_root, &batch, &rule);
        if !changes.is_empty() {
            emit(changes.into_tree_change(project_id.clone()));
        }
    })
}

/// ツリーの監視の張り替えのスレッド。Tauri の管理状態に登録し、アプリの終了時に止める。複製しても同じものを指す。
#[derive(Clone)]
pub struct TreeWatch {
    inner: Arc<Shared>,
}

struct Shared {
    requests: Mutex<Option<Sender<()>>>,
    handle: Mutex<Option<JoinHandle<()>>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl TreeWatch {
    /// 張り替えのスレッドを起動し、最初の張り替えを頼む。
    ///
    /// # Errors
    ///
    /// スレッドを起動できなかったとき。
    pub fn spawn(
        sessions: SessionRegistry,
        watcher: Arc<dyn WatcherPort>,
        fs: Arc<dyn FileSystemPort>,
        emit: TreeChangeSink,
    ) -> io::Result<Self> {
        let (tx, rx) = mpsc::channel::<()>();
        let mut rewatcher = Rewatcher {
            sessions,
            watcher,
            fs,
            emit,
            current: None,
        };
        let handle = thread::Builder::new()
            .name("arin-tree-watch".into())
            .spawn(move || run(&rx, &mut rewatcher))?;
        let _ = tx.send(());
        Ok(Self {
            inner: Arc::new(Shared {
                requests: Mutex::new(Some(tx)),
                handle: Mutex::new(Some(handle)),
            }),
        })
    }

    /// 張り替えを頼む(写しを送るたびに呼ぶ)。待たない。止めたあとは何もしない。
    pub fn refresh(&self) {
        if let Some(tx) = lock(&self.inner.requests).as_ref() {
            let _ = tx.send(());
        }
    }

    /// 止める。スレッドが抜ける(いまの監視を捨てる)まで待つ。2 回目以降は何もしない。
    pub fn stop(&self) {
        drop(lock(&self.inner.requests).take());
        let handle = lock(&self.inner.handle).take();
        if let Some(handle) = handle {
            let _ = handle.join();
        }
    }

    /// スレッドが動いているか。
    #[must_use]
    pub fn is_running(&self) -> bool {
        lock(&self.inner.handle)
            .as_ref()
            .is_some_and(|handle| !handle.is_finished())
    }
}

/// 頼まれるたびに張り替える。続けて届いた頼みは 1 回にまとめる。送り口が閉じたら(止めたら)監視を捨てて抜ける。
fn run(rx: &Receiver<()>, rewatcher: &mut Rewatcher) {
    while rx.recv().is_ok() {
        while rx.try_recv().is_ok() {}
        rewatcher.apply();
    }
    rewatcher.current = None;
}
