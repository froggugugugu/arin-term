//! M1 の Go/No-Go(T26、人間が実機で判定する)の材料にする検証用の記録(T23。開発ビルドだけ)。
//!
//! 設計書 §2.2 の「ウィンドウが隠れたとき」「クリップボードの制御シーケンス」「流量制御」を見るため、次を
//! 決まった頭の文字列(`VERIFY_PREFIX`)を付けた 1 行で標準エラーに書く。ソケットやファイルの口は作らない(PRD N2)。
//!
//! - `osc52`: PTY の生出力に OSC 52 が現れたとき(本文=クリップボードの中身は載せない)
//! - `flow`: 一定の間隔(`VERIFY_INTERVAL`)ごとの、処理済みの通知の回数・合計・間隔の最大値と、未処理の量の最大値、
//!   読み取りを止めた回数
//!
//! 記録の口は `VerifySink` に差し替えられる。本物(標準エラー)は開発ビルドにだけ入り、リリースビルドの
//! `default_verify` は `None` を返すので、セッション管理は走査も集計もしない。

use std::sync::Arc;
use std::time::{Duration, Instant};

use arin_core::terminal::TerminalId;

/// 記録の行の頭。`[arin:frontend-error]`(起動スモークが失敗とみなす行)とは別にする。
pub const VERIFY_PREFIX: &str = "[arin:verify]";

/// `flow` の行を出す間隔。
pub const VERIFY_INTERVAL: Duration = Duration::from_secs(5);

/// 記録 1 件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyRecord {
    /// 1 回の読み取りの中で OSC 52 が始まった。本文は持たない。
    Clipboard {
        /// ターミナル。
        terminal: TerminalId,
        /// この読み取りで見つけた数。
        seen: u64,
        /// セッションの始まりからの合計。
        total: u64,
    },
    /// 一定の間隔のまとめ。
    Flow(FlowSummary),
}

/// 一定の間隔のあいだの、処理済みの通知と未処理の量のまとめ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowSummary {
    /// ターミナル。
    pub terminal: TerminalId,
    /// セッションの始まりからの経過(ミリ秒)。
    pub at_ms: u64,
    /// このまとめの対象の長さ(ミリ秒)。
    pub window_ms: u64,
    /// 処理済みの通知の回数。
    pub acks: u64,
    /// 処理済みの通知の合計(バイト)。
    pub ack_bytes: u64,
    /// 通知と通知の間隔の最大値(ミリ秒。前の間隔の最後の通知からも測る)。2 回目の通知が無ければ `None`。
    pub max_ack_gap_ms: Option<u64>,
    /// 最後の通知からの経過(ミリ秒)。一度も通知が無ければ `None`。
    pub last_ack_ago_ms: Option<u64>,
    /// PTY から読んだ量(バイト)。
    pub read_bytes: u64,
    /// 未処理の量の最大値(バイト)。
    pub max_unacked: u64,
    /// セッションの始まりからの未処理の量の最大値(バイト)。
    pub peak_unacked: u64,
    /// まとめたときの未処理の量(バイト)。
    pub unacked: u64,
    /// まとめたときに読み取りを止めていたか。
    pub paused: bool,
    /// 読み取りを止めた回数。
    pub pauses: u64,
}

impl VerifyRecord {
    /// 頭の文字列を付けた 1 行(改行なし)。
    #[must_use]
    pub fn line(&self) -> String {
        match self {
            Self::Clipboard {
                terminal,
                seen,
                total,
            } => format!("{VERIFY_PREFIX} osc52 terminal={terminal} seen={seen} total={total}"),
            Self::Flow(s) => format!(
                "{VERIFY_PREFIX} flow terminal={} at_ms={} window_ms={} acks={} ack_bytes={} \
                 max_ack_gap_ms={} last_ack_ago_ms={} read_bytes={} max_unacked={} peak_unacked={} \
                 unacked={} paused={} pauses={}",
                s.terminal,
                s.at_ms,
                s.window_ms,
                s.acks,
                s.ack_bytes,
                optional(s.max_ack_gap_ms),
                optional(s.last_ack_ago_ms),
                s.read_bytes,
                s.max_unacked,
                s.peak_unacked,
                s.unacked,
                s.paused,
                s.pauses,
            ),
        }
    }
}

