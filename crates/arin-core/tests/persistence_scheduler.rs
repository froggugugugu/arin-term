//! 保存をまとめる規則(FR-056、設計書 §6.2「書き込みの契機」、§10.2)。
//!
//! スケジューラはスレッドもタイマーも持たない。偽の時計の単調な時刻を渡して進める。

use std::sync::Mutex;
use std::time::Duration;

use arin_core::persistence::scheduler::{SAVE_DEBOUNCE, SaveDecision, SaveScheduler};
use arin_core::ports::clock::Clock;

/// 偽の時計。`advance` で進める。
struct FakeClock {
    now: Mutex<Duration>,
}

impl FakeClock {
    fn new() -> Self {
        Self {
            now: Mutex::new(Duration::from_secs(100)),
        }
    }

    fn advance(&self, by: Duration) {
        *self.now.lock().unwrap() += by;
    }
}

impl Clock for FakeClock {
    fn monotonic(&self) -> Duration {
        *self.now.lock().unwrap()
    }

    fn since_unix_epoch(&self) -> Duration {
        Duration::from_secs(1_794_735_000) + self.monotonic()
    }
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

#[test]
fn まとめる時間は_0_5_秒() {
    assert_eq!(SAVE_DEBOUNCE, ms(500));
}

#[test]
fn 時計は単調な時刻と_unix_の時刻を返す() {
    let clock = FakeClock::new();
    let before = clock.monotonic();
    clock.advance(ms(1));

    assert!(clock.monotonic() > before);
    assert!(clock.since_unix_epoch().as_secs() > 1_794_735_000);
}

#[test]
fn 変更が無ければ保存しない() {
    let clock = FakeClock::new();
    let mut scheduler = SaveScheduler::new();

    assert_eq!(scheduler.poll(clock.monotonic()), SaveDecision::Idle);
    clock.advance(ms(10_000));
    assert_eq!(scheduler.poll(clock.monotonic()), SaveDecision::Idle);
    assert_eq!(scheduler.flush(), SaveDecision::Idle);
    assert!(!scheduler.has_unsaved_changes());
}

#[test]
fn 変更から_0_5_秒たつまで保存しない() {
    let clock = FakeClock::new();
    let mut scheduler = SaveScheduler::new();

    scheduler.mark_changed(clock.monotonic());
    assert_eq!(
        scheduler.poll(clock.monotonic()),
        SaveDecision::Wait(ms(500))
    );

    clock.advance(ms(499));
    assert_eq!(scheduler.poll(clock.monotonic()), SaveDecision::Wait(ms(1)));

    clock.advance(ms(1));
    assert_eq!(scheduler.poll(clock.monotonic()), SaveDecision::Save);
    assert_eq!(
        scheduler.poll(clock.monotonic()),
        SaveDecision::Idle,
        "保存を返したら予約は消える"
    );
}

#[test]
fn 期限を過ぎてから見ても_1_回だけ保存する() {
    let clock = FakeClock::new();
    let mut scheduler = SaveScheduler::new();

    scheduler.mark_changed(clock.monotonic());
    clock.advance(ms(5_000));

    assert_eq!(scheduler.poll(clock.monotonic()), SaveDecision::Save);
    assert_eq!(scheduler.poll(clock.monotonic()), SaveDecision::Idle);
}

#[test]
fn 時計が戻っても待つ時間は_0_5_秒を超えない() {
    let mut scheduler = SaveScheduler::new();

    scheduler.mark_changed(ms(1_000));

    assert_eq!(scheduler.poll(ms(900)), SaveDecision::Wait(ms(500)));
}

#[test]
fn 変更が_0_5_秒のあいだに続いても_1_回の保存になる() {
    let clock = FakeClock::new();
    let mut scheduler = SaveScheduler::new();
    let mut saves = 0;

    for _ in 0..4 {
        scheduler.mark_changed(clock.monotonic());
        if scheduler.poll(clock.monotonic()) == SaveDecision::Save {
            saves += 1;
        }
        clock.advance(ms(100));
    }
    // 最初の変更から 0.4 秒。まだ保存しない。
    assert_eq!(saves, 0);
    assert_eq!(
        scheduler.poll(clock.monotonic()),
        SaveDecision::Wait(ms(100))
    );

    clock.advance(ms(100));
    assert_eq!(scheduler.poll(clock.monotonic()), SaveDecision::Save);
    scheduler.save_succeeded();
    clock.advance(ms(10_000));
    assert_eq!(scheduler.poll(clock.monotonic()), SaveDecision::Idle);
}

#[test]
fn 保存の後の変更はあらためて_0_5_秒待つ() {
    let clock = FakeClock::new();
    let mut scheduler = SaveScheduler::new();

    scheduler.mark_changed(clock.monotonic());
    clock.advance(ms(500));
    assert_eq!(scheduler.poll(clock.monotonic()), SaveDecision::Save);
    scheduler.save_succeeded();

    clock.advance(ms(50));
    scheduler.mark_changed(clock.monotonic());
    assert_eq!(
        scheduler.poll(clock.monotonic()),
        SaveDecision::Wait(ms(500))
    );
}

#[test]
fn ウィンドウが前面でなくなったときと終了時は待たずに保存する() {
    let clock = FakeClock::new();
    let mut scheduler = SaveScheduler::new();

    scheduler.mark_changed(clock.monotonic());
    clock.advance(ms(10));
    assert_eq!(scheduler.flush(), SaveDecision::Save);
    scheduler.save_succeeded();
    assert_eq!(
        scheduler.poll(clock.monotonic() + ms(1_000)),
        SaveDecision::Idle,
        "即時に保存したら予約は残らない"
    );

    scheduler.mark_changed(clock.monotonic());
    assert_eq!(scheduler.flush(), SaveDecision::Save);
}

#[test]
fn 保存中に来た変更は保存のあとにもう一度予約する() {
    let clock = FakeClock::new();
    let mut scheduler = SaveScheduler::new();

    scheduler.mark_changed(clock.monotonic());
    clock.advance(ms(500));
    assert_eq!(scheduler.poll(clock.monotonic()), SaveDecision::Save);

    // 書いている途中に次の変更が来る。
    scheduler.mark_changed(clock.monotonic());
    scheduler.save_succeeded();
    assert!(scheduler.has_unsaved_changes());
    assert_eq!(
        scheduler.poll(clock.monotonic()),
        SaveDecision::Wait(ms(500))
    );

    // 失敗でも、途中に来た変更の予約は保つ。
    clock.advance(ms(500));
    assert_eq!(scheduler.poll(clock.monotonic()), SaveDecision::Save);
    scheduler.mark_changed(clock.monotonic());
    scheduler.save_failed();
    assert_eq!(
        scheduler.poll(clock.monotonic()),
        SaveDecision::Wait(ms(500))
    );
}

#[test]
fn 保存に失敗したら次の変更でもう一度予約する() {
    let clock = FakeClock::new();
    let mut scheduler = SaveScheduler::new();

    scheduler.mark_changed(clock.monotonic());
    clock.advance(ms(500));
    assert_eq!(scheduler.poll(clock.monotonic()), SaveDecision::Save);
    scheduler.save_failed();

    // 失敗しただけでは繰り返し書きに行かない。
    clock.advance(ms(10_000));
    assert_eq!(scheduler.poll(clock.monotonic()), SaveDecision::Idle);
    assert!(
        scheduler.has_unsaved_changes(),
        "保存できていない変更は残る"
    );

    scheduler.mark_changed(clock.monotonic());
    assert_eq!(
        scheduler.poll(clock.monotonic()),
        SaveDecision::Wait(ms(500))
    );
    clock.advance(ms(500));
    assert_eq!(scheduler.poll(clock.monotonic()), SaveDecision::Save);
    scheduler.save_succeeded();
    assert!(!scheduler.has_unsaved_changes());
}

#[test]
fn 保存に失敗したあとも前面でなくなったときと終了時には保存する() {
    let clock = FakeClock::new();
    let mut scheduler = SaveScheduler::new();

    scheduler.mark_changed(clock.monotonic());
    assert_eq!(scheduler.flush(), SaveDecision::Save);
    scheduler.save_failed();

    assert_eq!(scheduler.flush(), SaveDecision::Save);
}

#[test]
fn 書かない設定では一度も保存しない() {
    // 退避に失敗した回。
    let clock = FakeClock::new();
    let mut scheduler = SaveScheduler::disabled();
    assert!(!scheduler.is_enabled());

    scheduler.mark_changed(clock.monotonic());
    assert_eq!(scheduler.poll(clock.monotonic()), SaveDecision::Idle);
    clock.advance(ms(10_000));
    assert_eq!(scheduler.poll(clock.monotonic()), SaveDecision::Idle);
    assert_eq!(scheduler.flush(), SaveDecision::Idle);
    assert!(!scheduler.has_unsaved_changes());
}

#[test]
fn 既定は書く設定() {
    let scheduler = SaveScheduler::default();

    assert!(scheduler.is_enabled());
    assert_eq!(scheduler, SaveScheduler::new());
}
