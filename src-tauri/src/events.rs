//! Rust から画面へ送るイベントの名前(設計書 §5.2)。画面側は `src/ipc/client.ts` の `EventMap` と名前を合わせる。
//!
//! 写しのイベント(`app-snapshot`)の名前は `commands/app.rs` の `SNAPSHOT_EVENT`(M1 から)。

/// メニューバーの項目から画面へ操作を送るイベント。中身は `arin_core::workspace::MenuCommand`(例 `"select-all"`)。
pub const MENU_COMMAND_EVENT: &str = "menu-command";

/// 終了の要求を止めて、画面に確認を求めるイベント(FR-063)。中身は `arin_core::workspace::QuitRequest`。
pub const QUIT_REQUESTED_EVENT: &str = "quit-requested";

/// 選択中のプロジェクトのツリーで、外からの変更が届いたことを画面へ送るイベント(FR-014。計画書 T42b)。
/// 中身は `arin_core::tree::TreeChange`(プロジェクト ID・変わったフォルダの相対パス・全部読み直し。絶対パスは載せない)。
pub const TREE_CHANGED_EVENT: &str = "tree-changed";

/// 設定ファイルの内容が変わったことを画面へ送るイベント(ADR-012 の 5、前提 (g)。計画書 T65b で名前を置き、T65c が送る)。
/// 中身は無い。画面は受けたら `get_shortcuts` と `get_settings_notice` を呼び直す(T65e)。
pub const SETTINGS_CHANGED_EVENT: &str = "settings-changed";
