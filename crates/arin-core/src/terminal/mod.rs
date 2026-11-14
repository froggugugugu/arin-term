//! ターミナルの規則(設計書 §5.1)。
//!
//! いまはログインシェルの起動の仕様(`ShellLaunchSpec`)だけを置く。タブの並びと状態、アテンションの走査は M1 の後続タスクで足す。

mod launch;

pub use launch::{ShellLaunchError, ShellLaunchSpec};
