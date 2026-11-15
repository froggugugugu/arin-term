//! Rust から画面へ送るイベントの名前(設計書 §5.2)。画面側は `src/ipc/client.ts` の `EventMap` と名前を合わせる。
//!
//! 写しのイベント(`app-snapshot`)の名前は `commands/app.rs` の `SNAPSHOT_EVENT`(M1 から)。

/// メニューバーの項目から画面へ操作を送るイベント。中身は `arin_core::workspace::MenuCommand`(例 `"select-all"`)。
pub const MENU_COMMAND_EVENT: &str = "menu-command";

/// 終了の要求を止めて、画面に確認を求めるイベント(FR-063)。中身は `arin_core::workspace::QuitRequest`。
pub const QUIT_REQUESTED_EVENT: &str = "quit-requested";
