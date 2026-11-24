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

/// ⌘W(メニュー)で表示中のエディタが未保存のとき、閉じずに画面へ確認を求めるイベント(ADR-015 の 4。計画書 T81d)。
/// 中身は `arin_core::editor::EditorCloseRequest`(`{ editorId }`)。画面が「保存」「保存しない」「キャンセル」を聞き、
/// 閉じるなら `close_editor` を呼ぶ。
pub const EDITOR_CLOSE_REQUESTED_EVENT: &str = "editor-close-requested";

/// 新しい版の確認の結果を画面へ送るイベント(ADR-018)。中身は `arin_core::update::UpdateNotice`。
pub const UPDATE_CHANGED_EVENT: &str = "update-changed";
