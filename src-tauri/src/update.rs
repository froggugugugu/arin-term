//! 新しい版の確認(ADR-018)。
//!
//! 確かめるのは 3 つの時機だけ: 起動時(設定 `ui.checkUpdates` が真のとき)、設定でオンにした直後、
//! メニューの「アップデートを確認…」。どれも別のスレッドで問い合わせ(`ReleasePort`)、結果を [`UPDATE_CHANGED_EVENT`] で送る。
//! 起動時・オンにした直後は新しい版があるときだけ知らせ、メニューからは「最新です」「確かめられませんでした」も知らせる。
//! 入れ替えはしない。画面の「ダウンロードのページを開く」で、Rust が持つ版から作った Releases のページをブラウザで開く。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;

use arin_core::ports::desktop::{DesktopError, DesktopPort};
use arin_core::ports::release::ReleasePort;
use arin_core::update::{RELEASES_PAGE, UpdateNotice, Version, notice_for};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::events::UPDATE_CHANGED_EVENT;
use crate::sessions::SessionRegistry;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// 問い合わせの口・ブラウザで開く口と、いまの知らせ(Tauri の管理する状態)。
pub struct UpdateChecker {
    port: Mutex<Arc<dyn ReleasePort>>,
    desktop: Arc<dyn DesktopPort>,
    notice: Mutex<Option<UpdateNotice>>,
    /// 問い合わせ中か(重ねて問い合わせない)。
    running: AtomicBool,
}

impl UpdateChecker {
    #[must_use]
    pub fn new(port: Arc<dyn ReleasePort>, desktop: Arc<dyn DesktopPort>) -> Self {
        Self {
            port: Mutex::new(port),
            desktop,
            notice: Mutex::new(None),
            running: AtomicBool::new(false),
        }
    }

    /// 問い合わせの口を差し替える(結合テストが偽物に替える。本物は `CurlReleases`)。
    pub fn use_port(&self, port: Arc<dyn ReleasePort>) {
        *lock(&self.port) = port;
    }

    /// いまの知らせ(`get_update_notice`)。
    #[must_use]
    pub fn notice(&self) -> Option<UpdateNotice> {
        lock(&self.notice).clone()
    }

    /// 知らせを閉じる(`dismiss_update_notice`)。
    pub fn dismiss(&self) {
        *lock(&self.notice) = None;
    }

    /// Releases のページを既定のブラウザで開く。新しい版を知らせているならその版のページ、ほかは一覧のページ。
    ///
    /// # Errors
    ///
    /// ブラウザに渡せなかったとき。
    pub fn open_release_page(&self) -> Result<(), DesktopError> {
        let url = match self.notice() {
            Some(UpdateNotice::Available { version }) => Version::parse(&version)
                .map_or_else(|| RELEASES_PAGE.to_owned(), |v| v.release_page()),
            _ => RELEASES_PAGE.to_owned(),
        };
        self.desktop.open_release_page(&url)
    }

    /// 結果を持ち、画面へ送る。`None` は何もしない(起動時の確認で新しい版が無いとき)。
    /// 「新しい版があります」を出しているあいだは、確かめられなかった結果で上書きしない(ページを開くボタンを消さない)。
    fn publish<R: Runtime>(&self, app: &AppHandle<R>, notice: Option<UpdateNotice>) {
        let Some(notice) = notice else {
            return;
        };
        {
            let mut current = lock(&self.notice);
            if matches!(notice, UpdateNotice::Failed { .. })
                && matches!(*current, Some(UpdateNotice::Available { .. }))
            {
                return;
            }
            *current = Some(notice.clone());
        }
        let _ = app.emit(UPDATE_CHANGED_EVENT, Some(notice));
    }

    /// 設定をオフにしたとき: 知らせを消し、画面からも消す。
    pub fn clear<R: Runtime>(&self, app: &AppHandle<R>) {
        if lock(&self.notice).take().is_some() {
            let _ = app.emit(UPDATE_CHANGED_EVENT, None::<UpdateNotice>);
        }
    }
}

/// 別のスレッドで新しい版を確かめる。問い合わせ中なら何もしない。`manual` はメニューから確かめたか。
pub fn check<R: Runtime>(app: &AppHandle<R>, manual: bool) {
    let checker = app.state::<UpdateChecker>();
    if checker.running.swap(true, Ordering::AcqRel) {
        return;
    }
    let port = Arc::clone(&lock(&checker.port));
    let current = app.package_info().version.to_string();
    let handle = app.clone();
    let spawned = thread::Builder::new()
        .name("arin-update-check".to_owned())
        .spawn(move || {
            // panic しても「問い合わせ中」を戻す(戻らないと、再起動まで確かめられなくなる)
            let _running = RunningGuard(handle.clone());
            let fetched = port.fetch_latest();
            let notice = notice_for(
                &current,
                fetched.as_deref().map_err(|e| e.0.as_str()),
                manual,
            );
            // 起動時・オンにした直後の確認は、待つあいだに設定がオフにされたら知らせない
            if !manual && !handle.state::<SessionRegistry>().check_updates() {
                return;
            }
            handle.state::<UpdateChecker>().publish(&handle, notice);
        });
    if spawned.is_err() {
        checker.running.store(false, Ordering::Release);
        let notice = manual.then(|| UpdateNotice::Failed {
            reason: "確認を始められませんでした".to_owned(),
        });
        checker.publish(app, notice);
    }
}

/// 確認のスレッドが終わるとき(panic を含む)に「問い合わせ中」を戻す。
struct RunningGuard<R: Runtime>(AppHandle<R>);

impl<R: Runtime> Drop for RunningGuard<R> {
    fn drop(&mut self) {
        self.0
            .state::<UpdateChecker>()
            .running
            .store(false, Ordering::Release);
    }
}

/// 問い合わせ中か(テスト用。終わるのを待つ)。
#[must_use]
pub fn is_running<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.state::<UpdateChecker>().running.load(Ordering::Acquire)
}
