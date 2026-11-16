//! 設定ファイル(`settings.toml`)を読み、ショートカットの表と知らせにする(計画書 T65b、ADR-012 の 3・5、前提 (f)(g))。
//!
//! 流れ: `FsSettingsFile` で読む(`arin-platform`。TOML の解析もそこ)→ [`SettingsStore`] に当てる → `[shortcuts]` の行を
//! [`ShortcutOverride`] に直して [`apply_overrides`] に通す → 表と知らせ([`Resolved`])。規則は `arin-core` が持ち、
//! ここは型をつなぐだけにする(`settings` と `shortcuts` は互いの型を知らない。変換はここ)。
//!
//! - 起動時は、メニューを作る前に [`AppSettings::load`] で読む(`lib.rs`)。読んだ表を `menu::Shortcuts` に入れる
//! - 動作中の読み直し(T65c。`settings_watch.rs`)は [`AppSettings::reload_if_changed`](表か知らせが変わったときだけ
//!   結果を返す)。中身は [`AppSettings::reload`] と同じ。ファイル全体の問題(構文エラー・読めない)のときは
//!   `SettingsStore` が直前の内容を保つので、表も直前のままになる(起動時の「直前」は既定)
//! - 知らせは、ファイル全体の問題と行の問題をまとめて 1 行([`Resolved::notice`]。`get_settings_notice` が返す)。
//!   状態ファイルの知らせ(写しの `notice`)とは別に持つ(前提 (g))
//! - 標準エラーには `[arin:settings]` を頭に、問題 1 つにつき 1 行([`Resolved::diagnostics`]。[`report`] が書く)
//!
//! 設定ファイルは読むだけで、書き換え・退避・削除をしない(状態ファイルの退避の仕組み `persistence.rs` に渡さない)。
//! 例外は「設定ファイルを開く…」([`AppSettings::open_file`]。T65d、ADR-012 の 6)が、**無いときだけ**見本を作ること(上書きしない)。
//! 知らせと標準エラーに出すのは、コマンドの ID・キーの文字列・行と列・理由の種類だけ(中身の全文と絶対パスは出さない。AC-K09)。
//!
//! 「設定ファイルを開く…」の結果の知らせ(開けなかった・見本を作れなかった)は、設定ファイルの問題の知らせとは別の欄に持ち、
//! [`AppSettings::notice`] が 1 行にまとめて返す(開けなかった知らせが先)。設定ファイルの問題の知らせを消さない。
//! 読み直し([`AppSettings::reload_if_changed`])が「変わった」と比べるのは設定ファイルの問題の知らせと表だけで、
//! 変わったときに開けなかった知らせを外す(古くなるため)。中身の変わらない読み直し(見本の作成を監視が拾ったときなど)では外さない。

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use arin_core::paths::AppPaths;
use arin_core::ports::desktop::DesktopPort;
use arin_core::ports::settings_file::{SampleCreation, SettingsFilePort};
use arin_core::settings::{SettingValue, Settings, SettingsLoad, SettingsStore};
use arin_core::shortcuts::overrides::{
    OverrideValue, ShortcutOverride, ShortcutSettingProblem, apply_overrides, notice_line,
};
use arin_core::shortcuts::sample::sample_settings_toml;
use arin_core::shortcuts::table::ShortcutTable;
use arin_platform::storage::{FsSettingsFile, load_settings_file};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::events::SETTINGS_CHANGED_EVENT;

/// 標準エラーの行の頭の文字列(`docs/development-patterns.md` §1.1)。
pub const DIAGNOSTIC_PREFIX: &str = "[arin:settings]";

/// 設定を当てた結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// 使う表(既定の表に、問題の無い行を当てたもの)。
    pub table: ShortcutTable,
    /// 画面に出す知らせの 1 行。問題が無ければ `None`。
    pub notice: Option<String>,
    /// 標準エラーに書く行(問題 1 つにつき 1 行。頭は [`DIAGNOSTIC_PREFIX`])。
    pub diagnostics: Vec<String>,
}

