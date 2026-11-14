//! PTY のポート(`PtyPort`)の形。
//!
//! 実装は `arin-platform`(T15)が持つ。ここでは偽の実装を通して、ポートの型だけで
//! 起動・書き込み・寸法変更・終了・出力の読み取り・終了の検知ができることを確かめる。

use std::io::{Cursor, Read};
use std::path::Path;
use std::sync::{Arc, Mutex};

use arin_core::ports::{ChildExit, PtyError, PtyPort, PtySession, PtySize, SpawnedPty};
use arin_core::terminal::ShellLaunchSpec;

/// 偽のセッションが受け取った操作の記録。
#[derive(Debug, Default)]
struct Log {
    spawned: Vec<(String, PtySize)>,
    written: Vec<u8>,
    sizes: Vec<PtySize>,
    killed: bool,
}

struct FakePty {
    log: Arc<Mutex<Log>>,
    output: &'static [u8],
}

struct FakeSession {
    log: Arc<Mutex<Log>>,
}

impl PtyPort for FakePty {
    fn spawn(&self, spec: &ShellLaunchSpec, size: PtySize) -> Result<SpawnedPty, PtyError> {
        self.log
            .lock()
            .unwrap()
            .spawned
            .push((spec.argv0().to_string_lossy().into_owned(), size));
        Ok(SpawnedPty {
            session: Box::new(FakeSession {
                log: Arc::clone(&self.log),
            }),
            output: Box::new(Cursor::new(self.output)),
        })
    }
}

impl PtySession for FakeSession {
    fn write(&mut self, data: &[u8]) -> Result<(), PtyError> {
        self.log.lock().unwrap().written.extend_from_slice(data);
        Ok(())
    }

    fn resize(&mut self, size: PtySize) -> Result<(), PtyError> {
        self.log.lock().unwrap().sizes.push(size);
        Ok(())
    }

    fn kill(&mut self) -> Result<(), PtyError> {
        self.log.lock().unwrap().killed = true;
        Ok(())
    }

    fn try_wait(&mut self) -> Result<Option<ChildExit>, PtyError> {
        if self.log.lock().unwrap().killed {
            Ok(Some(ChildExit {
                exit_code: 1,
                signal: Some("Hangup".to_owned()),
            }))
        } else {
            Ok(None)
        }
    }
}

fn spec() -> ShellLaunchSpec {
    ShellLaunchSpec::new(
        Path::new("/bin/zsh"),
        Path::new("/Users/someone"),
        Vec::<(String, String)>::new(),
        None,
    )
    .unwrap()
}

#[test]
fn ポートの型だけで起動から終了の検知までを扱える() {
    let log = Arc::new(Mutex::new(Log::default()));
    // 呼び出し側(T17)は実装を知らず、トレイトオブジェクトとして持つ。
    let port: Box<dyn PtyPort> = Box::new(FakePty {
        log: Arc::clone(&log),
        output: b"% ",
    });
    let size = PtySize { rows: 24, cols: 80 };

    let SpawnedPty {
        mut session,
        mut output,
    } = port.spawn(&spec(), size).expect("起動できるはず");

    // 出力の読み取りは別のスレッドへ渡せる。
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        output.read_to_end(&mut buf).unwrap();
        buf
    });
    assert_eq!(reader.join().unwrap(), b"% ");

    session.write(b"echo ok\r").unwrap();
    session
        .resize(PtySize {
            rows: 40,
            cols: 120,
        })
        .unwrap();
    assert_eq!(session.try_wait().unwrap(), None);
    session.kill().unwrap();
    assert_eq!(
        session.try_wait().unwrap(),
        Some(ChildExit {
            exit_code: 1,
            signal: Some("Hangup".to_owned()),
        })
    );

    let log = log.lock().unwrap();
    assert_eq!(log.spawned, vec![("-zsh".to_owned(), size)]);
    assert_eq!(log.written, b"echo ok\r");
    assert_eq!(
        log.sizes,
        vec![PtySize {
            rows: 40,
            cols: 120
        }]
    );
}

#[test]
fn セッションは別のスレッドへ渡せる() {
    fn assert_send<T: Send>() {}
    assert_send::<Box<dyn PtySession>>();
    assert_send::<SpawnedPty>();

    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Box<dyn PtyPort>>();
}

#[test]
fn 失敗の文言に操作と原因が載る() {
    let cases = [
        (PtyError::Spawn("no such file".to_owned()), "起動"),
        (PtyError::Write("broken pipe".to_owned()), "書き込み"),
        (PtyError::Resize("bad fd".to_owned()), "寸法"),
        (PtyError::Kill("no such process".to_owned()), "終了"),
        (PtyError::Wait("interrupted".to_owned()), "終了の確認"),
    ];

    for (err, operation) in cases {
        let message = err.to_string();
        assert!(message.contains(operation), "{message}");
    }
    assert!(
        PtyError::Spawn("no such file".to_owned())
            .to_string()
            .contains("no such file")
    );
}
