//! PTY の生出力から OSC(Operating System Command。`ESC ]` で始まる制御文字列)を見つける走査器
//! (設計書 §1.3 #5、§7.1「ターミナルの入出力」の 2・「アテンション」の 1、§12、§15.2 #9)。
//!
//! 1 バイトずつの状態機械で、チャンクの切れ目をまたいでも同じ結果になる。本文は溜め込まず、
//! 状態の大きさは一定(本文がどれだけ長くてもメモリが増えない)。入出力・時計・スレッドを持たない。
//!
//! T23(検証用の記録)が OSC 52(クリップボード)の回数を数えるのに使い、M4 のアテンションの走査器
//! (`AttentionScanner`。`attention.rs`)が BEL 単独と通知の OSC(`Bell`・`Notify`)を拾うのに使う。
//!
//! 扱う形:
//!
//! - 始まり: 7 ビットの `ESC ]`(0x1B 0x5D)だけ。8 ビットの C1(0x9D)は UTF-8 の続きのバイトと区別できないので扱わない
//! - 番号: `ESC ]` の直後の 10 進の数字の並びを、`;` で区切ったもの。数字以外が混じる、数字が無い、`u32` に収まらない
//!   ときは番号の無い OSC とみなす(`Start` を出さない)。`;` の無いまま終わった OSC も番号を出さない
//! - 終わり: BEL(0x07)、ST(`ESC \`)。終端の無いまま `ESC` が来たら打ち切り、その `ESC` を次の制御列の始まりとして読む。
//!   CAN(0x18)・SUB(0x1A)も打ち切り(VT の規則と同じ)
//! - 先頭の引数: OSC 9 と OSC 777 だけ、番号のあとの最初の引数(次の `;` か終端まで)を決まった語と比べる。
//!   OSC 9 は `4`(進捗の報告)なら通知にしない。OSC 777 は `notify` のときだけ通知にする。
//!   比べるのは位置 1 つだけで、本文は持たない。BEL か ST で終わったときだけ `Notify` を出す
//! - BEL 単独: 制御文字列の外(と `ESC` の直後)の BEL は `Bell`。OSC の終端の BEL は `Bell` にしない
//! - DCS・SOS・PM・APC(`ESC P`・`ESC X`・`ESC ^`・`ESC _`): ST で終わる文字列として読み飛ばす。中の BEL は文字列の一部
//!   (xterm.js と同じく、これらは BEL では終わらない)。打ち切りは OSC と同じ

/// クリップボードを操作する OSC の番号(OSC 52)。
pub const OSC_CLIPBOARD: u32 = 52;

/// 通知の OSC の番号(OSC 9。iTerm2 などの通知。`9;4;…` は ConEmu の進捗の報告)。
pub const OSC_NOTIFY_9: u32 = 9;

/// 通知の OSC の番号(OSC 777。rxvt の `777;notify;題;本文`)。
pub const OSC_NOTIFY_777: u32 = 777;

/// OSC 9 の先頭の引数のうち、通知にしないもの(進捗の報告)。
const OSC_9_PROGRESS: &[u8] = b"4";

/// OSC 777 の先頭の引数のうち、通知にするもの。
const OSC_777_NOTIFY: &[u8] = b"notify";

const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;
const CAN: u8 = 0x18;
const SUB: u8 = 0x1a;

/// OSC の終わり方。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OscTerminator {
    /// BEL(0x07)で終わった。
    Bel,
    /// ST(`ESC \`)で終わった。
    St,
    /// 終端の無いまま、`ESC`(`\` 以外が続いた)・CAN・SUB で打ち切られた。
    Interrupted,
}

