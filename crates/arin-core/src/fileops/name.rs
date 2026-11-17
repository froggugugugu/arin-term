//! 名前の検査(設計書 §5.1・§7.2 の `NameValidator`、FR-025・FR-026、計画書 T39a)。
//!
//! 新規作成・名称変更で利用者が入れた名前と、両方残すで作った名前を検査する。1 つの名前だけを扱い、
//! 経路(`/` 区切り)は扱わない(経路は `RelPath`)。

use super::types::InvalidNameReason;

/// 名前の長さの上限(UTF-8 のバイト数。APFS の上限)。
pub const MAX_NAME_BYTES: usize = 255;

/// 1 つの名前として正しいかを確かめる。
///
/// 規則: 空でない、NUL を含まない、`/` を含まない、`.`・`..` でない、UTF-8 で 255 バイト以内。
/// それ以外の文字(空白、`:`、`~` で始まる、`.` で始まるなど)は通す。
///
/// # Errors
///
/// 当たった理由を返す。複数に当たるときは 空 → NUL → `/` → `.`・`..` → 長さ の順で先のもの。
pub fn validate_name(name: &str) -> Result<(), InvalidNameReason> {
    if name.is_empty() {
        return Err(InvalidNameReason::Empty);
    }
    if name.contains('\0') {
        return Err(InvalidNameReason::ContainsNul);
    }
    if name.contains('/') {
        return Err(InvalidNameReason::ContainsSlash);
    }
    if name == "." || name == ".." {
        return Err(InvalidNameReason::DotName);
    }
    if name.len() > MAX_NAME_BYTES {
        return Err(InvalidNameReason::TooLong);
    }
    Ok(())
}
