//! 両方残すの名前(設計書 §5.1 の `UniqueNameGenerator`、FR-030、計画書 M3 の【仮定】11、計画書 T39a)。
//!
//! Finder にならい、拡張子の前に「空白 + 番号」を足す(`a.md` → `a 2.md`、`docs` → `docs 2`、`.env` → `.env 2`)。
//! 番号は 2 から、既存の名前と重ならない最小のもの。
//!
//! 拡張子の分け方は名称変更の選択範囲(【仮定】10、`src/logic/tree/renameSelection.ts`)と同じ:
//! ファイル(とファイルへのリンク・その他)は最後の `.` から。ただし先頭の `.` は拡張子の区切りにしない。
//! フォルダとフォルダへのリンクは全体を名前とみなし、末尾に足す。
//!
//! 既存の名前とは大文字・小文字を区別せずに比べる(APFS は既定で区別しない)。ここで空いて見えても、
//! 置く瞬間に同じ名前ができていれば、上書きしないポートの失敗で衝突になる(手順の側。T39b)。

use super::name::validate_name;
use super::types::InvalidNameReason;
use crate::ports::fs::EntryKind;

/// `name` を両方残すときの、`existing` のどれとも重ならない番号付きの名前を返す。
///
/// `kind` は置く項目の種類(フォルダかどうかで番号を足す位置が変わる)。`existing` は置く先のフォルダの
/// 中の名前(`FileSystemPort::read_dir` の名前など)。返す名前は必ず番号付き(元の名前は返さない)。
/// 元の名前の末尾の番号は読まない(`a 2.md` → `a 2 2.md`)。
///
/// # Errors
///
/// 元の名前が正しくないとき、番号を足すと 255 バイトを超えるときは、その理由。
pub fn keep_both_name<S: AsRef<str>>(
    name: &str,
    kind: EntryKind,
    existing: &[S],
) -> Result<String, InvalidNameReason> {
    validate_name(name)?;
    let (stem, extension) = split_extension(name, kind);
    let taken: Vec<String> = existing.iter().map(|n| fold_case(n.as_ref())).collect();
    // 番号が `existing.len() + 2` までのどれかは必ず空いている。
    let last = existing.len() + 2;
    for number in 2..=last {
        let candidate = format!("{stem} {number}{extension}");
        if !taken.contains(&fold_case(&candidate)) {
            validate_name(&candidate)?;
            return Ok(candidate);
        }
    }
    unreachable!("{last} 個の候補のどれかは空いている")
}

/// 名前を番号を足す前と後(拡張子。`.` を含む)に分ける。
fn split_extension(name: &str, kind: EntryKind) -> (&str, &str) {
    if matches!(kind, EntryKind::Directory | EntryKind::DirectoryLink) {
        return (name, "");
    }
    match name.rfind('.') {
        Some(dot) if dot > 0 => name.split_at(dot),
        _ => (name, ""),
    }
}

/// 大文字・小文字を区別せずに比べるための形。
fn fold_case(name: &str) -> String {
    name.to_lowercase()
}
