//! 読み取りスレッドのアテンションの走査と写しの送り(計画書 T48c。FR-055、設計書 §7.1「ターミナルの入出力」の 2・
//! 「アテンション」・「スレッドの分担」、ADR-011)。偽の `PtyPort` と偽の出力の送り先で確かめる。
//!
//! - Tauri のアプリは作らないので、`sessions.rs` と同じく標準のテストハーネスで動かす(メニューの部品を作らない)。
//!   写しの送りは、セッション管理に渡す受け取りの関数(本物はイベントで画面へ送る)が呼ばれた回数で数える
//! - どの待ちにも上限時間(`WAIT`)を付ける。待ちは条件が成り立つまでの見回りで、固定の時間に頼らない
//! - 「写しを送らない」の確かめは、契機のチャンクのあとに目印のチャンクを流し、目印が送り先に届くまで待ってから数える。
//!   読み取りスレッドは 1 つのチャンクの走査と印の処理を終えてから次を読むので、目印が届いた時点で前のチャンクの処理は済んでいる

use std::io::{self, Read};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use arin_core::layout::tiles::DisplayMode;
use arin_core::persistence::codec::PersistedState;
use arin_core::ports::{ChildExit, Foreground, PtyError, PtyPort, PtySession, PtySize, SpawnedPty};
use arin_core::project::{Project, ProjectColor, ProjectId, RootReference};
use arin_core::terminal::{ShellLaunchSpec, TerminalId};
use arin_core::workspace::{AppSnapshot, RootResolution, Startup, Workspace};
use arin_lib::sessions::{OutputSink, SessionConfig, SessionRegistry, ShellDefaults};
use arin_lib::verify_log::{VerifyConfig, VerifyRecord, VerifySink};

const WAIT: Duration = Duration::from_secs(10);
const HOME: &str = "/Users/someone";
const ROOT_A: &str = "/Users/someone/dev/alpha";
const ROOT_B: &str = "/Users/someone/dev/beta";

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// 条件が成り立つまで待つ。上限を過ぎたら `false`。
fn wait_until(mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        thread::sleep(Duration::from_millis(1));
    }
    condition()
}

// ---------------------------------------------------------------------------
// 偽の PTY(テストが渡したチャンクを 1 つずつ、その切れ目のまま出す)
// ---------------------------------------------------------------------------

struct Control {
    feed: Mutex<Option<Sender<Vec<u8>>>>,
    /// `true` のあいだ、読み取りはチャンクを受け取ったあと返す前に止まる(読み取りの途中でタブを閉じる確かめに使う)。
    hold_reads: Mutex<bool>,
    release: Condvar,
    /// 止まっている読み取りの数。
    held: AtomicUsize,
    killed: AtomicBool,
    /// 出力(読み取りスレッドが持つ)を捨てたか(読み取りスレッドが抜けたか)。
    output_dropped: AtomicBool,
}

impl Control {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            feed: Mutex::new(None),
            hold_reads: Mutex::new(false),
            release: Condvar::new(),
            held: AtomicUsize::new(0),
            killed: AtomicBool::new(false),
            output_dropped: AtomicBool::new(false),
        })
    }

    fn feed(&self, data: &[u8]) {
        lock(&self.feed)
            .as_ref()
            .expect("まだ起動していないか、出力を閉じた")
            .send(data.to_vec())
            .expect("読み取り側がいない");
    }

    fn set_hold_reads(&self, hold: bool) {
        *lock(&self.hold_reads) = hold;
        self.release.notify_all();
    }
}

/// 起動のたびに、用意した偽の PTY を順に 1 つずつ使う。
struct FakePty {
    controls: Vec<Arc<Control>>,
    next: AtomicUsize,
}

impl PtyPort for FakePty {
    fn spawn(&self, _spec: &ShellLaunchSpec, _size: PtySize) -> Result<SpawnedPty, PtyError> {
        let index = self.next.fetch_add(1, Ordering::SeqCst);
        let control = self
            .controls
            .get(index)
            .ok_or_else(|| PtyError::Spawn("用意した偽の PTY を使い切った".to_owned()))?;
        let (tx, rx) = mpsc::channel();
        *lock(&control.feed) = Some(tx);
        Ok(SpawnedPty {
            session: Box::new(FakeSession(Arc::clone(control))),
            output: Box::new(FakeOutput {
                control: Arc::clone(control),
                rx,
                rest: Vec::new(),
            }),
        })
    }
}

