//! 公開された最新版の問い合わせのポート(ADR-018)。実装は `arin-platform/src/release/`。
//!
//! 問い合わせ先は GitHub の Releases の最新版の 1 つだけ([`crate::update::LATEST_RELEASE_API`])。
//! 送るのは問い合わせそのものだけで、利用者の情報は送らない。

use thiserror::Error;

/// 問い合わせの失敗。理由は画面に出す短い文。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{0}")]
pub struct ReleaseFetchError(pub String);

/// 最新版の情報(JSON の本文)を取る口。
pub trait ReleasePort: Send + Sync {
    /// GitHub の Releases の最新版の JSON を返す。
    ///
    /// # Errors
    ///
    /// 通信できない・上限時間を超えた・応答が失敗だったとき。
    fn fetch_latest(&self) -> Result<String, ReleaseFetchError>;
}
