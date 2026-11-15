//! 保存をまとめる規則(FR-056、設計書 §6.2「書き込みの契機」、計画書 T30b)。
//!
//! 保存対象の変更から 0.5 秒まとめる。ウィンドウが前面でなくなったときと終了時は待たずに保存する。
//! スレッドもタイマーも持たない。呼び出し側(`src-tauri`)が `Clock::monotonic` の時刻を渡して
//! `poll` を呼び、`Wait` の時間だけ待ってから見直す。
//!
//! 使い方: `Save` を受け取ったら、その時点の状態で保存する内容を作って書き、結果を
//! `save_succeeded` か `save_failed` で知らせる。書いている途中の変更は `mark_changed` で予約し直される。

use std::time::Duration;

/// 変更をまとめる時間。
pub const SAVE_DEBOUNCE: Duration = Duration::from_millis(500);

/// いま保存すべきか。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveDecision {
    /// いま保存する。
    Save,
    /// この時間がたったらもう一度 `poll` で見る。
    Wait(Duration),
    /// 予約が無い。次の変更まで見なくてよい。
    Idle,
}

/// 保存の予約の状態。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pending {
    /// 保存できていない変更が無い。
    Clean,
    /// 予約がある。値はまとめを終える単調な時刻(最初の変更 + `SAVE_DEBOUNCE`)。
    Due(Duration),
    /// 保存に失敗し、保存できていない変更が残る。次の変更か `flush` まで書きに行かない。
    Failed,
}

/// 保存をまとめる規則。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveScheduler {
    enabled: bool,
    pending: Pending,
}

impl Default for SaveScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl SaveScheduler {
    /// 書く設定で作る。
    #[must_use]
    pub fn new() -> Self {
        Self {
            enabled: true,
            pending: Pending::Clean,
        }
    }

    /// 書かない設定で作る(退避に失敗した回)。一度も `Save` を返さない。
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            pending: Pending::Clean,
        }
    }

    /// 書く設定か。
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// 保存できていない変更があるか(書かない設定では常に `false`)。
    #[must_use]
    pub fn has_unsaved_changes(&self) -> bool {
        self.pending != Pending::Clean
    }

    /// 保存対象が変わった。予約が無ければ `now` から 0.5 秒後に予約する。
    /// 予約中の変更はまとめるので、期限を延ばさない。
    pub fn mark_changed(&mut self, now: Duration) {
        if !self.enabled {
            return;
        }
        if !matches!(self.pending, Pending::Due(_)) {
            self.pending = Pending::Due(now + SAVE_DEBOUNCE);
        }
    }

    /// いま保存すべきかを返す。`Save` を返したら予約は消える。
    pub fn poll(&mut self, now: Duration) -> SaveDecision {
        match self.pending {
            Pending::Due(due) if now >= due => {
                self.pending = Pending::Clean;
                SaveDecision::Save
            }
            // 時計が戻っても待つ時間は SAVE_DEBOUNCE を超えない。
            Pending::Due(due) => SaveDecision::Wait((due - now).min(SAVE_DEBOUNCE)),
            Pending::Clean | Pending::Failed => SaveDecision::Idle,
        }
    }

    /// ウィンドウが前面でなくなったとき・終了時。保存できていない変更があれば待たずに `Save` を返す。
    pub fn flush(&mut self) -> SaveDecision {
        if self.has_unsaved_changes() {
            self.pending = Pending::Clean;
            SaveDecision::Save
        } else {
            SaveDecision::Idle
        }
    }

    /// 保存できた。書いている途中に来た変更の予約は残す。
    pub fn save_succeeded(&mut self) {
        // `Save` を返した時点で予約は消している。途中の変更は `Due` のまま残る。
    }

    /// 保存に失敗した。書いている途中に来た変更が無ければ、次の変更でもう一度予約する。
    pub fn save_failed(&mut self) {
        if self.pending == Pending::Clean {
            self.pending = Pending::Failed;
        }
    }
}