fn optional(value: Option<u64>) -> String {
    value.map_or_else(|| "-".to_owned(), |v| v.to_string())
}

/// 記録の口。本物は標準エラーに 1 行で書く。テストでは偽物で受ける。
pub trait VerifySink: Send + Sync {
    /// 記録 1 件を受け取る。呼ぶ側はロックを持たない。
    fn record(&self, record: VerifyRecord);
}

/// 時計(テストでは差し替える)。
pub type Clock = Arc<dyn Fn() -> Instant + Send + Sync>;

/// 検証用の記録の設定。これを渡したセッション管理だけが走査と集計をする。
#[derive(Clone)]
pub struct VerifyConfig {
    /// 記録の口。
    pub sink: Arc<dyn VerifySink>,
    /// 時計。
    pub clock: Clock,
    /// `flow` の行を出す間隔。
    pub interval: Duration,
}

/// 組み立てで使う設定。開発ビルドは標準エラーへ書く本物、リリースビルドは `None`(記録しない)。
#[must_use]
pub fn default_verify() -> Option<VerifyConfig> {
    #[cfg(debug_assertions)]
    {
        Some(VerifyConfig {
            sink: Arc::new(StderrSink),
            clock: Arc::new(Instant::now),
            interval: VERIFY_INTERVAL,
        })
    }
    #[cfg(not(debug_assertions))]
    {
        None
    }
}

/// 標準エラーに 1 行で書く記録の口(開発ビルドだけ)。
#[cfg(debug_assertions)]
struct StderrSink;

#[cfg(debug_assertions)]
impl VerifySink for StderrSink {
    fn record(&self, record: VerifyRecord) {
        eprintln!("{}", record.line());
    }
}

/// ターミナル 1 枚の、処理済みの通知と未処理の量の集計。時計は持たず、時刻を引数で受け取る。
#[derive(Debug, Clone)]
pub(crate) struct FlowRecorder {
    terminal: TerminalId,
    started: Instant,
    window_start: Instant,
    last_ack: Option<Instant>,
    acks: u64,
    ack_bytes: u64,
    max_ack_gap: Option<Duration>,
    read_bytes: u64,
    max_unacked: u64,
    peak_unacked: u64,
    pauses: u64,
    was_paused: bool,
}

impl FlowRecorder {
    pub(crate) fn new(terminal: TerminalId, now: Instant) -> Self {
        Self {
            terminal,
            started: now,
            window_start: now,
            last_ack: None,
            acks: 0,
            ack_bytes: 0,
            max_ack_gap: None,
            read_bytes: 0,
            max_unacked: 0,
            peak_unacked: 0,
            pauses: 0,
            was_paused: false,
        }
    }

    /// PTY から `n` バイト読んだ。`unacked`・`paused` は読んだあとの流量制御の状態。
    pub(crate) fn on_read(&mut self, n: usize, unacked: u64, paused: bool) {
        let n = u64::try_from(n).unwrap_or(u64::MAX);
        self.read_bytes = self.read_bytes.saturating_add(n);
        self.observe(unacked, paused);
    }

    /// 画面が `bytes` バイトの処理済みを返した。`unacked`・`paused` は通知のあとの流量制御の状態。
    pub(crate) fn on_ack(&mut self, bytes: u64, now: Instant, unacked: u64, paused: bool) {
        self.acks = self.acks.saturating_add(1);
        self.ack_bytes = self.ack_bytes.saturating_add(bytes);
        if let Some(last) = self.last_ack {
            let gap = now.saturating_duration_since(last);
            self.max_ack_gap = Some(self.max_ack_gap.map_or(gap, |m| m.max(gap)));
        }
        self.last_ack = Some(now);
        self.observe(unacked, paused);
    }

