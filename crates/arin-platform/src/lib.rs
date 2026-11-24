//! `arin-core` が定義するポートの実装。
//!
//! ファイル操作、ファイル監視、PTY、ブックマーク、ルートフォルダの現在位置、ペーストボード、乱数、時計など、OS への入出力をここに集める。

pub mod bookmark;
mod command;
pub mod desktop;
pub mod fs;
pub mod pty;
pub mod release;
pub mod storage;
pub mod system;
pub mod watch;
