//! 時計のポート(設計書 §10.2。計画書 M2 の【仮定】3)。
//!
//! 実装は `arin-platform`(T31c。`Instant` と `SystemTime`)。テストは偽の時計を渡して進める。

use std::time::Duration;

/// いまの時刻を返す口。
pub trait Clock: Send + Sync {
    /// 単調な時刻。起点は実装が決める(例: 時計を作ったとき)。前の値より戻らない。
    /// 保存のまとめ(`persistence::scheduler`)など、経過時間を測るのに使う。
    fn monotonic(&self) -> Duration;

    /// Unix 時刻(1970-01-01T00:00:00Z からの経過)。秒は `as_secs`、ミリ秒は `subsec_millis` で取る。
    /// 退避の名前の日時(UTC)など、壁時計の時刻が要るところで使う。時計が起点より前なら 0 を返す。
    fn since_unix_epoch(&self) -> Duration;
}