/// `[shortcuts]` の行を、ショートカットの規則の入力に直す(書いた順のまま)。
#[must_use]
pub fn shortcut_overrides(settings: &Settings) -> Vec<ShortcutOverride> {
    settings
        .shortcuts()
        .iter()
        .map(|entry| ShortcutOverride {
            id: entry.key.clone(),
            value: match &entry.value {
                SettingValue::Text(text) => OverrideValue::Text(text.clone()),
                SettingValue::NotText => OverrideValue::NotText,
            },
        })
        .collect()
}

/// 使っている設定の内容を既定の表に当て、表・知らせ・標準エラーの行にする。
#[must_use]
pub fn resolve(store: &SettingsStore, defaults: &ShortcutTable) -> Resolved {
    let (table, problems) = apply_overrides(defaults, &shortcut_overrides(store.settings()));
    Resolved {
        table,
        notice: combined_notice(store, &problems),
        diagnostics: diagnostic_lines(store, &problems),
    }
}

/// ファイル全体(または節)の問題の知らせと、行の問題の知らせを 1 行にまとめる。
fn combined_notice(store: &SettingsStore, problems: &[ShortcutSettingProblem]) -> Option<String> {
    match (store.notice_line(), notice_line(problems)) {
        (None, None) => None,
        (Some(one), None) | (None, Some(one)) => Some(one),
        (Some(file), Some(rows)) => Some(format!("{file}。{rows}")),
    }
}

/// 標準エラーに書く行(ファイル全体の問題が先、行の問題は表の順)。
fn diagnostic_lines(store: &SettingsStore, problems: &[ShortcutSettingProblem]) -> Vec<String> {
    store
        .problem()
        .map(|p| p.diagnostic_line())
        .into_iter()
        .chain(problems.iter().map(ShortcutSettingProblem::diagnostic_line))
        .map(|line| format!("{DIAGNOSTIC_PREFIX} {line}"))
        .collect()
}

/// 標準エラーの行を書く。
pub fn report(resolved: &Resolved) {
    for line in &resolved.diagnostics {
        eprintln!("{line}");
    }
}

/// 既定のアプリで開けず、Finder で表示したときの知らせ【仮定】。
pub const NOTICE_NO_APP_REVEALED: &str =
    "設定ファイルを開けるアプリが無いため、Finder で表示しました";
/// ふつうのファイルでない・実行できる設定ファイルを、開かずに Finder で表示したときの知らせ【仮定】。
pub const NOTICE_NOT_PLAIN_REVEALED: &str =
    "設定ファイルがふつうのファイルでないか実行できるため、開かずに Finder で表示しました";
/// 既定のアプリでも Finder でも出せなかったときの知らせ【仮定】。
pub const NOTICE_NOT_OPENED: &str = "設定ファイルを開けず、Finder でも表示できませんでした";
/// 見本を作れなかったときの知らせの頭【仮定】。後ろに理由の種類を括弧で付ける。
pub const NOTICE_SAMPLE_FAILED: &str = "設定ファイルの見本を作れなかったため、開きませんでした";

/// 「設定ファイルを開く…」の結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenOutcome {
    /// 既定のアプリで開いた。`created` は見本を作ったか。
    Opened {
        /// 無かったので見本を作った。
        created: bool,
    },
    /// 開かずに(開けずに)Finder で表示した。
    Revealed,
    /// 既定のアプリでも Finder でも出せなかった。
    NotOpened,
    /// 見本を作れなかったので開かなかった。
    SampleNotCreated,
}

/// [`AppSettings::open_file`] の結果と、知らせ([`AppSettings::notice`])が変わったか。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenReport {
    /// 結果。
    pub outcome: OpenOutcome,
    /// 知らせが前と変わった(画面へ `settings-changed` を送る)。
    pub notice_changed: bool,
}

/// 設定の管理状態(`Builder::manage` で登録する)。設定ファイルの読み口と、使っている内容・知らせを持つ。
///
/// 表そのものは `menu::Shortcuts` が持つ(メニューと `get_shortcuts` が見る)。ここは知らせ(`get_settings_notice`)と、
/// 読み直し(T65c)のための直前の内容を持つ。
#[derive(Debug)]
pub struct AppSettings {
    file: FsSettingsFile,
    /// 設定ファイルの絶対パス(Finder・既定のアプリに渡すだけ。知らせ・標準エラーには出さない)。
    path: PathBuf,
    state: Mutex<State>,
}