struct FakeOutput {
    control: Arc<Control>,
    rx: Receiver<Vec<u8>>,
    rest: Vec<u8>,
}

impl Read for FakeOutput {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.rest.is_empty() {
            match self.rx.recv() {
                Ok(chunk) => self.rest = chunk,
                Err(_) => return Ok(0),
            }
        }
        let mut hold = lock(&self.control.hold_reads);
        if *hold {
            self.control.held.fetch_add(1, Ordering::SeqCst);
            while *hold {
                hold = self
                    .control
                    .release
                    .wait(hold)
                    .unwrap_or_else(PoisonError::into_inner);
            }
            self.control.held.fetch_sub(1, Ordering::SeqCst);
        }
        drop(hold);
        let n = self.rest.len().min(buf.len());
        buf[..n].copy_from_slice(&self.rest[..n]);
        self.rest.drain(..n);
        Ok(n)
    }
}

impl Drop for FakeOutput {
    fn drop(&mut self) {
        self.control.output_dropped.store(true, Ordering::SeqCst);
    }
}

struct FakeSession(Arc<Control>);

impl PtySession for FakeSession {
    fn write(&mut self, _data: &[u8]) -> Result<(), PtyError> {
        Ok(())
    }

    fn resize(&mut self, _size: PtySize) -> Result<(), PtyError> {
        Ok(())
    }

    fn kill(&mut self) -> Result<(), PtyError> {
        self.0.killed.store(true, Ordering::SeqCst);
        // 本物と同じく、シェルが終わると出力も終わる(受け取り済みのチャンクは読める)。
        lock(&self.0.feed).take();
        Ok(())
    }

    fn try_wait(&mut self) -> Result<Option<ChildExit>, PtyError> {
        Ok(None)
    }

    fn foreground(&mut self) -> Foreground {
        Foreground::Unknown
    }
}

// ---------------------------------------------------------------------------
// 偽の送り先と、検証用の記録の口
// ---------------------------------------------------------------------------

#[derive(Default)]
struct CollectSink(Mutex<Vec<u8>>);

impl CollectSink {
    fn bytes(&self) -> Vec<u8> {
        lock(&self.0).clone()
    }

    fn ends_with(&self, tail: &[u8]) -> bool {
        lock(&self.0).ends_with(tail)
    }
}

impl OutputSink for CollectSink {
    fn send(&self, data: Vec<u8>) -> Result<(), String> {
        lock(&self.0).extend_from_slice(&data);
        Ok(())
    }
}

#[derive(Default)]
struct CollectRecords(Mutex<Vec<VerifyRecord>>);

impl CollectRecords {
    fn clipboard_totals(&self) -> Vec<u64> {
        lock(&self.0)
            .iter()
            .filter_map(|r| match r {
                VerifyRecord::Clipboard { total, .. } => Some(*total),
                VerifyRecord::Flow(_) => None,
            })
            .collect()
    }
}

impl VerifySink for CollectRecords {
    fn record(&self, record: VerifyRecord) {
        lock(&self.0).push(record);
    }
}

// ---------------------------------------------------------------------------
// 2 つのプロジェクト(A が選択中で表示中、B は表示していない)
// ---------------------------------------------------------------------------

fn project_id(n: u8) -> ProjectId {
    ProjectId::from_random_bytes([n; 16])
}

