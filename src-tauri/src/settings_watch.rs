//! 設定ファイルの保存の検知と即時の反映(計画書 T65c、ADR-012 の 5、設計書 §1.3 #10、AC-K05・AC-K07)。
//!
//! - 専用のスレッド 1 本(`arin-settings-watch`)が、`WatcherPort` で**状態フォルダ**に監視を張る(エディタは一時ファイルに
//!   書いて改名して保存することが多いので、ファイルではなくフォルダに張る)。状態フォルダが無ければ 0700 で作ってから張る
//!   (`FsStateFile` と同じ権限)。設定ファイルは作らない
//! - 届いた束に、状態フォルダの直下の `settings.toml` があるとき(または OS が個々の変更を伝えきれなかったとき)だけ
//!   読み直しを頼む。`state.json` の書き込みなどは捨てる。受け口(監視のスレッド)は頼むだけで戻る
//! - 読み直し([`reload_and_publish`])は、表か知らせが今と違うときだけ、標準エラーに書き、表を差し替え、表が変わったら
//!   メインスレッドでメニューを作り直し、画面へ `settings-changed` を送る。知らせだけが変わったときも送る。
//!   構文エラーのときは直前の表のまま知らせだけが替わる(`SettingsStore`)。`Workspace` のロックは取らない
//! - 張れなかったときは落とさず、標準エラーに `[arin:settings]` で 1 行出す(パスを出さない)。保存しても読み直さない
//! - 止めるとき([`SettingsWatch::stop`]。アプリの終了時に `lifecycle.rs` が呼ぶ)は、スレッドが抜けるまで待つ。
//!   抜けるときに監視を捨てる(FSEvents のスレッドも止まる)
//! - シンボリックリンクの `settings.toml` のリンクの先の変更は拾えない(ADR-012 の 1・5。リンク自体を触るか再起動する)

use std::fs::{self, DirBuilder};
use std::io;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};

use arin_core::ports::settings_file::SETTINGS_FILE_NAME;
use arin_core::ports::watch::{ActiveWatch, WatchBatch, WatcherPort};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::events::SETTINGS_CHANGED_EVENT;
use crate::menu::{self, Shortcuts};
use crate::settings::{self, AppSettings, DIAGNOSTIC_PREFIX};

/// 状態フォルダの権限(作るときだけ。`FsStateFile` と同じ)。
const DIR_MODE: u32 = 0o700;

/// 保存を検知したときに呼ぶもの(本物は [`reload_and_publish`]。監視のスレッドとは別の、設定の監視のスレッドで呼ばれる)。
pub type SavedHandler = Arc<dyn Fn() + Send + Sync>;

/// 束に、状態フォルダ(実パス)の直下の `settings.toml` があるか。OS が個々の変更を伝えきれなかったときも真。
#[must_use]
pub fn touches_settings_file(real_state_dir: &Path, batch: &WatchBatch) -> bool {
    batch.rescan
        || batch.paths.iter().any(|path| {
            path.parent() == Some(real_state_dir)
                && path.file_name() == Some(SETTINGS_FILE_NAME.as_ref())
        })
}

/// 設定ファイルを読み直し、表か知らせが今と違うときだけ反映して画面へ知らせる。反映したら真。
///
/// 標準エラーに `[arin:settings]` の行を書き → 表を差し替え → 表が変わったらメインスレッドでメニューを作り直し
/// (待たない)→ `settings-changed` を送る。設定の管理状態が無ければ何もしない。
pub fn reload_and_publish<R: Runtime>(app: &AppHandle<R>) -> bool {
    let (Some(settings), Some(shortcuts)) =
        (app.try_state::<AppSettings>(), app.try_state::<Shortcuts>())
    else {
        return false;
    };
    let Some(resolved) = settings.reload_if_changed(&shortcuts.table()) else {
        return false;
    };
    settings::report(&resolved);
    if shortcuts.replace(resolved.table) {
        let handle = app.clone();
        let scheduled = app.run_on_main_thread(move || {
            if let Err(e) = menu::rebuild(&handle) {
                eprintln!(
                    "{DIAGNOSTIC_PREFIX} メニューを作り直せなかった(前のメニューのまま): {e}"
                );
            }
        });
        if let Err(e) = scheduled {
            eprintln!(
                "{DIAGNOSTIC_PREFIX} メニューの作り直しを頼めなかった(前のメニューのまま): {e}"
            );
        }
    }
    // 画面がまだ無いときなどは送れない。画面は起動時に表と知らせを取るので捨ててよい。
    let _ = app.emit(SETTINGS_CHANGED_EVENT, ());
    true
}

