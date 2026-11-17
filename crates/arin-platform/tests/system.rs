//! 乱数と時計の結合テスト(`arin_platform::system`、T31c)。計画書 M2 の【仮定】1・3。

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use arin_core::ports::IdSource;
use arin_core::ports::clock::Clock;
use arin_platform::system::{SystemClock, UrandomIdSource};

#[test]
fn 乱数は_16_バイトで_2_回続けて同じにならない() {
    let source = UrandomIdSource::new();
    let a = source.random_16().expect("読めるはず");
    let b = source.random_16().expect("読めるはず");
    assert_eq!(a.len(), 16);
    assert_ne!(a, b);
    assert_ne!(a, [0u8; 16]);
}

#[test]
fn 時計は_unix_の秒とミリ秒を返す() {
    let clock = SystemClock::new();
    let before = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let got = clock.since_unix_epoch();
    let after = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    assert!(
        before <= got && got <= after,
        "{before:?} <= {got:?} <= {after:?}"
    );
    // 2023-11-14 より後
    assert!(got.as_secs() > 1_700_000_000);
    assert!(got.subsec_millis() < 1000);
}

#[test]
fn 単調な時刻は戻らず経過を測れる() {
    let clock = SystemClock::new();
    let a = clock.monotonic();
    std::thread::sleep(Duration::from_millis(20));
    let b = clock.monotonic();
    assert!(b >= a + Duration::from_millis(20), "{a:?} → {b:?}");
    // 起点は作ったとき。作った直後は小さい
    assert!(a < Duration::from_secs(5));
}
