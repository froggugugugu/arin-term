//! ターミナルの規則(設計書 §5.1)。
//!
//! ログインシェルの起動の仕様(`ShellLaunchSpec`)、ターミナルの ID と状態、出力の流量制御とまとめの規則を置く。
//! OSC の走査器(`OscScanner`)は T23 の検証用の記録が OSC 52 を数えるのに使い、アテンションの走査器(`AttentionScanner`)の土台にする。
//! タブの並びと表示中のタブとアテンションの印(`TerminalStore`)、印を立てるかの判定(`AttentionPolicy`)を置く。実行中の判定は後続のタスクで足す。

mod attention;
mod flow;
mod launch;
mod osc;
mod state;
mod store;

pub use attention::{AttentionPolicy, AttentionScanner};
pub use flow::{
    FlowControl, OUTPUT_BATCH_MAX_BYTES, OUTPUT_BATCH_WINDOW_MS, OUTPUT_HIGH_WATER,
    OUTPUT_LOW_WATER, OutputBatch,
};
pub use launch::{ShellLaunchError, ShellLaunchSpec};
pub use osc::{OSC_CLIPBOARD, OSC_NOTIFY_9, OSC_NOTIFY_777, OscEvent, OscScanner, OscTerminator};
pub use state::{
    MAX_COLS, MAX_ROWS, TerminalError, TerminalId, TerminalSnapshot, TerminalStatus, validate_size,
};
pub use store::{ExitAction, ExitPolicy, TerminalStore, TerminalTab};
