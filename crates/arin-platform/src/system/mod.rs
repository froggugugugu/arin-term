//! 乱数と時計(設計書 §4 の `system/`、計画書 M2 の【仮定】1・3)。標準ライブラリだけで書く。

use std::fs::File;
use std::io::Read;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use arin_core::ports::clock::Clock;
use arin_core::ports::{IdSource, IdSourceError};

/// 乱数を読むデバイス。
const URANDOM: &str = "/dev/urandom";

/// `/dev/urandom` から 16 バイトを読む。呼ぶたびに開いて閉じる(記述子を持ち続けない)。
#[derive(Debug, Clone, Copy, Default)]
pub struct UrandomIdSource;

impl UrandomIdSource {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl IdSource for UrandomIdSource {
    fn random_16(&self) -> Result<[u8; 16], IdSourceError> {
        let mut bytes = [0u8; 16];
        File::open(URANDOM)
            .and_then(|mut f| f.read_exact(&mut bytes))
            .map_err(|e| IdSourceError(format!("{URANDOM}: {e}")))?;
        Ok(bytes)
    }
}

/// `Instant` と `SystemTime` による時計。単調な時刻の起点は作ったとき。
#[derive(Debug, Clone, Copy)]
pub struct SystemClock {
    origin: Instant,
}

impl SystemClock {
    #[must_use]
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn monotonic(&self) -> Duration {
        self.origin.elapsed()
    }

    fn since_unix_epoch(&self) -> Duration {
        // 時計が 1970 年より前に合わされているときは 0(ポートの約束)
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
    }
}