/// プロジェクト A(選択中)と B を 1 枚ずつのタブで持つ、通常の起動の Workspace。
fn two_projects() -> Workspace {
    let projects: Vec<(ProjectId, RootReference)> = [(1, ROOT_A), (2, ROOT_B)]
        .into_iter()
        .map(|(n, root)| (project_id(n), RootReference::new(PathBuf::from(root), None)))
        .collect();
    Workspace::restore(
        "0.0.0-test",
        PathBuf::from(HOME),
        [("HOME", HOME)],
        Startup {
            state: PersistedState {
                projects: projects
                    .iter()
                    .map(|(id, root)| {
                        Project::restore(id.clone(), root.clone(), None, ProjectColor::Green)
                    })
                    .collect(),
                selected_project_id: Some(project_id(1)),
                ..PersistedState::default()
            },
            roots: projects
                .into_iter()
                .map(|(id, root)| (id, RootResolution::Found(root)))
                .collect(),
            notice: None,
        },
    )
}

const A: usize = 0;
const B: usize = 1;

struct Fixture {
    registry: SessionRegistry,
    controls: Vec<Arc<Control>>,
    sinks: Vec<Arc<CollectSink>>,
    ids: Vec<TerminalId>,
    snapshots: Arc<Mutex<Vec<AppSnapshot>>>,
}

impl Fixture {
    fn new() -> Self {
        Self::with_verify(None)
    }

    /// A・B のタブの PTY を起動し、写しの記録を空にしてから返す。
    fn with_verify(verify: Option<VerifyConfig>) -> Self {
        let controls: Vec<Arc<Control>> = (0..2).map(|_| Control::new()).collect();
        let snapshots = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&snapshots);
        let registry = SessionRegistry::with_verify(
            two_projects(),
            Arc::new(FakePty {
                controls: controls.clone(),
                next: AtomicUsize::new(0),
            }),
            Arc::new(|| ShellDefaults {
                shell: PathBuf::from("/bin/zsh"),
                locale: None,
            }),
            Arc::new(move |snapshot| lock(&seen).push(snapshot)),
            SessionConfig {
                exit_poll: Duration::from_millis(10),
                ..SessionConfig::default()
            },
            verify,
        );
        let snapshot = registry.snapshot();
        let ids: Vec<TerminalId> = snapshot
            .projects
            .iter()
            .map(|p| p.terminal_ids[0].clone())
            .collect();
        let sinks: Vec<Arc<CollectSink>> = ids.iter().map(|_| Arc::default()).collect();
        // A を先に起動するので、1 つ目の偽の PTY が A、2 つ目が B になる。
        for (id, sink) in ids.iter().zip(&sinks) {
            registry
                .connect(id, 24, 80, Arc::clone(sink) as Arc<dyn OutputSink>)
                .expect("接続できなかった");
        }
        lock(&snapshots).clear();
        Self {
            registry,
            controls,
            sinks,
            ids,
            snapshots,
        }
    }

    /// `i` 番目のタブに `chunks` を 1 つずつ流し、最後に目印を流して、目印が送り先に届くまで待つ
    /// (届いた時点で、それより前のチャンクの走査と印の処理は済んでいる)。
    fn feed_and_settle(&self, i: usize, chunks: &[&[u8]]) {
        for chunk in chunks {
            self.controls[i].feed(chunk);
        }
        let marker = format!("<settled-{}>", lock(&self.snapshots).len());
        self.controls[i].feed(marker.as_bytes());
        assert!(
            wait_until(|| self.sinks[i].ends_with(marker.as_bytes())),
            "目印が届かなかった: {:?}",
            String::from_utf8_lossy(&self.sinks[i].bytes())
        );
    }

    /// いまの状態での、そのタブのアテンションの印。
    fn attention(&self, i: usize) -> bool {
        attention_in(&self.registry.snapshot(), &self.ids[i])
    }

    fn sent(&self) -> Vec<AppSnapshot> {
        lock(&self.snapshots).clone()
    }

    /// 送った写しのうち、そのタブの印が立っているものの数。
    fn sent_with_attention(&self, i: usize) -> usize {
        self.sent()
            .iter()
            .filter(|s| attention_in(s, &self.ids[i]))
            .count()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // 止めた読み取りが残っていても抜けられるように放してから終わらせる。
        for control in &self.controls {
            control.set_hold_reads(false);
        }
        self.registry.shutdown();
    }
}

fn attention_in(snapshot: &AppSnapshot, id: &TerminalId) -> bool {
    snapshot
        .terminals
        .iter()
        .find(|t| &t.id == id)
        .is_some_and(|t| t.attention)
}

