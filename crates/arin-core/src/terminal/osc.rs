//! PTY の生出力から OSC(Operating System Command。`ESC ]` で始まる制御文字列)を見つける走査器
//! (設計書 §1.3 #5、§7.1「ターミナルの入出力」の 2・「アテンション」の 1、§12、§15.2 #9)。
//!
//! 1 バイトずつの状態機械で、チャンクの切れ目をまたいでも同じ結果になる。本文は溜め込まず、
//! 状態の大きさは一定(本文がどれだけ長くてもメモリが増えない)。入出力・時計・スレッドを持たない。
//!
//! いまは T23(検証用の記録)が OSC 52(クリップボード)の回数を数えるのに使う。
//! M4・M6 のアテンション(OSC 9・OSC 777・BEL 単独)はこの走査器の上に足す
//! (`OscEvent` に BEL 単独や先頭の引数を足す。OSC の終端としての BEL は既に区別している)。
//!
//! 扱う形:
//!
//! - 始まり: 7 ビットの `ESC ]`(0x1B 0x5D)だけ。8 ビットの C1(0x9D)は UTF-8 の続きのバイトと区別できないので扱わない
//! - 番号: `ESC ]` の直後の 10 進の数字の並びを、`;` で区切ったもの。数字以外が混じる、数字が無い、`u32` に収まらない
//!   ときは番号の無い OSC とみなす(`Start` を出さない)。`;` の無いまま終わった OSC も番号を出さない
//! - 終わり: BEL(0x07)、ST(`ESC \`)。終端の無いまま `ESC` が来たら打ち切り、その `ESC` を次の制御列の始まりとして読む。
//!   CAN(0x18)・SUB(0x1A)も打ち切り(VT の規則と同じ)

/// クリップボードを操作する OSC の番号(OSC 52)。
pub const OSC_CLIPBOARD: u32 = 52;

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
    /// 本文を読んでいる(中身は捨てる)。
    Body { code: Option<u32> },
    /// 本文の中で `ESC` を読んだ(次が `\` なら ST)。
    BodyEscape { code: Option<u32> },
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
            State::Ground => ground(byte),
            State::Escape => escape(byte),
            State::Number { value, digits } => match byte {
                b'0'..=b'9' => value
                    .checked_mul(10)
                    .and_then(|v| v.checked_add(u32::from(byte - b'0')))
                    .map_or(State::Body { code: None }, |value| State::Number {
                        value,
                        digits: true,
                    }),
                b';' if digits => {
                    on_event(OscEvent::Start { code: value });
                    State::Body { code: Some(value) }
                }
                _ => body(None, byte, on_event),
            },
            State::Body { code } => body(code, byte, on_event),
            State::BodyEscape { code } => {
                if byte == b'\\' {
                    on_event(OscEvent::End {
                        code,
                        terminator: OscTerminator::St,
                    });
                    State::Ground
                } else {
                    on_event(OscEvent::End {
                        code,
                        terminator: OscTerminator::Interrupted,
                    });
                    // 打ち切った ESC を次の制御列の始まりとして読む。
                    escape(byte)
                }
            }
        };
    }
}

/// 制御文字列の外で `byte` を読んだあとの状態。
fn ground(byte: u8) -> State {
    if byte == ESC {
        State::Escape
    } else {
        State::Ground
    }
}

/// `ESC` のあとで `byte` を読んだあとの状態。
fn escape(byte: u8) -> State {
    match byte {
        b']' => State::Number {
            value: 0,
            digits: false,
        },
        ESC => State::Escape,
        _ => State::Ground,
    }
}

/// OSC の本文(または番号として読めなかった部分)で `byte` を読んだあとの状態。
fn body(code: Option<u32>, byte: u8, on_event: &mut impl FnMut(OscEvent)) -> State {
    let terminator = match byte {
        BEL => OscTerminator::Bel,
        CAN | SUB => OscTerminator::Interrupted,
        ESC => return State::BodyEscape { code },
        _ => return State::Body { code },
    };
    on_event(OscEvent::End { code, terminator });
    State::Ground
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
    fn 既定は制御文字列の外から始める() {
        assert_eq!(OscScanner::default(), OscScanner::new());
    }
}
