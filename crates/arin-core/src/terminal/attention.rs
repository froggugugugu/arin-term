//! アテンションの走査器(設計書 §1.3 #5、§7.1「アテンション」の 1、FR-045・FR-055)。
//!
//! PTY の生出力を、読み取りスレッドが受け取ったチャンクの順に渡す。チャンクの中に
//! アテンションの契機があったかだけを返す(本文は保持しない。OSC 9・777 の本文を使うのは M6)。
//!
//! 契機にするもの:
//!
//! - BEL 単独(制御文字列の外の BEL)
//! - OSC 9 のうち、先頭の引数が `4`(進捗の報告)でないもの
//! - OSC 777 のうち、先頭の引数が `notify` のもの
//!
//! 契機にしないもの: OSC などの文字列の終端としての BEL、DCS・SOS・PM・APC の中の BEL、OSC 9;4、
//! OSC 777 のほかの副コマンド、ほかの番号の OSC、CAN・SUB・ST でない `ESC` で打ち切られた OSC。
//!
//! 走査は `OscScanner` の 1 バイトずつの状態機械に任せる。チャンクの切れ目がどこにあっても同じ結果になり、
//! 状態の大きさは一定(終わらない OSC が続いてもメモリは増えない)。確保をしない。

use super::osc::{OscEvent, OscScanner};

/// アテンションの走査器。ターミナル 1 枚につき 1 個、読み取りスレッドだけが持つ。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AttentionScanner {
    osc: OscScanner,
}

impl AttentionScanner {
    /// 制御文字列の外から始める走査器を作る。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `data` を走査し、このチャンクの中でアテンションの契機があったかを返す。
    /// 何度あっても `true` を 1 回返すだけ。状態は次のチャンクへ引き継ぐ。
    pub fn scan(&mut self, data: &[u8]) -> bool {
        let mut seen = false;
        self.osc.feed(data, |event| {
            if matches!(event, OscEvent::Bell | OscEvent::Notify { .. }) {
                seen = true;
            }
        });
        seen
    }
}
