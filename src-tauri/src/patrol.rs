//! ルートフォルダへの追従の見回り(計画書 T33d、FR-007、設計書 §7.1「ルートフォルダへの追従」「スレッドの分担」)。
//!
//! 専用のスレッド 1 本([`PatrolThread`])が 1 秒ごとに [`RootPatrol::tick`] を呼ぶ。1 回の見回りでは:
//!
//! 1. 登録済みのプロジェクトの一覧を `SessionRegistry` から取り、開いているフォルダの一覧を張り替える
//!    (削除されたものを閉じ、再リンクなどでルートが変わったものを開き直し、まだ開いていないものを開く)
//! 2. 開いたフォルダの現在位置(`OpenRoot::current_path`)を調べ、変わっていたらブックマークを作り直して追従する
//!    (`SessionRegistry::follow_project_root`。写しを送ると保存が予約される)。ゴミ箱の中なら見つからない(規則は `arin-core`)
//! 3. 取れなかったら、すぐには見つからないにしない(改名と同時に調べるとまれに取れない)。続けて
//!    [`MISSING_AFTER_FAILURES`] 回取れなかったら、開いたフォルダを手放して開き直しを試み(元のパス、次にブックマーク)、
//!    それでも見つからなければ見つからないにする。カードとタブは残す(C-06)
//! 4. 見つからないプロジェクトも毎回開き直しを試み、見つかれば追従して見つからないを解く(タブは開かない)
//!
//! `OpenRoot` は `Send` だが `Sync` ではないので、開いたフォルダは見回りのスレッドだけが持つ。
//! 入出力(フォルダを開く・調べる・ブックマーク)は `Workspace` のロックの外で行う。

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use arin_core::ports::{BookmarkPort, OpenRoot, RootLocatorPort};
use arin_core::project::{ProjectId, RootReference};
use arin_core::workspace::ProjectRoot;

use crate::sessions::SessionRegistry;

/// 見回りの間隔(設計書 §7.1「スレッドの分担」)。
pub const PATROL_INTERVAL: Duration = Duration::from_secs(1);

/// 開いたフォルダの現在位置が続けて何回取れなかったら、見つからないかを確かめにいくか。
/// 1 回の失敗では決めず、次の見回りで確かめ直す。
pub const MISSING_AFTER_FAILURES: u32 = 2;

/// 1 つのプロジェクトについて見回りが持つもの。
struct Tracked {
    /// 見回りが最後に知った `Workspace` のルートのパス。これと違えば、ほかの操作(再リンク)で変わったので開き直す。
    root_path: PathBuf,
    /// 開いたフォルダ。開けていなければ `None`。
    handle: Option<Box<dyn OpenRoot>>,
    /// 現在位置が続けて取れなかった回数。
    failures: u32,
    /// 追従しても何も変わらなかった位置(ゴミ箱の中・別のプロジェクトと同じ場所)。位置が変わるまで追従し直さない。
    ignored: Option<PathBuf>,
}

impl Tracked {
    fn new(root_path: PathBuf) -> Self {
        Self {
            root_path,
            handle: None,
            failures: 0,
            ignored: None,
        }
    }
}

/// 見回りの 1 回分の処理と、開いたフォルダの一覧。見回りのスレッドが所有する(テストは `tick` を直接呼ぶ)。
pub struct RootPatrol {
    roots: Arc<dyn RootLocatorPort>,
    bookmarks: Arc<dyn BookmarkPort>,
    tracked: HashMap<ProjectId, Tracked>,
}

impl RootPatrol {
    #[must_use]
    pub fn new(roots: Arc<dyn RootLocatorPort>, bookmarks: Arc<dyn BookmarkPort>) -> Self {
        Self {
            roots,
            bookmarks,
            tracked: HashMap::new(),
        }
    }

    /// 見回りの 1 回分。待たない(フォルダを調べる入出力だけ)。
    pub fn tick(&mut self, sessions: &SessionRegistry) {
        let targets = sessions.project_roots();
        self.tracked
            .retain(|id, _| targets.iter().any(|target| &target.id == id));
        for target in targets {
            let path = target.root.last_known_path().to_path_buf();
            let entry = self
                .tracked
                .entry(target.id.clone())
                .or_insert_with(|| Tracked::new(path.clone()));
            if entry.root_path != path {
                *entry = Tracked::new(path.clone());
            }
            match locate(self.roots.as_ref(), self.bookmarks.as_ref(), entry, &target) {
                Some(found) => {
                    entry.failures = 0;
                    if (found != path || target.missing) && entry.ignored.as_ref() != Some(&found) {
                        follow(
                            sessions,
                            self.bookmarks.as_ref(),
                            entry,
                            &target,
                            &path,
                            found,
                        );
                    }
                }
                None => {
                    if entry.failures >= MISSING_AFTER_FAILURES && !target.missing {
                        // 一覧から読んだばかりの ID なので、失敗するのは同時に削除されたときだけ(次の見回りで外れる)。
                        let _ = sessions.mark_project_missing(&target.id, &path);
                    }
                }
            }
        }
    }
}