/// 走査で見つけたもの。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OscEvent {
    /// 番号付きの OSC が始まった(`ESC ] <番号> ;` まで読んだ)。
    Start {
        /// OSC の番号(例 52)。
        code: u32,
    },
    /// OSC が終わった。番号の無い OSC でも出す。
    End {
        /// `Start` を出した OSC ならその番号。番号の無い OSC は `None`。
        code: Option<u32>,
        /// 終わり方。
        terminator: OscTerminator,
    },
    /// 通知の OSC が BEL か ST で終わった(`9;4` 以外の OSC 9、`777;notify` の OSC 777)。
    /// 同じ OSC の `End` の直前に出す。本文は渡さない。
    Notify {
        /// OSC の番号(9 か 777)。
        code: u32,
    },
    /// 制御文字列の外の BEL(OSC などの終端ではない BEL)。
    Bell,
}

/// 先頭の引数を決まった語と比べている途中の結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FirstArg {
    /// 比べない(OSC 9・777 以外、番号の無い OSC)。
    Ignored,
    /// 先頭から `n` バイトが語と一致している(まだ引数の途中)。
    Matching(u8),
    /// 引数が語と同じだった(語のあとに `;` が来た)。
    Equal,
    /// 引数が語と違った。
    Differ,
}

/// 走査の状態。本文は持たない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// 制御文字列の外。
    Ground,
    /// `ESC` を読んだ。
    Escape,
    /// `ESC ]` のあと、番号を読んでいる。`value` は `digits` が真のときだけ意味を持つ。
    Number { value: u32, digits: bool },
    /// 本文を読んでいる(中身は捨てる。先頭の引数を比べた途中の結果だけ持つ)。
    Body { code: Option<u32>, arg: FirstArg },
    /// 本文の中で `ESC` を読んだ(次が `\` なら ST)。
    BodyEscape { code: Option<u32>, arg: FirstArg },
    /// DCS・SOS・PM・APC の文字列を読んでいる(中身は捨てる。BEL では終わらない)。
    Str,
    /// 文字列の中で `ESC` を読んだ(次が `\` なら ST)。
    StrEscape,
}

/// OSC の走査器。ターミナル 1 枚につき 1 個持ち、読んだ順に `feed` へ渡す。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OscScanner {
    state: State,
}

impl Default for OscScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl OscScanner {
    /// 制御文字列の外から始める走査器を作る。
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: State::Ground,
        }
    }

    /// `data` を走査し、見つけたものを順に `on_event` へ渡す。確保はしない。
    pub fn feed(&mut self, data: &[u8], mut on_event: impl FnMut(OscEvent)) {
        for &byte in data {
            self.step(byte, &mut on_event);
        }
    }

    /// `data` の中で始まった OSC 52 の数。`feed` と同じく状態を引き継ぐ。
    pub fn count_clipboard(&mut self, data: &[u8]) -> u64 {
        let mut count = 0;
        self.feed(data, |event| {
            if event
                == (OscEvent::Start {
                    code: OSC_CLIPBOARD,
                })
            {
                count += 1;
            }
        });
        count
    }

    fn step(&mut self, byte: u8, on_event: &mut impl FnMut(OscEvent)) {
        self.state = match self.state {
            State::Ground => ground(byte, on_event),
            State::Escape => escape(byte, on_event),
            State::Number { value, digits } => match byte {
                b'0'..=b'9' => value
                    .checked_mul(10)
                    .and_then(|v| v.checked_add(u32::from(byte - b'0')))
                    .map_or(
                        State::Body {
                            code: None,
                            arg: FirstArg::Ignored,
                        },
                        |value| State::Number {
                            value,
                            digits: true,
                        },
                    ),
                b';' if digits => {
                    on_event(OscEvent::Start { code: value });
                    State::Body {
                        code: Some(value),
                        arg: first_arg_for(value),
                    }
                }
                _ => body(None, FirstArg::Ignored, byte, on_event),
            },
            State::Body { code, arg } => body(code, arg, byte, on_event),
            State::BodyEscape { code, arg } => {
                if byte == b'\\' {
                    end(code, arg, OscTerminator::St, on_event);
                    State::Ground
                } else {
                    end(code, arg, OscTerminator::Interrupted, on_event);
                    // 打ち切った ESC を次の制御列の始まりとして読む。
                    escape(byte, on_event)
                }
            }
            State::Str => match byte {
                CAN | SUB => State::Ground,
                ESC => State::StrEscape,
                _ => State::Str,
            },
            State::StrEscape => {
                if byte == b'\\' {
                    State::Ground
                } else {
                    // ST でない ESC は文字列の打ち切り。その ESC を次の制御列の始まりとして読む。
                    escape(byte, on_event)
                }
            }
        };
    }
}