/// 設定の監視のスレッドへの頼み。
enum Request {
    /// 設定ファイルが保存された(読み直す)。
    Saved,
    /// 止める。
    Stop,
}

/// 設定ファイルの監視のスレッド。Tauri の管理状態に登録し、アプリの終了時に止める。複製しても同じものを指す。
#[derive(Clone)]
pub struct SettingsWatch {
    inner: Arc<Shared>,
}

struct Shared {
    requests: Mutex<Option<Sender<Request>>>,
    handle: Mutex<Option<JoinHandle<()>>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl SettingsWatch {
    /// 監視のスレッドを起動する。状態フォルダを作って張るのはスレッドの中(張れなくても落とさない)。
    ///
    /// # Errors
    ///
    /// スレッドを起動できなかったとき。
    pub fn spawn(
        state_dir: PathBuf,
        watcher: Arc<dyn WatcherPort>,
        on_saved: SavedHandler,
    ) -> io::Result<Self> {
        let (tx, rx) = mpsc::channel::<Request>();
        let sink_tx = tx.clone();
        let handle = thread::Builder::new()
            .name("arin-settings-watch".into())
            .spawn(move || {
                let watch = start(&state_dir, watcher.as_ref(), sink_tx);
                run(&rx, on_saved.as_ref());
                drop(watch);
            })?;
        Ok(Self {
            inner: Arc::new(Shared {
                requests: Mutex::new(Some(tx)),
                handle: Mutex::new(Some(handle)),
            }),
        })
    }

    /// 止める。スレッドが抜ける(監視を捨てる)まで待つ。2 回目以降は何もしない。
    pub fn stop(&self) {
        if let Some(tx) = lock(&self.inner.requests).take() {
            let _ = tx.send(Request::Stop);
        }
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

/// 状態フォルダを(無ければ 0700 で)作り、実パスにそろえて張る。張れなければ標準エラーに 1 行出して `None`。
fn start(
    state_dir: &Path,
    watcher: &dyn WatcherPort,
    tx: Sender<Request>,
) -> Option<Box<dyn ActiveWatch>> {
    let prepared = ensure_dir(state_dir).and_then(|()| fs::canonicalize(state_dir));
    let real = match prepared {
        Ok(real) => real,
        Err(e) => {
            eprintln!(
                "{DIAGNOSTIC_PREFIX} 状態フォルダを用意できないため、設定ファイルの保存を検知しない({:?})",
                e.kind()
            );
            return None;
        }
    };
    let root = real.clone();
    let sink = Box::new(move |batch: WatchBatch| {
        if touches_settings_file(&root, &batch) {
            let _ = tx.send(Request::Saved);
        }
    });
    // 監視の失敗の文言はパスを含むので出さない。
    watcher
        .watch(&real, sink)
        .inspect_err(|_| {
            eprintln!(
                "{DIAGNOSTIC_PREFIX} 設定ファイルの監視を張れないため、保存しても読み直さない"
            );
        })
        .ok()
}

/// 状態フォルダが無ければ 0700 で作る。あれば権限を変えない。親のフォルダは作らない。
fn ensure_dir(dir: &Path) -> io::Result<()> {
    match DirBuilder::new().mode(DIR_MODE).create(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(e),
    }
}

/// 頼まれるたびに読み直す。続けて届いた頼みは 1 回にまとめる。止める頼みで抜ける。
fn run(rx: &Receiver<Request>, on_saved: &(dyn Fn() + Send + Sync)) {
    while let Ok(request) = rx.recv() {
        if matches!(request, Request::Stop) {
            return;
        }
        while let Ok(next) = rx.try_recv() {
            if matches!(next, Request::Stop) {
                return;
            }
        }
        on_saved();
    }
}
