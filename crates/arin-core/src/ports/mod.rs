//! OS 入出力の抽象(設計書 §5.1、§3.2)。実装は `arin-platform` に置く。
//!
//! いまは PTY のポート(`PtyPort`)だけを置く。他のポートは使うマイルストーンで足す。

mod pty;

pub use pty::{ChildExit, PtyError, PtyPort, PtySession, PtySize, SpawnedPty};
