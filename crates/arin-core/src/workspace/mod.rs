//! ストアをまたぐ操作の入口と、画面へ渡す状態の写しの生成(設計書 §5.1・§5.2・§6.3)。
//!
//! `Workspace` が `ProjectStore`(プロジェクトの一覧と選択)・`TerminalStore`(タブの並び)・`EditorStore`(エディタのタブ。
//! `editors.rs`。ADR-015 の 3)・ペインの寸法・表示の状態
//! (表示モード・タイルのページ・拡大。`display.rs`)・文字の大きさと透過(`appearance.rs`)と、
//! ターミナルごとの cwd と状態を持つ。入出力はしない。起動時に読んだ状態とルートの解決の結果、
//! 削除と終了の確認に使う「動作中のターミナル」は呼び出し側(`src-tauri`)が調べて引数で渡す。

mod appearance;
mod display;
mod editors;
mod events;
mod outcome;
mod panes;
mod projects;
mod snapshot;
mod state;
mod terminals;

pub use editors::ActiveTab;
pub use events::{MenuCommand, QuitRequest};
pub use outcome::{
    AddProjectOutcome, CloseEditorOutcome, ConnectOutcome, ProjectRoot, RemoveOutcome,
    RemoveProjectOutcome, RootResolution, RunningTerminal, Startup, UnsavedEditor, WorkspaceError,
};
pub use snapshot::{
    AppSnapshot, DisplaySnapshot, EditorSettingsSnapshot, EditorTabSnapshot, FontSizeSnapshot,
    FontSizesSnapshot, ProjectCardSnapshot, SNAPSHOT_SCHEMA_VERSION, TerminalTabSnapshot,
    TranslucencySnapshot, TranslucencyValueSnapshot,
};
pub use state::Workspace;
pub use terminals::ConnectPlan;
