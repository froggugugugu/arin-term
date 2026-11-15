//! ツリーの並び順(計画書 M3 の【仮定】2・T38c、設計書 §15.2 #8)。
//!
//! フォルダ(とフォルダへのシンボリックリンク)が先、その中は名前の順。名前は大文字・小文字を区別せずに比べ、
//! 同じならバイトの順で決める(決定的にするため)。数字の自然順は使わない(Finder と違う)。
//! 名前まで同じなら種類の順で決める(同じフォルダには同名は無いが、全順序にしておく)。

use std::cmp::Ordering;

use crate::ports::fs::{DirEntry, EntryKind};

/// フォルダの並びに置く種類か(フォルダとフォルダへのシンボリックリンク)。
#[must_use]
pub fn is_folder_like(kind: EntryKind) -> bool {
    matches!(kind, EntryKind::Directory | EntryKind::DirectoryLink)
}

/// ツリーで展開できる種類か。ふつうのフォルダだけ(リンクはたどらない。ルートの外を操作させないため)。
#[must_use]
pub fn is_expandable(kind: EntryKind) -> bool {
    matches!(kind, EntryKind::Directory)
}

/// 名前が同じときの種類の順。
fn kind_rank(kind: EntryKind) -> u8 {
    match kind {
        EntryKind::Directory => 0,
        EntryKind::DirectoryLink => 1,
        EntryKind::File => 2,
        EntryKind::FileLink => 3,
        EntryKind::Other => 4,
    }
}

/// 大文字・小文字を区別せずに名前を比べる(Unicode の小文字にそろえて 1 文字ずつ)。
fn compare_names_ignoring_case(a: &str, b: &str) -> Ordering {
    a.chars()
        .flat_map(char::to_lowercase)
        .cmp(b.chars().flat_map(char::to_lowercase))
}

/// 2 つの項目の並び順。全順序(同じ入力なら同じ結果)。
#[must_use]
pub fn compare_entries(a: &DirEntry, b: &DirEntry) -> Ordering {
    // フォルダの並びを先にする(`true` が先)。
    is_folder_like(b.kind)
        .cmp(&is_folder_like(a.kind))
        .then_with(|| compare_names_ignoring_case(&a.name, &b.name))
        .then_with(|| a.name.as_bytes().cmp(b.name.as_bytes()))
        .then_with(|| kind_rank(a.kind).cmp(&kind_rank(b.kind)))
}

/// フォルダの中身を並び順に並べ替える。
pub fn sort_entries(entries: &mut [DirEntry]) {
    entries.sort_by(compare_entries);
}