// ---------------------------------------------------------------------------
// テスト
// ---------------------------------------------------------------------------

#[test]
fn 表示していないタブで_bel_が出ると写しにアテンションが載る() {
    let f = Fixture::new();
    assert!(!f.attention(B));

    f.controls[B].feed(b"build done\x07");

    assert!(
        wait_until(|| f.sent_with_attention(B) == 1),
        "印の載った写しが送られなかった: {} 回",
        f.sent().len()
    );
    assert!(f.attention(B));
    assert!(!f.attention(A), "表示中の A に印が立った");
    // 出力はそのまま画面へ届く(走査は出力を変えない)。
    assert!(wait_until(|| f.sinks[B].bytes() == b"build done\x07"));
}

#[test]
fn 表示中のタブの_bel_では写しを送らない() {
    let f = Fixture::new();

    f.feed_and_settle(
        A,
        &[b"\x07", b"\x1b]9;done\x07", b"\x1b]777;notify;t;b\x07"],
    );

    assert!(f.sent().is_empty(), "写しを {} 回送った", f.sent().len());
    assert!(!f.attention(A));
}

#[test]
fn bel_が続いても写しは_1_回だけ送られる() {
    let f = Fixture::new();
    let bells = [b'\x07'; 64];
    let chunks: Vec<&[u8]> = (0..200).map(|_| &bells[..]).collect();

    f.feed_and_settle(B, &chunks);

    assert_eq!(f.sent().len(), 1, "写しの送りが連発した");
    assert_eq!(f.sent_with_attention(B), 1);
    // 出力は欠けずに届く(64 バイト × 200 回と目印)。
    assert_eq!(
        f.sinks[B].bytes().iter().filter(|&&b| b == 0x07).count(),
        64 * 200
    );
}

#[test]
/// 計画書の名前は `osc_9;4_の進捗では写しを送らない`(Rust の名前に `;` を使えないので `_` にした)。
fn osc_9_4_の進捗では写しを送らない() {
    let f = Fixture::new();

    f.feed_and_settle(
        B,
        &[
            b"\x1b]9;4;1;50\x07",
            b"\x1b]9;4;0\x1b\\",
            b"\x1b]0;title\x07",
            b"\x1b]777;other;x\x07",
        ],
    );

    assert!(f.sent().is_empty(), "写しを {} 回送った", f.sent().len());
    assert!(!f.attention(B));
}

#[test]
fn チャンクの切れ目をまたぐ_osc_9_の通知も拾う() {
    let f = Fixture::new();

    // 1 つ目のチャンクだけでは契機にならない。
    f.feed_and_settle(B, &[b"\x1b]9;bui"]);
    assert!(f.sent().is_empty());

    f.controls[B].feed(b"ld done\x1b\\");

    assert!(
        wait_until(|| f.sent_with_attention(B) == 1),
        "切れ目をまたいだ OSC 9 で印が立たなかった"
    );
}

#[test]
fn 表示すると印が消え_また隠れたあとの_bel_でもう一度立つ() {
    let f = Fixture::new();
    f.controls[B].feed(b"\x07");
    assert!(wait_until(|| f.attention(B)));

    // B を選んで表示すると消える(写しで画面へ伝わる)。
    f.registry.select_project(&project_id(2)).unwrap();
    assert!(!f.attention(B));
    assert!(!attention_in(f.sent().last().unwrap(), &f.ids[B]));

    // 表示中のあいだの B のベルでは立たない。
    let before = f.sent().len();
    f.feed_and_settle(B, &[b"\x07"]);
    assert_eq!(f.sent().len(), before);

    // A に戻して B が隠れると、次のベルでもう一度立つ。
    f.registry.select_project(&project_id(1)).unwrap();
    let before = f.sent().len();
    f.controls[B].feed(b"\x07");
    assert!(
        wait_until(|| f.sent().len() == before + 1),
        "もう一度立ったときに写しが送られなかった"
    );
    assert!(f.attention(B));
}

