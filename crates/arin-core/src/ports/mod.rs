//! OS 入出力の抽象(設計書 §5.1、§3.2)。実装は `arin-platform` に置く。
//!
//! PTY(`PtyPort`)、ルートフォルダ(`RootLocatorPort`・`BookmarkPort`)、乱数(`IdSource`)を置く。
//! 他のポートは使うマイルストーンで足す。

mod pty;
mod root;
mod system;

pub use pty::{ChildExit, PtyError, PtyPort, PtySession, PtySize, SpawnedPty};
pub use root::{BookmarkPort, OpenRoot, ResolvedBookmark, RootError, RootLocatorPort};
pub use system::{IdSource, IdSourceError};
