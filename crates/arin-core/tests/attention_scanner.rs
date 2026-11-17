//! アテンションの走査器(`AttentionScanner`)。設計書 §1.3 #5、§7.1「アテンション」の 1、FR-055(FR-045 の走査の前倒し)。
//!
//! 契機になるのは BEL 単独、OSC 9(`9;4` の進捗を除く)、OSC 777 の `notify`。
//! OSC などの文字列の終端としての BEL は契機にしない。チャンクの切れ目がどこにあっても結果は同じ。

use arin_core::terminal::AttentionScanner;

/// 1 つのチャンクとして走査した結果。
fn scan(data: &[u8]) -> bool {
    AttentionScanner::new().scan(data)
}

/// 契機があったと返したチャンクの数を、あらゆる位置で 2 つに切って確かめる。
/// どこで切っても、契機を返すのはちょうど `expected` 個のチャンク(0 か 1)。
fn assert_every_cut(data: &[u8], expected: usize) {
    for cut in 0..=data.len() {
        let mut scanner = AttentionScanner::new();
        let hits = [&data[..cut], &data[cut..]]
            .iter()
            .filter(|chunk| scanner.scan(chunk))
            .count();
        assert_eq!(hits, expected, "{cut} バイト目で切ったとき");
    }
}

/// 1 バイトずつ渡したとき、契機を返したバイトの数。
fn hits_bytewise(data: &[u8]) -> usize {
    let mut scanner = AttentionScanner::new();
    data.iter()
        .filter(|b| scanner.scan(std::slice::from_ref(*b)))
        .count()
}

#[test]
fn bel_単独はアテンションの契機() {
    assert!(scan(b"\x07"));
    assert!(scan(b"done\x07\r\n"));
    // CSI の後ろの BEL も単独の BEL(CSI は BEL で終わらない)。
    assert!(scan(b"\x1b[0m\x07"));
}

#[test]
fn osc_の終端の_bel_は契機にしない() {
    assert!(!scan(b"\x1b]0;title\x07"));
    assert!(!scan(b"\x1b]2;~/work\x07"));
    assert!(!scan(b"\x1b]52;c;QQ==\x07"));
    assert!(!scan(b"\x1b]7;file://host/tmp\x07"));
    // 番号の無い OSC の終端も同じ。
    assert!(!scan(b"\x1b];x\x07"));
    assert!(!scan(b"\x1b]\x07"));
    // 終端のあとの BEL は単独の BEL。
    assert!(scan(b"\x1b]0;title\x07\x07"));
}

#[test]
fn osc_9_は契機() {
    assert!(scan(b"\x1b]9;Build finished\x07"));
    // 本文が空でも OSC 9 の通知。
    assert!(scan(b"\x1b]9;\x07"));
    // 先頭の引数が 4 で始まるだけの文字列は進捗ではない。
    assert!(scan(b"\x1b]9;42 tests passed\x07"));
}

#[test]
fn st_で終わる_osc_9_も契機() {
    assert!(scan(b"\x1b]9;Build finished\x1b\\"));
}

#[test]
fn osc_9_4_の進捗は契機にしない() {
    assert!(!scan(b"\x1b]9;4;1;50\x07"));
    assert!(!scan(b"\x1b]9;4;0;0\x1b\\"));
    assert!(!scan(b"\x1b]9;4\x07"));
}

#[test]
fn osc_777_notify_は契機() {
    assert!(scan(b"\x1b]777;notify;Claude Code;Waiting for input\x07"));
    assert!(scan(b"\x1b]777;notify;title;body\x1b\\"));
    assert!(scan(b"\x1b]777;notify\x07"));
}

#[test]
fn osc_777_のほかの副コマンドは契機にしない() {
    assert!(!scan(b"\x1b]777;preexec\x07"));
    assert!(!scan(b"\x1b]777;precmd\x1b\\"));
    assert!(!scan(b"\x1b]777;notifyx;t;b\x07"));
    assert!(!scan(b"\x1b]777;notif;t;b\x07"));
    assert!(!scan(b"\x1b]777;\x07"));
    assert!(!scan(b"\x1b]777\x07"));
}

#[test]
fn ほかの番号の_osc_は契機にしない() {
    assert!(!scan(b"\x1b]99;i=1;hello\x1b\\"));
    assert!(!scan(b"\x1b]90;x\x07"));
    assert!(!scan(b"\x1b]7770;notify;t\x07"));
    assert!(!scan(b"\x1b]1337;SetMark\x07"));
}

#[test]
fn チャンクの切れ目をまたぐ_osc_9_も_1_回だけ() {
    assert_every_cut(b"ab\x1b]9;Build finished\x07cd", 1);
    assert_every_cut(b"ab\x1b]9;Build finished\x1b\\cd", 1);
    assert_every_cut(b"\x1b]777;notify;t;b\x1b\\", 1);
    assert_every_cut(b"x\x07y", 1);
    assert_every_cut(b"\x1b]9;4;1;50\x07\x1b]0;t\x07\x1b]777;precmd\x1b\\", 0);
    assert_eq!(hits_bytewise(b"ab\x1b]9;Build finished\x07cd"), 1);
    assert_eq!(hits_bytewise(b"\x1b]9;4;1;50\x07\x1b]0;t\x1b\\"), 0);
}