/// 制御文字列の外で `byte` を読んだあとの状態。
fn ground(byte: u8, on_event: &mut impl FnMut(OscEvent)) -> State {
    match byte {
        ESC => State::Escape,
        BEL => {
            on_event(OscEvent::Bell);
            State::Ground
        }
        _ => State::Ground,
    }
}

/// `ESC` のあとで `byte` を読んだあとの状態。
fn escape(byte: u8, on_event: &mut impl FnMut(OscEvent)) -> State {
    match byte {
        b']' => State::Number {
            value: 0,
            digits: false,
        },
        b'P' | b'X' | b'^' | b'_' => State::Str,
        ESC => State::Escape,
        // エスケープ列の途中の C0 は実行し、列は続く(VT の規則と同じ)。
        BEL => {
            on_event(OscEvent::Bell);
            State::Escape
        }
        _ => State::Ground,
    }
}

/// OSC の本文(または番号として読めなかった部分)で `byte` を読んだあとの状態。
fn body(code: Option<u32>, arg: FirstArg, byte: u8, on_event: &mut impl FnMut(OscEvent)) -> State {
    let terminator = match byte {
        BEL => OscTerminator::Bel,
        CAN | SUB => OscTerminator::Interrupted,
        ESC => return State::BodyEscape { code, arg },
        _ => {
            return State::Body {
                code,
                arg: advance(code, arg, byte),
            };
        }
    };
    end(code, arg, terminator, on_event);
    State::Ground
}

/// OSC の終わりを渡す。通知の OSC が BEL か ST で終わったときは、その前に `Notify` を渡す。
fn end(
    code: Option<u32>,
    arg: FirstArg,
    terminator: OscTerminator,
    on_event: &mut impl FnMut(OscEvent),
) {
    if terminator != OscTerminator::Interrupted
        && let Some(code) = code
    {
        let notify = match code {
            OSC_NOTIFY_9 => !first_arg_equal(code, arg),
            OSC_NOTIFY_777 => first_arg_equal(code, arg),
            _ => false,
        };
        if notify {
            on_event(OscEvent::Notify { code });
        }
    }
    on_event(OscEvent::End { code, terminator });
}

/// 先頭の引数と比べる語。比べない番号は `None`。
fn first_arg_word(code: u32) -> Option<&'static [u8]> {
    match code {
        OSC_NOTIFY_9 => Some(OSC_9_PROGRESS),
        OSC_NOTIFY_777 => Some(OSC_777_NOTIFY),
        _ => None,
    }
}

/// 番号を読み終えたときの、先頭の引数の比べ始めの状態。
fn first_arg_for(code: u32) -> FirstArg {
    if first_arg_word(code).is_some() {
        FirstArg::Matching(0)
    } else {
        FirstArg::Ignored
    }
}

/// 本文の 1 バイト(終端・打ち切りでないもの)を読んだあとの、先頭の引数の比べ途中の状態。
fn advance(code: Option<u32>, arg: FirstArg, byte: u8) -> FirstArg {
    let FirstArg::Matching(n) = arg else {
        return arg;
    };
    let Some(word) = code.and_then(first_arg_word) else {
        return FirstArg::Ignored;
    };
    match word.get(usize::from(n)) {
        // 語を読み終えたあと: `;` なら引数が語と同じ。
        None if byte == b';' => FirstArg::Equal,
        None => FirstArg::Differ,
        Some(&expected) if expected == byte => FirstArg::Matching(n + 1),
        Some(_) => FirstArg::Differ,
    }
}