    /// 送り先を付け替えて未処理の量を捨てた。
    pub(crate) fn on_reset(&mut self) {
        self.was_paused = false;
    }

    /// 間隔に達していれば、まとめを返して数え直す。通知も出力も無く止まってもいなければ、まとめは返さない。
    pub(crate) fn tick(
        &mut self,
        now: Instant,
        interval: Duration,
        unacked: u64,
        paused: bool,
    ) -> Option<FlowSummary> {
        let window = now.saturating_duration_since(self.window_start);
        if window < interval {
            return None;
        }
        let active = self.acks > 0 || self.read_bytes > 0 || paused;
        let summary = active.then(|| FlowSummary {
            terminal: self.terminal.clone(),
            at_ms: millis(now.saturating_duration_since(self.started)),
            window_ms: millis(window),
            acks: self.acks,
            ack_bytes: self.ack_bytes,
            max_ack_gap_ms: self.max_ack_gap.map(millis),
            last_ack_ago_ms: self
                .last_ack
                .map(|last| millis(now.saturating_duration_since(last))),
            read_bytes: self.read_bytes,
            max_unacked: self.max_unacked,
            peak_unacked: self.peak_unacked,
            unacked,
            paused,
            pauses: self.pauses,
        });
        self.window_start = now;
        self.acks = 0;
        self.ack_bytes = 0;
        self.max_ack_gap = None;
        self.read_bytes = 0;
        self.pauses = 0;
        self.max_unacked = unacked;
        summary
    }

    fn observe(&mut self, unacked: u64, paused: bool) {
        self.max_unacked = self.max_unacked.max(unacked);
        self.peak_unacked = self.peak_unacked.max(unacked);
        if paused && !self.was_paused {
            self.pauses = self.pauses.saturating_add(1);
        }
        self.was_paused = paused;
    }
}

