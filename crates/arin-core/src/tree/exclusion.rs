//! 除外の判定(設計書 §5.1 の `ExclusionRule`、§15.2 #1 の A、C-07、FR-017、計画書 M3 の【仮定】3・T38c)。
//!
//! 既定で隠すのは `.git`・`node_modules`・`.DS_Store` の 3 つだけ(名前の完全一致。どの階層でも)。
//! ほかのドットファイル(`.claude`・`.github` など)は既定でも出す。隠しファイル表示を ON にすると 3 つも出す。
//! 監視の変更のまとめ(`changes`)も同じ規則で除外の中の変更を捨てる(PRD C7)。

use crate::paths::RelPath;
use crate::ports::fs::DirEntry;

/// 既定で隠す名前(大文字・小文字も含めて完全に一致するものだけ)。
pub const DEFAULT_HIDDEN_NAMES: [&str; 3] = [".git", "node_modules", ".DS_Store"];

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

    /// フォルダの中身から隠す項目を除く(順は変えない)。
    #[must_use]
    pub fn filter(&self, mut entries: Vec<DirEntry>) -> Vec<DirEntry> {
        entries.retain(|entry| !self.is_hidden_name(&entry.name));
        entries
    }
}
