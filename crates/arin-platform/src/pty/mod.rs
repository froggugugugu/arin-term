//! PTY のポート(`arin_core::ports::PtyPort`)の実装と、ターミナルの起動に要る値の取得(設計書 §7.1、ADR-004)。
//!
//! `portable-pty` の型はこのモジュールの外に出さない。クレートを替えても影響はここに閉じる。

mod native;
mod user;

pub use native::NativePty;
pub use user::{FALLBACK_SHELL, default_shell, system_locale};
