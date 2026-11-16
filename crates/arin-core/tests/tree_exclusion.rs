//! 作業用の名前(コピーの `.arin-copy-`・保存の `.arin-save-`)の除外(ADR-015 の 5、計画書 T81c)。
//!
//! 一覧は隠しファイル表示が OFF のとき作業用の名前を出さない(ON なら、残ったものを本人が見つけられるよう出す)。
//! 監視はどちらのときも作業用の名前の中の変更を知らせない。

use arin_core::paths::RelPath;
use arin_core::ports::fs::{DirEntry, EntryKind};
use arin_core::tree::exclusion::{ExclusionRule, WORK_NAME_PREFIXES, in_work_path, is_work_name};

fn entry(name: &str) -> DirEntry {
    DirEntry {
        name: name.to_owned(),
        kind: EntryKind::File,
    }
}

fn names(entries: &[DirEntry]) -> Vec<&str> {
    entries.iter().map(|e| e.name.as_str()).collect()
}

#[test]
fn 除外の規則は_arin_save_を隠す() {
    assert_eq!(WORK_NAME_PREFIXES, [".arin-copy-", ".arin-save-"]);
    assert!(is_work_name(".arin-save-123-0"));
    assert!(is_work_name(".arin-copy-123-0"));
    assert!(!is_work_name("arin-save-1"));
    assert!(!is_work_name(".arin-saved"));
    assert!(!is_work_name("README.md"));

    let entries = vec![
        entry("README.md"),
        entry(".arin-save-1-0"),
        entry(".arin-copy-1-0"),
        entry(".git"),
        entry(".claude"),
    ];
    assert_eq!(
        names(&ExclusionRule::new(false).filter_listing(entries.clone())),
        vec!["README.md", ".claude"]
    );
    // 隠しファイル表示が ON なら全部出す(残った作業用の名前も)。
    assert_eq!(
        names(&ExclusionRule::new(true).filter_listing(entries)),
        vec![
            "README.md",
            ".arin-save-1-0",
            ".arin-copy-1-0",
            ".git",
            ".claude"
        ]
    );
}

#[test]
fn 監視は作業用の名前の中の変更をいつも捨てる() {
    let rel = |t: &str| RelPath::parse(t).unwrap();
    assert!(in_work_path(&rel("docs/.arin-save-1-0")));
    assert!(in_work_path(&rel(".arin-copy-1-0/a/b.txt")));
    assert!(!in_work_path(&rel("docs/a.md")));
    assert!(!in_work_path(&RelPath::root()));
}

#[test]
fn 既存の判定は作業用の名前を変えない() {
    // `is_hidden_name`・`filter` は今までどおり既定の 3 つだけ(作業用の名前は `filter_listing`・`in_work_path`)。
    let rule = ExclusionRule::new(false);
    assert!(!rule.is_hidden_name(".arin-save-1-0"));
    assert_eq!(
        names(&rule.filter(vec![entry(".arin-save-1-0")])),
        vec![".arin-save-1-0"]
    );
}
