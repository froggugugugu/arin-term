//! Arin の状態・規則・ポート定義。
//!
//! このクレートは OS への入出力(ファイル、プロセス、ネットワーク)を持たない。
//! 入出力はポート(trait)として定義し、実装は `arin-platform` に置く。

#![forbid(unsafe_code)]

pub mod workspace;

/// このクレートの版。
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