#[derive(Debug, Default)]
struct State {
    store: SettingsStore,
    /// 設定ファイルの問題の知らせ(直前に当てた [`Resolved::notice`])。
    file_notice: Option<String>,
    /// 「設定ファイルを開く…」の結果の知らせ(開けなかった・見本を作れなかった)。開けたら外す。
    open_notice: Option<String>,
}

impl State {
    /// 画面に出す 1 行(開けなかった知らせが先)。
    fn notice(&self) -> Option<String> {
        match (&self.open_notice, &self.file_notice) {
            (None, None) => None,
            (Some(one), None) | (None, Some(one)) => Some(one.clone()),
            (Some(open), Some(file)) => Some(format!("{open}。{file}")),
        }
    }

    /// 開いた結果の知らせを替え、画面に出す 1 行が変わったら真。
    fn set_open_notice(&mut self, notice: Option<String>) -> bool {
        let before = self.notice();
        self.open_notice = notice;
        self.notice() != before
    }
}

impl AppSettings {
    /// 起動時に読む。状態フォルダ(差し替えたら差し替え先)の `settings.toml` だけを読む(AC-K10)。
    /// 無ければ既定の表で、知らせは無い。標準エラーには書かない(呼ぶ側が [`report`] で書く)。
    #[must_use]
    pub fn load(paths: &AppPaths) -> (Self, Resolved) {
        let settings = Self {
            file: FsSettingsFile::new(paths.state_dir().to_path_buf()),
            path: paths.settings_file(),
            state: Mutex::new(State::default()),
        };
        let resolved = settings.reload();
        (settings, resolved)
    }

    /// 設定ファイルを読み直して当てる(T65c が保存を検知したときに使う)。ファイル全体の問題のときは
    /// 直前の内容のまま当てる。設定ファイルの問題の知らせを結果の知らせに替える。表の差し替え・メニューの作り直し・画面への
    /// イベント(`settings-changed`)・標準エラーへの書き出しは呼ぶ側が行う。
    pub fn reload(&self) -> Resolved {
        let load = load_settings_file(&self.file);
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        Self::apply(&mut state, load)
    }

    /// 読み直して当て、表が `current` と違うか設定ファイルの問題の知らせが直前と違うときだけ結果を返す(T65c。保存を検知したとき)。
    /// 同じなら `None`(内容と知らせは読み直したものになるが、見た目は変わらない)。直前の知らせとの比べと
    /// 当てるのは 1 つのロックの中で行う。返すとき(変わったとき)は「設定ファイルを開く…」の知らせを外す
    /// (画面は `settings-changed` で [`Self::notice`] を取り直し、設定ファイルの問題の知らせだけになる)。
    pub fn reload_if_changed(&self, current: &ShortcutTable) -> Option<Resolved> {
        let load = load_settings_file(&self.file);
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let before = state.file_notice.clone();
        let resolved = Self::apply(&mut state, load);
        let changed = resolved.table != *current || resolved.notice != before;
        if changed {
            state.open_notice = None;
        }
        changed.then_some(resolved)
    }

    /// 読んだ結果を当てる。
    fn apply(state: &mut State, load: SettingsLoad) -> Resolved {
        state.store.apply(load);
        let resolved = resolve(&state.store, &ShortcutTable::default_table());
        state.file_notice.clone_from(&resolved.notice);
        resolved
    }

    /// いまの知らせの 1 行(開けなかった知らせと設定ファイルの問題の知らせ)。どちらも無ければ `None`。
    #[must_use]
    pub fn notice(&self) -> Option<String> {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .notice()
    }

