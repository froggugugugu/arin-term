//! OS 入出力の抽象(設計書 §5.1、§3.2)。実装は `arin-platform` に置く。
//!
//! PTY(`PtyPort`)、ルートフォルダ(`RootLocatorPort`・`BookmarkPort`)、乱数(`IdSource`)を置く。
//! 状態ファイル(`state_file`)、ペーストボード(`pasteboard`)、時計(`clock`)、ファイルシステム(`fs`)、
//! ファイル監視(`watch`)、デスクトップ(`desktop`)、設定ファイル(`settings_file`。M4.5)、文書(`document`。M4.8)は、モジュールを公開して
//! 中の名前で使う(並行するタスクがこのファイルを触らずに足せるようにするため)。他のポートは使うマイルストーンで足す。

pub mod clock;
pub mod desktop;
pub mod document;
pub mod fs;
pub mod pasteboard;
mod pty;
pub mod release;
mod root;
pub mod settings_file;
pub mod state_file;
mod system;
pub mod watch;

pub use pty::{ChildExit, Foreground, PtyError, PtyPort, PtySession, PtySize, SpawnedPty};
pub use root::{BookmarkPort, OpenRoot, ResolvedBookmark, RootError, RootLocatorPort};
pub use system::{IdSource, IdSourceError};
