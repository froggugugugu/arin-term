//! 乱数のポート(計画書 M2 の【仮定】1)。
//!
//! プロジェクト ID の元になる 16 バイトを返す。実装は `arin-platform`(T31c。`/dev/urandom` を読む)。
//! 16 バイトから UUID v4 の形への整形は `arin-core`(`ProjectId::from_random_bytes`)が行う。

use thiserror::Error;

/// 乱数を得られなかった。原因は実装側の文言をそのまま持つ。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("乱数を得られなかった: {0}")]
pub struct IdSourceError(pub String);

/// ID の元になる乱数を返す口。
pub trait IdSource: Send + Sync {
    /// 16 バイトの乱数を返す。
    ///
    /// # Errors
    ///
    /// 乱数を読めなかったとき。
    fn random_16(&self) -> Result<[u8; 16], IdSourceError>;
}