#[test]
fn 切れ目が_esc_と角括弧の間でも同じ() {
    let mut scanner = AttentionScanner::new();
    assert!(!scanner.scan(b"title: \x1b"));
    assert!(!scanner.scan(b"]0;title\x07"));

    let mut scanner = AttentionScanner::new();
    assert!(!scanner.scan(b"\x1b"));
    assert!(scanner.scan(b"]9;done\x07"));

    // ST の ESC と \ の間で切れても、終端は ST。
    let mut scanner = AttentionScanner::new();
    assert!(!scanner.scan(b"\x1b]9;done\x1b"));
    assert!(scanner.scan(b"\\"));
}

#[test]
fn utf_8_の途中で切れても同じ() {
    let data = "ビルド完了\x1b]9;ビルドが終わりました\x07次の行".as_bytes();
    assert_every_cut(data, 1);
    let data = "日本語\x1b]0;タイトル\x07本文".as_bytes();
    assert_every_cut(data, 0);
    let data = "日本語\x07本文".as_bytes();
    assert_every_cut(data, 1);
}

#[test]
fn 一つのチャンクに何度あっても契機があったとだけ返す() {
    let mut scanner = AttentionScanner::new();
    assert!(scanner.scan(b"\x07\x07\x1b]9;a\x07\x1b]777;notify;t;b\x07\x07"));
    // 次のチャンクに契機が無ければ偽。
    assert!(!scanner.scan(b"plain text\r\n"));
}

#[test]
fn 本文が長くても状態の大きさが変わらない() {
    let body = vec![b'A'; 4 * 1024 * 1024];

    let mut long = AttentionScanner::new();
    assert!(!long.scan(b"\x1b]9;"));
    assert!(!long.scan(&body));
    let mut short = AttentionScanner::new();
    assert!(!short.scan(b"\x1b]9;"));
    assert!(!short.scan(b"A"));

    // 本文の長さによらず状態は同じ(本文を溜め込まない)。
    assert_eq!(long, short);
    assert!(std::mem::size_of::<AttentionScanner>() <= 16);
    // そのまま終われば、長い本文の OSC 9 も 1 回の契機。
    assert!(long.scan(b"\x07"));
}

#[test]
fn 終わらない_osc_のあとの_bel_は終端として読む() {
    // 終端の来ない OSC の中では BEL は終端。大量の本文のあとでも状態は一定のまま。
    let mut scanner = AttentionScanner::new();
    assert!(!scanner.scan(b"\x1b]0;"));
    for _ in 0..64 {
        assert!(!scanner.scan(&[b'x'; 64 * 1024]));
    }
    assert!(!scanner.scan(b"\x07"));
    // 終端のあとは本文の外に戻る。
    assert!(scanner.scan(b"\x07"));
}

#[test]
fn can_と_sub_で打ち切った_osc_は契機にしない() {
    assert!(!scan(b"\x1b]9;done\x18"));
    assert!(!scan(b"\x1b]777;notify;t;b\x1a"));
    // 打ち切りのあとの BEL は単独の BEL。
    assert!(scan(b"\x1b]9;done\x18\x07"));
}

#[test]
fn st_以外の_esc_で打ち切った_osc_は契機にしない() {
    // 【仮定】終端の無いまま次の制御列が始まった OSC は通知として扱わない。
    assert!(!scan(b"\x1b]9;done\x1b[0m"));
    // 打ち切りの ESC から次の OSC を読む。
    assert!(scan(b"\x1b]0;t\x1b]9;done\x07"));
}

#[test]
fn dcs_や_apc_の中の_bel_は契機にしない() {
    // DCS・SOS・PM・APC の文字列は ST で終わり、中の BEL は文字列の一部。
    assert!(!scan(b"\x1bPq#0;2;0;0;0\x07-\x1b\\"));
    assert!(!scan(b"\x1bX\x07\x1b\\"));
    assert!(!scan(b"\x1b^\x07\x1b\\"));
    assert!(!scan(b"\x1b_Gf=24;\x07\x1b\\"));
    // 文字列が終われば BEL は単独。
    assert!(scan(b"\x1b_x\x1b\\\x07"));
    // CAN で打ち切ったあとも単独。
    assert!(scan(b"\x1bPabc\x18\x07"));
    assert_every_cut(b"\x1bPq\x07\x1b\\x\x1b]9;y\x07", 1);
}

#[test]
fn 不正な列や未知の列で_panic_しない() {
    // 決まった種から作った疑似乱数のバイト列。制御文字が多く出るように偏らせる。
    let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
    let mut data = Vec::with_capacity(256 * 1024);
    for _ in 0..data.capacity() {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let byte = match seed % 8 {
            0 => 0x1b,
            1 => 0x07,
            2 => b']',
            3 => b';',
            4 => b'9',
            5 => b'\\',
            _ => (seed >> 8) as u8,
        };
        data.push(byte);
    }
    let whole = scan(&data);
    // 切れ目を変えても「どこかに契機があったか」は変わらない。
    for cut in (0..=data.len()).step_by(4099) {
        let mut scanner = AttentionScanner::new();
        let a = scanner.scan(&data[..cut]);
        let b = scanner.scan(&data[cut..]);
        assert_eq!(a || b, whole, "{cut} バイト目で切ったとき");
    }
}

#[test]
fn 既定は制御文字列の外から始める() {
    assert_eq!(AttentionScanner::default(), AttentionScanner::new());
}