/// 先頭の引数が語と同じだったか(語のあとで `;` か終端が来た)。
fn first_arg_equal(code: u32, arg: FirstArg) -> bool {
    match arg {
        FirstArg::Equal => true,
        FirstArg::Matching(n) => first_arg_word(code).is_some_and(|w| w.len() == usize::from(n)),
        FirstArg::Ignored | FirstArg::Differ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const START: &[u8] = b"\x1b]52;";

    fn events(scanner: &mut OscScanner, data: &[u8]) -> Vec<OscEvent> {
        let mut out = Vec::new();
        scanner.feed(data, |e| out.push(e));
        out
    }

    fn count(data: &[u8]) -> u64 {
        OscScanner::new().count_clipboard(data)
    }

    /// 1 バイトずつ渡したときの回数。
    fn count_bytewise(data: &[u8]) -> u64 {
        let mut scanner = OscScanner::new();
        data.iter()
            .map(|b| scanner.count_clipboard(std::slice::from_ref(b)))
            .sum()
    }

    #[test]
    fn osc_52_の始まりを見つける() {
        assert_eq!(count(b"hello\x1b]52;c;aGVsbG8=\x07world"), 1);
    }

    #[test]
    fn 番号と終わり方を出来事として渡す() {
        let mut scanner = OscScanner::new();

        assert_eq!(
            events(&mut scanner, b"\x1b]52;c;QQ==\x07"),
            vec![
                OscEvent::Start { code: 52 },
                OscEvent::End {
                    code: Some(52),
                    terminator: OscTerminator::Bel
                },
            ]
        );
        assert_eq!(
            events(&mut scanner, b"\x1b]0;title\x1b\\"),
            vec![
                OscEvent::Start { code: 0 },
                OscEvent::End {
                    code: Some(0),
                    terminator: OscTerminator::St
                },
            ]
        );
    }

    #[test]
    fn チャンクの切れ目をまたいでも見つける() {
        let data = b"ab\x1b]52;c;QUJD\x07cd\x1b]52;p;\x1b\\ef";
        for cut in 0..=data.len() {
            let mut scanner = OscScanner::new();
            let n = scanner.count_clipboard(&data[..cut]) + scanner.count_clipboard(&data[cut..]);
            assert_eq!(n, 2, "{cut} バイト目で切ったとき");
        }
        assert_eq!(count_bytewise(data), 2);
    }

    #[test]
    fn 一バイトずつ渡しても出来事の列は同じ() {
        let data = b"\x1b]9;hi\x07x\x1b]52;c;\x1b]777;notify;a;b\x1b\\\x1b]x;y\x07";
        let whole = events(&mut OscScanner::new(), data);
        let mut scanner = OscScanner::new();
        let mut bytewise = Vec::new();
        for b in data {
            scanner.feed(std::slice::from_ref(b), |e| bytewise.push(e));
        }
        assert_eq!(whole, bytewise);
    }

    #[test]
    fn osc_9_や_777_は数えない() {
        assert_eq!(count(b"\x1b]9;done\x07\x1b]777;notify;t;b\x1b\\"), 0);
    }

    #[test]
    fn 番号が_52_でなければ数えない() {
        // 間に文字、3 桁、数字の前の文字、番号の前の空白。
        assert_eq!(count(b"\x1b]5 2;x\x07"), 0);
        assert_eq!(count(b"\x1b]520;x\x07"), 0);
        assert_eq!(count(b"\x1b]5;x\x07"), 0);
        assert_eq!(count(b"\x1b] 52;x\x07"), 0);
        assert_eq!(count(b"\x1b]x52;x\x07"), 0);
    }

    #[test]
    fn esc_の無い_52_や_esc_と_の間に文字があれば数えない() {
        assert_eq!(count(b"]52;x\x07"), 0);
        assert_eq!(count(b"\x1bx]52;x\x07"), 0);
        // CSI の中の ']' は OSC の始まりではない。
        assert_eq!(count(b"\x1b[]52;x\x07"), 0);
    }

    #[test]
    fn 区切りの無いまま終わった_osc_は番号を出さない() {
        let mut scanner = OscScanner::new();
        assert_eq!(
            events(&mut scanner, b"\x1b]52\x07"),
            vec![OscEvent::End {
                code: None,
                terminator: OscTerminator::Bel
            }]
        );
        assert_eq!(
            events(&mut scanner, b"\x1b];x\x1b\\"),
            vec![OscEvent::End {
                code: None,
                terminator: OscTerminator::St
            }]
        );
    }

    #[test]
    fn u32_に収まらない番号は番号の無い_osc_にする() {
        let mut scanner = OscScanner::new();
        assert_eq!(
            events(&mut scanner, b"\x1b]99999999999999999952;x\x07"),
            vec![OscEvent::End {
                code: None,
                terminator: OscTerminator::Bel
            }]
        );
    }

    #[test]
    fn bel_終端と_st_終端のあとは本文の外に戻る() {
        // 終端のあとの "]52;" は OSC ではない。
        assert_eq!(count(b"\x1b]52;a\x07]52;\x1b]0;t\x1b\\]52;"), 1);
        // ST の ESC のあとに続けて OSC 52 が来ても見つける。
        assert_eq!(count(b"\x1b]0;t\x1b\\\x1b]52;a\x07"), 1);
    }

    #[test]
    fn 本文の中の_esc_は_st_でなければ打ち切って次の制御列として読む() {
        let mut scanner = OscScanner::new();
        assert_eq!(
            events(&mut scanner, b"\x1b]52;abc\x1b]52;def\x07"),
            vec![
                OscEvent::Start { code: 52 },
                OscEvent::End {
                    code: Some(52),
                    terminator: OscTerminator::Interrupted
                },
                OscEvent::Start { code: 52 },
                OscEvent::End {
                    code: Some(52),
                    terminator: OscTerminator::Bel
                },
            ]
        );
        // 打ち切りの ESC のあとが CSI なら、その後の文字は OSC にならない。
        assert_eq!(count(b"\x1b]52;abc\x1b[0m]52;"), 1);
        // ESC が 2 つ続いても、2 つ目から読み直す。
        assert_eq!(count(b"\x1b]0;a\x1b\x1b]52;b\x07"), 1);
    }

    #[test]
    fn 番号を読んでいる途中の_esc_も打ち切り() {
        let mut scanner = OscScanner::new();
        assert_eq!(
            events(&mut scanner, b"\x1b]5\x1b]52;x\x07"),
            vec![
                OscEvent::End {
                    code: None,
                    terminator: OscTerminator::Interrupted
                },
                OscEvent::Start { code: 52 },
                OscEvent::End {
                    code: Some(52),
                    terminator: OscTerminator::Bel
                },
            ]
        );
    }

    #[test]
    fn can_と_sub_は打ち切り() {
        let mut scanner = OscScanner::new();
        assert_eq!(
            events(&mut scanner, b"\x1b]52;a\x18\x1b]5\x1a"),
            vec![
                OscEvent::Start { code: 52 },
                OscEvent::End {
                    code: Some(52),
                    terminator: OscTerminator::Interrupted
                },
                OscEvent::End {
                    code: None,
                    terminator: OscTerminator::Interrupted
                },
            ]
        );
        // 打ち切りのあとの "]52;" は OSC ではない。
        assert_eq!(count(b"\x1b]52;a\x18]52;"), 1);
    }

    #[test]
    fn 二回続けて現れたら二回数える() {
        assert_eq!(count(b"\x1b]52;c;QQ==\x07\x1b]52;c;Qg==\x07"), 2);
        assert_eq!(count(b"\x1b]52;c;QQ==\x1b\\\x1b]52;c;Qg==\x1b\\"), 2);
        assert_eq!(count_bytewise(b"\x1b]52;c;QQ==\x07\x1b]52;c;Qg==\x07"), 2);
    }

    #[test]
    fn 先頭の_0_は数として読む() {
        assert_eq!(count(b"\x1b]052;x\x07"), 1);
    }

    #[test]
    fn 大きな本文でも状態の大きさは一定() {
        let body = vec![b'A'; 4 * 1024 * 1024];

        let mut long = OscScanner::new();
        assert_eq!(long.count_clipboard(START), 1);
        assert_eq!(long.count_clipboard(&body), 0);
        let mut short = OscScanner::new();
        assert_eq!(short.count_clipboard(START), 1);
        assert_eq!(short.count_clipboard(b"A"), 0);

        // 本文の長さによらず、状態は同じ(本文を溜め込まない)。
        assert_eq!(long, short);
        assert!(std::mem::size_of::<OscScanner>() <= 16);
        // そのまま終われば 1 回のまま。
        assert_eq!(long.count_clipboard(b"\x07"), 0);
    }

    #[test]
    fn 制御文字列の外の_bel_は_bell_で終端の_bel_は_end() {
        let mut scanner = OscScanner::new();
        assert_eq!(
            events(&mut scanner, b"a\x07\x1b]0;t\x07"),
            vec![
                OscEvent::Bell,
                OscEvent::Start { code: 0 },
                OscEvent::End {
                    code: Some(0),
                    terminator: OscTerminator::Bel
                },
            ]
        );
        // ESC の直後の BEL も実行される BEL。ESC はそのまま続く。
        assert_eq!(
            events(&mut scanner, b"\x1b\x07]9;x\x07"),
            vec![
                OscEvent::Bell,
                OscEvent::Start { code: 9 },
                OscEvent::Notify { code: 9 },
                OscEvent::End {
                    code: Some(9),
                    terminator: OscTerminator::Bel
                },
            ]
        );
    }

    #[test]
    fn 通知の_osc_は終わりの直前に_notify_を出す() {
        let mut scanner = OscScanner::new();
        assert_eq!(
            events(&mut scanner, b"\x1b]777;notify;t;b\x1b\\"),
            vec![
                OscEvent::Start { code: 777 },
                OscEvent::Notify { code: 777 },
                OscEvent::End {
                    code: Some(777),
                    terminator: OscTerminator::St
                },
            ]
        );
    }

    #[test]
    fn 先頭の引数で通知かを決める() {
        let notified = |data: &[u8]| {
            let mut n = 0;
            OscScanner::new().feed(data, |e| {
                if matches!(e, OscEvent::Notify { .. }) {
                    n += 1;
                }
            });
            n
        };
        assert_eq!(notified(b"\x1b]9;4;1;50\x07"), 0);
        assert_eq!(notified(b"\x1b]9;4\x07"), 0);
        assert_eq!(notified(b"\x1b]9;4x\x07"), 1);
        assert_eq!(notified(b"\x1b]9;44;1\x07"), 1);
        assert_eq!(notified(b"\x1b]9;\x07"), 1);
        assert_eq!(notified(b"\x1b]9;;4\x07"), 1);
        assert_eq!(notified(b"\x1b]777;notify\x07"), 1);
        assert_eq!(notified(b"\x1b]777;notify;\x07"), 1);
        assert_eq!(notified(b"\x1b]777;notif\x07"), 0);
        assert_eq!(notified(b"\x1b]777;notifyy\x07"), 0);
        assert_eq!(notified(b"\x1b]777;x;notify\x07"), 0);
        // 打ち切られた OSC は通知にしない。
        assert_eq!(notified(b"\x1b]9;x\x18"), 0);
        assert_eq!(notified(b"\x1b]9;x\x1b[m"), 0);
    }

    #[test]
    fn dcs_などの文字列の中の_bel_は出来事にしない() {
        let mut scanner = OscScanner::new();
        assert_eq!(events(&mut scanner, b"\x1bPq\x07\x1b\\"), vec![]);
        assert_eq!(
            events(&mut scanner, b"\x1b_a\x07b\x1a\x07"),
            vec![OscEvent::Bell]
        );
        // 文字列の中の ST でない ESC は打ち切りで、続く OSC を読む。
        assert_eq!(count(b"\x1bPtmux;\x1b\x1b]52;c;QQ==\x07\x1b\\"), 1);
    }

    #[test]
    fn 既定は制御文字列の外から始める() {
        assert_eq!(OscScanner::default(), OscScanner::new());
    }
}