#[test]
fn タイル表示で表示中のページのタイルの_bel_では写しを送らない() {
    let f = Fixture::new();
    assert!(f.registry.set_display_mode(DisplayMode::Tiles));
    let before = f.sent().len();

    // 2 枚とも 1 ページ目に並ぶので、どちらも表示中。
    f.feed_and_settle(B, &[b"\x07"]);
    f.feed_and_settle(A, &[b"\x07"]);

    assert_eq!(f.sent().len(), before);
    assert!(!f.attention(A) && !f.attention(B));

    // 拡大すると、拡大していない B は隠れるので立つ。
    assert!(f.registry.toggle_tile_maximize());
    let before = f.sent().len();
    f.controls[B].feed(b"\x07");
    assert!(wait_until(|| f.sent().len() == before + 1));
    assert!(f.attention(B));
}

#[test]
fn 検証用の記録の_osc_52_の数え方は変わらない() {
    let records = Arc::new(CollectRecords::default());
    let verify = VerifyConfig {
        sink: Arc::clone(&records) as Arc<dyn VerifySink>,
        clock: Arc::new(Instant::now),
        interval: Duration::from_secs(3600),
    };
    let f = Fixture::with_verify(Some(verify));

    // OSC 52 は切れ目をまたぐ。終端の BEL は契機にしない。単独の BEL だけが契機。
    f.feed_and_settle(
        B,
        &[b"\x1b]5", b"2;c;c2VjcmV0\x07", b"\x1b]52;c;c2VjcmV0\x1b\\"],
    );
    assert!(
        wait_until(|| records.clipboard_totals() == vec![1, 2]),
        "回数が届かなかった: {:?}",
        records.clipboard_totals()
    );
    assert!(f.sent().is_empty(), "OSC 52 の終端の BEL を契機にした");

    f.controls[B].feed(b"\x07");
    assert!(wait_until(|| f.sent_with_attention(B) == 1));
    assert_eq!(records.clipboard_totals(), vec![1, 2]);
}

#[test]
fn 検証用の記録が無くても走査する() {
    // `Fixture::new` は検証用の記録を渡さない(リリースビルドと同じ)。
    let f = Fixture::new();
    f.controls[B].feed(b"\x1b]777;notify;title;body\x07");
    assert!(wait_until(|| f.sent_with_attention(B) == 1));
}

#[test]
fn 読み取りの途中でタブを閉じても落ちず写しを送らない() {
    let f = Fixture::new();
    f.controls[B].set_hold_reads(true);
    f.controls[B].feed(b"\x07");
    assert!(
        wait_until(|| f.controls[B].held.load(Ordering::SeqCst) == 1),
        "読み取りが止まらなかった"
    );

    f.registry
        .close_terminal(&f.ids[B])
        .expect("閉じられなかった");
    let after_close = f.sent().len();
    // 止めていた読み取りが、閉じたあとにベルのチャンクを返す。
    f.controls[B].set_hold_reads(false);

    assert!(
        wait_until(|| f.controls[B].output_dropped.load(Ordering::SeqCst)),
        "閉じたタブの読み取りスレッドが抜けない"
    );
    assert_eq!(f.sent().len(), after_close, "閉じたあとに写しを送った");
    // A は続けて動く。
    f.feed_and_settle(A, &[b"still alive"]);
}

#[test]
fn 終了の処理のあとの契機では写しを送らない() {
    let f = Fixture::new();
    f.controls[B].set_hold_reads(true);
    f.controls[B].feed(b"\x07");
    assert!(
        wait_until(|| f.controls[B].held.load(Ordering::SeqCst) == 1),
        "読み取りが止まらなかった"
    );

    f.registry.shutdown();
    let after_shutdown = f.sent().len();
    f.controls[B].set_hold_reads(false);

    assert!(
        wait_until(|| f.controls[B].output_dropped.load(Ordering::SeqCst)),
        "終了の処理のあとに読み取りスレッドが抜けない"
    );
    assert_eq!(
        f.sent().len(),
        after_shutdown,
        "終了の処理のあとに写しを送った"
    );
    // 終了の処理ではタブが残るが、印は立てない。
    assert!(!f.attention(B));
}
