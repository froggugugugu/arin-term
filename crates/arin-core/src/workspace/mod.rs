//! ストアをまたぐ操作の入口と、画面へ渡す状態の写しの生成(設計書 §5.1・§5.2・§6.3)。
//!
//! `Workspace` が `ProjectStore`(プロジェクトの一覧と選択)・`TerminalStore`(タブの並び)・ペインの寸法と、
//! ターミナルごとの cwd と状態を持つ。入出力はしない。起動時に読んだ状態とルートの解決の結果、
//! 削除と終了の確認に使う「動作中のターミナル」は呼び出し側(`src-tauri`)が調べて引数で渡す。

mod outcome;
mod projects;
mod snapshot;
mod state;
mod terminals;

pub use outcome::{
    AddProjectOutcome, ConnectOutcome, RemoveOutcome, RemoveProjectOutcome, RootResolution,
    RunningTerminal, Startup, WorkspaceError,
};
pub use snapshot::{
    AppSnapshot, ProjectCardSnapshot, SNAPSHOT_SCHEMA_VERSION, TerminalTabSnapshot,
};
pub use state::Workspace;
pub use terminals::ConnectPlan;