/// プロジェクトのルートのいまの位置。取れなければ `None`(失敗の回数を数える)。
///
/// 開いたフォルダがあればその現在位置を返す。取れなかったら失敗を数え、続けて `MISSING_AFTER_FAILURES` 回に達するまでは
/// 開いたまま次の見回りを待つ。達したら(または開いていなければ)開き直しを試みる。
fn locate(
    roots: &dyn RootLocatorPort,
    bookmarks: &dyn BookmarkPort,
    entry: &mut Tracked,
    target: &ProjectRoot,
) -> Option<PathBuf> {
    if let Some(handle) = &entry.handle {
        match handle.current_path() {
            Ok(path) => return Some(path),
            Err(_) => {
                entry.failures = entry.failures.saturating_add(1);
                if entry.failures < MISSING_AFTER_FAILURES {
                    return None;
                }
                entry.handle = None;
                // この回の失敗は数えたので、開き直せなければそのまま見つからないを確かめる。
                return reopen_into(roots, bookmarks, entry, target);
            }
        }
    }
    let found = reopen_into(roots, bookmarks, entry, target);
    if found.is_none() {
        entry.failures = entry.failures.saturating_add(1);
    }
    found
}

/// 開き直しを試み、開けたら `entry` に持たせてその位置を返す。
fn reopen_into(
    roots: &dyn RootLocatorPort,
    bookmarks: &dyn BookmarkPort,
    entry: &mut Tracked,
    target: &ProjectRoot,
) -> Option<PathBuf> {
    let (path, handle) = reopen(roots, bookmarks, target)?;
    entry.handle = Some(handle);
    Some(path)
}

/// ルートを開き直す。最後に分かっていたパスを開き、開けなければブックマークを解決した先を開く。
fn reopen(
    roots: &dyn RootLocatorPort,
    bookmarks: &dyn BookmarkPort,
    target: &ProjectRoot,
) -> Option<(PathBuf, Box<dyn OpenRoot>)> {
    let path = target.root.last_known_path();
    if let Ok(handle) = roots.open(path) {
        return Some((path.to_path_buf(), handle));
    }
    let resolved = bookmarks.resolve(target.root.bookmark()?).ok()?;
    let handle = roots.open(&resolved.path).ok()?;
    Some((resolved.path, handle))
}

/// 見つかった位置に追従する。ブックマークを作り直し(作れなければ前のもの)、`Workspace` に渡す。
fn follow(
    sessions: &SessionRegistry,
    bookmarks: &dyn BookmarkPort,
    entry: &mut Tracked,
    target: &ProjectRoot,
    expected: &Path,
    found: PathBuf,
) {
    let bookmark = bookmarks
        .create(&found)
        .ok()
        .or_else(|| target.root.bookmark().map(ToOwned::to_owned));
    let result = sessions.follow_project_root(
        &target.id,
        expected,
        RootReference::new(found.clone(), bookmark),
    );
    let now = sessions
        .project_root_path(&target.id)
        .unwrap_or_else(|_| expected.to_path_buf());
    // ゴミ箱の中・別のプロジェクトと同じ場所など、別の位置が見つかったのにルートが動かなかったときは、
    // 位置が変わるまで追従し直さない(毎回ブックマークを作り直さない)。
    let stuck = !matches!(result, Ok(true)) || (now == expected && found != expected);
    entry.ignored = stuck.then_some(found);
    entry.root_path = now;
}

/// 見回りのスレッド。Tauri の管理状態に登録し、アプリの終了時に止める。
pub struct PatrolThread {
    stop: Mutex<Option<Sender<()>>>,
    handle: Mutex<Option<JoinHandle<()>>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl PatrolThread {
    /// 見回りのスレッドを起動する。`interval` ごとに `patrol.tick` を呼ぶ(本物は [`PATROL_INTERVAL`])。
    ///
    /// # Errors
    ///
    /// スレッドを起動できなかったとき。
    pub fn spawn(
        sessions: SessionRegistry,
        mut patrol: RootPatrol,
        interval: Duration,
    ) -> io::Result<Self> {
        let (stop, stopped) = mpsc::channel::<()>();
        let handle = thread::Builder::new()
            .name("arin-root-patrol".into())
            .spawn(move || {
                // 止める知らせ(または送り手が落ちたこと)が来るまで、間隔ごとに見回る。
                while let Err(RecvTimeoutError::Timeout) = stopped.recv_timeout(interval) {
                    patrol.tick(&sessions);
                }
            })?;
        Ok(Self {
            stop: Mutex::new(Some(stop)),
            handle: Mutex::new(Some(handle)),
        })
    }

    /// 見回りを止め、スレッドが抜けるまで待つ(見回りの途中なら、その 1 回分が終わるまで)。開いていたフォルダは閉じる。
    /// 2 回目以降は何もしない。
    pub fn stop(&self) {
        if let Some(stop) = lock(&self.stop).take() {
            let _ = stop.send(());
        }
        if let Some(handle) = lock(&self.handle).take() {
            let _ = handle.join();
        }
    }

    /// スレッドが動いているか。
    #[must_use]
    pub fn is_running(&self) -> bool {
        lock(&self.handle)
            .as_ref()
            .is_some_and(|handle| !handle.is_finished())
    }
}
