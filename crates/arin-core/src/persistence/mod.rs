//! 状態の保存と復元の規則(設計書 §6.2)。入出力はしない。
//!
//! `codec` は `state.json` の読み書きの形、`scheduler` は保存をまとめる規則。

pub mod codec;
pub mod scheduler;
