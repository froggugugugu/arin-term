//! ターミナルの規則(設計書 §5.1)。
//!
//! ログインシェルの起動の仕様(`ShellLaunchSpec`)、ターミナルの ID と状態、出力の流量制御とまとめの規則を置く。
//! タブの並び、アテンションの走査、実行中の判定は M1・M2 の後続タスクで足す。

mod flow;
mod launch;
mod state;

pub use flow::{
    FlowControl, OUTPUT_BATCH_MAX_BYTES, OUTPUT_BATCH_WINDOW_MS, OUTPUT_HIGH_WATER,
    OUTPUT_LOW_WATER, OutputBatch,
};
pub use launch::{ShellLaunchError, ShellLaunchSpec};
pub use state::{TerminalError, TerminalId, TerminalSnapshot, TerminalStatus, validate_size};