    /// 「設定ファイルを開く…」(T65d、ADR-012 の 6、AC-K08)。待つことがあるので、メインスレッドの外で呼ぶ。
    ///
    /// 1. 設定ファイルが**無いときだけ**、既定の表から作った見本(`sample_settings_toml`)で作る(0600。あれば中身を変えない)。
    ///    作れなければ開かずに知らせる
    /// 2. ふつうのファイルで実行の権限が無ければ、既定のアプリで開く。開けなければ Finder で表示して知らせる。
    ///    ふつうのファイルでない(フォルダ・先の無いリンクなど)か実行できるときは、開かずに Finder で表示して知らせる
    ///    (既定のアプリが実行・起動しうるため。`commands/desktop.rs` の「開く」と同じ考え)
    ///
    /// 開けたら前の「開けなかった」知らせを外す。知らせと標準エラー(`[arin:settings]`)にパスと OS の文言を出さない。
    /// 設定ファイルを読み直さない(見本は既定の表と同じ。保存の検知が読み直す)。
    pub fn open_file(&self, desktop: &dyn DesktopPort) -> OpenReport {
        let sample = sample_settings_toml(&ShortcutTable::default_table());
        let (outcome, notice) = match self.file.create_sample_if_missing(sample.as_bytes()) {
            Err(e) => {
                eprintln!("{DIAGNOSTIC_PREFIX} 設定ファイルの見本を作れなかった({e})");
                (
                    OpenOutcome::SampleNotCreated,
                    Some(format!("{NOTICE_SAMPLE_FAILED}({e})")),
                )
            }
            Ok(creation) => self.open_or_reveal(desktop, creation == SampleCreation::Created),
        };
        let notice_changed = self
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .set_open_notice(notice);
        OpenReport {
            outcome,
            notice_changed,
        }
    }

    /// 開くか、開けなければ Finder で表示する。結果と、出す知らせ(開けたら `None`)。
    fn open_or_reveal(
        &self,
        desktop: &dyn DesktopPort,
        created: bool,
    ) -> (OpenOutcome, Option<String>) {
        // リンクはたどって調べる(リンクの先を開く。ADR-012 の 1)。
        let plain = std::fs::metadata(&self.path)
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 == 0);
        let revealed_notice = if plain {
            // 失敗の文言は絶対パスを含むので出さない。
            if desktop.open_with_default_app(&self.path).is_ok() {
                return (OpenOutcome::Opened { created }, None);
            }
            eprintln!("{DIAGNOSTIC_PREFIX} 設定ファイルを既定のアプリで開けなかった");
            NOTICE_NO_APP_REVEALED
        } else {
            eprintln!(
                "{DIAGNOSTIC_PREFIX} 設定ファイルがふつうのファイルでないか実行できるため、開かない"
            );
            NOTICE_NOT_PLAIN_REVEALED
        };
        if desktop.reveal_in_finder(&self.path).is_ok() {
            (OpenOutcome::Revealed, Some(revealed_notice.to_owned()))
        } else {
            eprintln!("{DIAGNOSTIC_PREFIX} 設定ファイルを Finder で表示できなかった");
            (OpenOutcome::NotOpened, Some(NOTICE_NOT_OPENED.to_owned()))
        }
    }
}

/// 「設定ファイルを開く…」が Finder・既定のアプリに渡す口(アプリの管理状態。`lib.rs` が `Builder::manage` で登録する)。
/// テストは偽の `DesktopPort` を渡す(本物の Finder を開かない)。
pub struct SettingsOpener {
    desktop: Arc<dyn DesktopPort>,
}

impl SettingsOpener {
    /// 口を持たせる。
    #[must_use]
    pub fn new(desktop: Arc<dyn DesktopPort>) -> Self {
        Self { desktop }
    }
}

/// メニューの「設定ファイルを開く…」が選ばれたとき(`menu::on_menu_event` から。メインスレッドで呼ばれる)。
///
/// 待ってよいスレッドで [`AppSettings::open_file`] を呼び、知らせが変わったら画面へ `settings-changed` を送る
/// (画面は `get_settings_notice` を取り直す)。管理状態が無ければ何もしない。
pub fn open_from_menu<R: Runtime>(app: &AppHandle<R>) {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (Some(settings), Some(opener)) = (
            handle.try_state::<AppSettings>(),
            handle.try_state::<SettingsOpener>(),
        ) else {
            return;
        };
        let report = settings.open_file(opener.desktop.as_ref());
        if report.notice_changed {
            // 画面がまだ無いときなどは送れない。画面は起動時に知らせを取るので捨ててよい。
            let _ = handle.emit(SETTINGS_CHANGED_EVENT, ());
        }
    });
}
