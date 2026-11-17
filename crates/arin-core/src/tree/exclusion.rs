//! 除外の判定(設計書 §5.1 の `ExclusionRule`、§15.2 #1 の A、C-07、FR-017、計画書 M3 の【仮定】3・T38c)。
//!
//! 既定で隠すのは `.git`・`node_modules`・`.DS_Store` の 3 つだけ(名前の完全一致。どの階層でも)。
//! ほかのドットファイル(`.claude`・`.github` など)は既定でも出す。隠しファイル表示を ON にすると 3 つも出す。
//! 監視の変更のまとめ(`changes`)も同じ規則で除外の中の変更を捨てる(PRD C7)。
//!
//! アプリが一時的に作る作業用の名前(コピーの `.arin-copy-`、保存の `.arin-save-`。ADR-015 の 5)は別に扱う:
//! 一覧(`filter_listing`)は隠しファイル表示が OFF のとき出さず(ON なら、片付けられずに残ったものを本人が見つけられるよう出す)、
//! 監視(`in_work_path`)はどちらのときも知らせない。

use crate::paths::RelPath;
use crate::ports::fs::DirEntry;

/// 既定で隠す名前(大文字・小文字も含めて完全に一致するものだけ)。
pub const DEFAULT_HIDDEN_NAMES: [&str; 3] = [".git", "node_modules", ".DS_Store"];

/// アプリが一時的に作る作業用の名前の頭(コピーの作業用フォルダ、保存の一時ファイル)。
/// `arin-platform` の `STAGING_PREFIX`・`SAVE_PREFIX` と同じ(`crates/arin-platform/tests/document.rs` が確かめる)。
pub const WORK_NAME_PREFIXES: [&str; 2] = [".arin-copy-", ".arin-save-"];

/// 作業用の名前(`WORK_NAME_PREFIXES` のどれかで始まる)か。
#[must_use]
pub fn is_work_name(name: &str) -> bool {
    WORK_NAME_PREFIXES
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

/// どれかの要素が作業用の名前か(監視はこの中の変更を隠しファイル表示によらず知らせない)。ルートは違う。
#[must_use]
pub fn in_work_path(path: &RelPath) -> bool {
    path.components().any(is_work_name)
}

/// 除外の規則。プロジェクトごとの隠しファイル表示(`projects[].tree.showHidden`)から作る。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ExclusionRule {
    show_hidden: bool,
}

impl ExclusionRule {
    /// 隠しファイル表示の値から作る(既定は `false`)。
    #[must_use]
    pub fn new(show_hidden: bool) -> Self {
        Self { show_hidden }
    }

    /// 隠しファイル表示が ON か。
    #[must_use]
    pub fn show_hidden(&self) -> bool {
        self.show_hidden
    }

    /// この名前の項目を隠すか。
    #[must_use]
    pub fn is_hidden_name(&self, name: &str) -> bool {
        !self.show_hidden && DEFAULT_HIDDEN_NAMES.contains(&name)
    }

    /// この相対パスが隠す項目か、その中にあるか(どれかの要素が隠す名前なら隠す)。ルートは隠さない。
    #[must_use]
    pub fn is_hidden_path(&self, path: &RelPath) -> bool {
        path.components().any(|name| self.is_hidden_name(name))
    }

    /// フォルダの中身から隠す項目を除く(順は変えない)。作業用の名前は見ない(一覧には `filter_listing`)。
    #[must_use]
    pub fn filter(&self, mut entries: Vec<DirEntry>) -> Vec<DirEntry> {
        entries.retain(|entry| !self.is_hidden_name(&entry.name));
        entries
    }

    /// ツリーの一覧に出す項目だけを残す(順は変えない)。隠す名前に加え、隠しファイル表示が OFF なら作業用の名前も除く。
    #[must_use]
    pub fn filter_listing(&self, mut entries: Vec<DirEntry>) -> Vec<DirEntry> {
        entries.retain(|entry| {
            !self.is_hidden_name(&entry.name) && (self.show_hidden || !is_work_name(&entry.name))
        });
        entries
    }
}
