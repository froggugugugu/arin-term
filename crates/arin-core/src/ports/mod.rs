//! OS 入出力の抽象(設計書 §5.1、§3.2)。実装は `arin-platform` に置く。
//!
//! PTY(`PtyPort`)、ルートフォルダ(`RootLocatorPort`・`BookmarkPort`)、乱数(`IdSource`)を置く。
//! 状態ファイル(`state_file`)、ペーストボード(`pasteboard`)、時計(`clock`)は、モジュールを公開して中の名前で使う
//! (並行するタスクがこのファイルを触らずに足せるようにするため)。他のポートは使うマイルストーンで足す。

pub mod clock;
pub mod pasteboard;
mod pty;
mod root;
pub mod state_file;
mod system;

pub use pty::{ChildExit, Foreground, PtyError, PtyPort, PtySession, PtySize, SpawnedPty};
pub use root::{BookmarkPort, OpenRoot, ResolvedBookmark, RootError, RootLocatorPort};
pub use system::{IdSource, IdSourceError};