fn millis(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEC: Duration = Duration::from_secs(1);

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn recorder() -> (FlowRecorder, Instant) {
        let t0 = Instant::now();
        (FlowRecorder::new(TerminalId::new("t-1"), t0), t0)
    }

    #[test]
    fn osc52_の行は回数だけを載せる() {
        let record = VerifyRecord::Clipboard {
            terminal: TerminalId::new("t-1"),
            seen: 1,
            total: 3,
        };
        assert_eq!(
            record.line(),
            "[arin:verify] osc52 terminal=t-1 seen=1 total=3"
        );
    }

    #[test]
    fn flow_の行は_1_行で無い値は横棒にする() {
        let summary = FlowSummary {
            terminal: TerminalId::new("t-1"),
            at_ms: 10_000,
            window_ms: 5_000,
            acks: 0,
            ack_bytes: 0,
            max_ack_gap_ms: None,
            last_ack_ago_ms: Some(7_000),
            read_bytes: 0,
            max_unacked: 524_288,
            peak_unacked: 530_000,
            unacked: 524_288,
            paused: true,
            pauses: 0,
        };
        let line = VerifyRecord::Flow(summary).line();
        assert_eq!(
            line,
            "[arin:verify] flow terminal=t-1 at_ms=10000 window_ms=5000 acks=0 ack_bytes=0 \
             max_ack_gap_ms=- last_ack_ago_ms=7000 read_bytes=0 max_unacked=524288 \
             peak_unacked=530000 unacked=524288 paused=true pauses=0"
        );
        assert!(!line.contains('\n'));
    }

    #[test]
    fn 頭の文字列は起動スモークの失敗の行と別にする() {
        assert_eq!(VERIFY_PREFIX, "[arin:verify]");
        assert_ne!(VERIFY_PREFIX, crate::diagnostics::FRONTEND_ERROR_PREFIX);
    }

    #[test]
    fn 間隔に達するまではまとめない() {
        let (mut r, t0) = recorder();
        r.on_read(10, 10, false);
        assert_eq!(r.tick(t0 + ms(4_999), 5 * SEC, 10, false), None);
        assert!(r.tick(t0 + 5 * SEC, 5 * SEC, 10, false).is_some());
    }

    #[test]
    fn 通知の回数と合計と間隔の最大値をまとめ_次の間隔は数え直す() {
        let (mut r, t0) = recorder();
        r.on_read(1000, 1000, false);
        r.on_ack(100, t0 + ms(10), 900, false);
        r.on_ack(200, t0 + ms(110), 700, false);
        r.on_ack(300, t0 + ms(140), 400, false);

        let s = r.tick(t0 + 5 * SEC, 5 * SEC, 400, false).unwrap();
        assert_eq!(
            (s.acks, s.ack_bytes, s.max_ack_gap_ms, s.last_ack_ago_ms),
            (3, 600, Some(100), Some(4_860))
        );
        assert_eq!((s.read_bytes, s.max_unacked, s.unacked), (1000, 1000, 400));
        assert_eq!((s.at_ms, s.window_ms), (5_000, 5_000));

        // 前の間隔の最後の通知からの間隔も測る。
        r.on_ack(50, t0 + ms(6_140), 350, false);
        let s = r.tick(t0 + 10 * SEC, 5 * SEC, 350, false).unwrap();
        assert_eq!(
            (s.acks, s.ack_bytes, s.max_ack_gap_ms),
            (1, 50, Some(6_000))
        );
        assert_eq!(
            (s.read_bytes, s.max_unacked, s.peak_unacked),
            (0, 400, 1000)
        );
        assert_eq!((s.at_ms, s.window_ms), (10_000, 5_000));
    }

    #[test]
    fn 何も起きず止まってもいなければまとめを出さないが_止まったままなら出す() {
        let (mut r, t0) = recorder();
        assert_eq!(r.tick(t0 + 5 * SEC, 5 * SEC, 0, false), None);

        r.on_read(600, 600, true);
        assert!(r.tick(t0 + 10 * SEC, 5 * SEC, 600, true).is_some());
        // 通知が来ないまま止まっている間も、間隔ごとに出す。
        let s = r.tick(t0 + 15 * SEC, 5 * SEC, 600, true).unwrap();
        assert_eq!((s.acks, s.read_bytes, s.paused, s.pauses), (0, 0, true, 0));
        assert_eq!(s.last_ack_ago_ms, None);
    }

    #[test]
    fn 止めた回数は止まった瞬間だけ数える() {
        let (mut r, t0) = recorder();
        r.on_read(600, 600, true);
        r.on_read(10, 610, true);
        r.on_ack(500, t0 + ms(1), 110, false);
        r.on_read(600, 710, true);

        let s = r.tick(t0 + 5 * SEC, 5 * SEC, 710, true).unwrap();
        assert_eq!(s.pauses, 2);
        assert_eq!(s.max_unacked, 710);

        // 付け替えで捨てたあと、また止まれば数える。
        r.on_reset();
        r.on_read(600, 600, true);
        let s = r.tick(t0 + 10 * SEC, 5 * SEC, 600, true).unwrap();
        assert_eq!(s.pauses, 1);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn 開発ビルドでは標準エラーへ書く記録を使う() {
        let verify = default_verify().expect("開発ビルドなのに記録しない");
        assert_eq!(verify.interval, VERIFY_INTERVAL);
        // 本物の口も 1 行を書けること(中身は読まない)。
        verify.sink.record(VerifyRecord::Clipboard {
            terminal: TerminalId::new("t-test"),
            seen: 1,
            total: 1,
        });
        let _ = (verify.clock)();
    }

    /// `cargo test --release -p arin --lib verify_log` で確かめる。
    #[cfg(not(debug_assertions))]
    #[test]
    fn リリースビルドでは記録しない() {
        assert!(default_verify().is_none());
    }
}
