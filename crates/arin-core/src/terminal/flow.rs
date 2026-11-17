//! 出力の流量制御とまとめの規則(設計書 §1.3 #3、§7.1「ターミナルの入出力」の 3・5、ADR-004、NFR-004)。
//!
//! 読み取りスレッドとまとめの処理(`src-tauri/src/sessions.rs`)はここの規則だけで判断する。時計とスレッドは持たない。
//! しきい値とまとめの単位は仮の値。M1 の確認(T26)で、大量出力のあいだもメモリが増え続けないことを見て決める。

/// 未処理の量の上限(バイト)。これを超えたら PTY の読み取りを止める。仮の値(T26 で決める)。
pub const OUTPUT_HIGH_WATER: u64 = 512 * 1024;
/// 未処理の量の下限(バイト)。止めたあと、ここまで減ったら読み取りを再開する。仮の値(T26 で決める)。
pub const OUTPUT_LOW_WATER: u64 = 128 * 1024;
/// 1 回に画面へ送る量の目安(バイト)。これに達したら時間を待たずに送る。仮の値(T26 で決める)。
pub const OUTPUT_BATCH_MAX_BYTES: usize = 64 * 1024;
/// まとめる時間(ミリ秒)。最初の出力からこの時間ぶんを 1 回にまとめて送る。仮の値(T26 で決める)。
pub const OUTPUT_BATCH_WINDOW_MS: u64 = 4;

/// 未処理の量(PTY から読んだが、画面が取り込み済みと返していない量)と、読み取りを止めるかの判定。
///
/// 上限を超えたら止め、下限まで減ったら再開する(行き来を繰り返さないよう間を空ける)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowControl {
    unacked: u64,
    paused: bool,
    high: u64,
    low: u64,
}

impl FlowControl {
    /// 上限 `high` と下限 `low` を指定して作る。
    ///
    /// # Panics
    ///
    /// 下限が上限以上のとき(再開の条件が止める条件と重なり、止めたまま戻らないか、止まらない)。
    #[must_use]
    pub fn new(high: u64, low: u64) -> Self {
        assert!(low < high, "下限 {low} は上限 {high} より小さくする");
        Self {
            unacked: 0,
            paused: false,
            high,
            low,
        }
    }

    /// PTY から `n` バイト読んだ。上限を超えたら止める。
    pub fn on_read(&mut self, n: usize) {
        let n = u64::try_from(n).unwrap_or(u64::MAX);
        self.unacked = self.unacked.saturating_add(n);
        if self.unacked > self.high {
            self.paused = true;
        }
    }

    /// 画面が `n` バイトを取り込んだ。止めていて下限まで減ったら再開し、`true` を返す(読み取り側を起こす合図)。
    ///
    /// 未処理の量を超える通知は 0 に留める(画面の数え違いで負にしない)。
    pub fn on_acked(&mut self, n: u64) -> bool {
        self.unacked = self.unacked.saturating_sub(n);
        if self.paused && self.unacked <= self.low {
            self.paused = false;
            return true;
        }
        false
    }

    /// 出力の送り先を付け替えた。前の送り先が返すはずだった通知は届かないので、未処理の量を捨てて再開する。
    pub fn reset(&mut self) {
        self.unacked = 0;
        self.paused = false;
    }

    /// 読み取りを止めているか。
    #[must_use]
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// 未処理の量(バイト)。
    #[must_use]
    pub fn unacked(&self) -> u64 {
        self.unacked
    }
}

impl Default for FlowControl {
    /// 既定のしきい値(`OUTPUT_HIGH_WATER`、`OUTPUT_LOW_WATER`)で作る。
    fn default() -> Self {
        Self::new(OUTPUT_HIGH_WATER, OUTPUT_LOW_WATER)
    }
}

/// 画面へ送る前に出力を貯めるところ。届いた順につなぐ。
///
/// 時間で区切るのは呼び出し側(最初の出力から `OUTPUT_BATCH_WINDOW_MS` で送る)。ここは量の判定だけを持つ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputBatch {
    pending: Vec<u8>,
    max_bytes: usize,
}

impl OutputBatch {
    /// `max_bytes` に達したら満杯とみなすまとめを作る。
    #[must_use]
    pub fn new(max_bytes: usize) -> Self {
        Self {
            pending: Vec::new(),
            max_bytes,
        }
    }

    /// 出力を後ろにつなぐ。上限を超える分も切らない(1 回の読み取りを分けて送らない)。
    pub fn push(&mut self, data: &[u8]) {
        self.pending.extend_from_slice(data);
    }

    /// 上限に達したか(時間を待たずに送る合図)。
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.pending.len() >= self.max_bytes
    }

    /// 何も貯まっていないか。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// 貯めた分を取り出し、空に戻す。
    pub fn take(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.pending)
    }
}

impl Default for OutputBatch {
    /// 既定の上限(`OUTPUT_BATCH_MAX_BYTES`)で作る。
    fn default() -> Self {
        Self::new(OUTPUT_BATCH_MAX_BYTES)
    }
}
