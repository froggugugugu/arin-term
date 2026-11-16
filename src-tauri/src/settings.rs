//! 設定ファイル(`settings.toml`)を読み、ショートカットの表と知らせにする(計画書 T65b、ADR-012 の 3・5、前提 (f)(g))。
//!
//! 流れ: `FsSettingsFile` で読む(`arin-platform`。TOML の解析もそこ)→ [`SettingsStore`] に当てる → `[shortcuts]` の行を
//! [`ShortcutOverride`] に直して [`apply_overrides`] に通す → 表と知らせ([`Resolved`])。規則は `arin-core` が持ち、
//! ここは型をつなぐだけにする(`settings` と `shortcuts` は互いの型を知らない。変換はここ)。
//!
//! - 起動時は、メニューを作る前に [`AppSettings::load`] で読む(`lib.rs`)。読んだ表を `menu::Shortcuts` に入れる
//! - 動作中の読み直し(T65c)は [`AppSettings::reload`]。ファイル全体の問題(構文エラー・読めない)のときは
//!   `SettingsStore` が直前の内容を保つので、表も直前のままになる(起動時の「直前」は既定)
//! - 知らせは、ファイル全体の問題と行の問題をまとめて 1 行([`Resolved::notice`]。`get_settings_notice` が返す)。
//!   状態ファイルの知らせ(写しの `notice`)とは別に持つ(前提 (g))
//! - 標準エラーには `[arin:settings]` を頭に、問題 1 つにつき 1 行([`Resolved::diagnostics`]。[`report`] が書く)
//!
//! 設定ファイルは読むだけで、書き換え・退避・削除をしない(状態ファイルの退避の仕組み `persistence.rs` に渡さない)。
//! 知らせと標準エラーに出すのは、コマンドの ID・キーの文字列・行と列・理由の種類だけ(中身の全文と絶対パスは出さない。AC-K09)。

use std::sync::{Mutex, PoisonError};

use arin_core::paths::AppPaths;
use arin_core::settings::{SettingValue, Settings, SettingsLoad, SettingsStore};
use arin_core::shortcuts::overrides::{
    OverrideValue, ShortcutOverride, ShortcutSettingProblem, apply_overrides, notice_line,
};
use arin_core::shortcuts::table::ShortcutTable;
use arin_platform::storage::{FsSettingsFile, load_settings_file};

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

/// 設定の管理状態(`Builder::manage` で登録する)。設定ファイルの読み口と、使っている内容・知らせを持つ。
///
/// 表そのものは `menu::Shortcuts` が持つ(メニューと `get_shortcuts` が見る)。ここは知らせ(`get_settings_notice`)と、
/// 読み直し(T65c)のための直前の内容を持つ。
#[derive(Debug)]
pub struct AppSettings {
    file: FsSettingsFile,
    state: Mutex<State>,
}

#[derive(Debug, Default)]
struct State {
    store: SettingsStore,
    notice: Option<String>,
}

impl AppSettings {
    /// 起動時に読む。状態フォルダ(差し替えたら差し替え先)の `settings.toml` だけを読む(AC-K10)。
    /// 無ければ既定の表で、知らせは無い。標準エラーには書かない(呼ぶ側が [`report`] で書く)。
    #[must_use]
    pub fn load(paths: &AppPaths) -> (Self, Resolved) {
        let settings = Self {
            file: FsSettingsFile::new(paths.state_dir().to_path_buf()),
            state: Mutex::new(State::default()),
        };
        let resolved = settings.reload();
        (settings, resolved)
    }

    /// 設定ファイルを読み直して当てる(T65c が保存を検知したときに使う)。ファイル全体の問題のときは
    /// 直前の内容のまま当てる。知らせを結果の知らせに替える。表の差し替え・メニューの作り直し・画面への
    /// イベント(`settings-changed`)・標準エラーへの書き出しは呼ぶ側が行う。
    pub fn reload(&self) -> Resolved {
        let load = load_settings_file(&self.file);
        self.apply(load)
    }

    /// 読んだ結果を当てる。
    fn apply(&self, load: SettingsLoad) -> Resolved {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.store.apply(load);
        let resolved = resolve(&state.store, &ShortcutTable::default_table());
        state.notice.clone_from(&resolved.notice);
        resolved
    }

    /// いまの知らせの 1 行。問題が無ければ `None`。
    #[must_use]
    pub fn notice(&self) -> Option<String> {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .notice
            .clone()
    }
}
