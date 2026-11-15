//! プロジェクトの規則(設計書 §5.1 の `arin-core/project`、FR-001・003・007・010・059)。
//!
//! プロジェクトの識別子(`ProjectId`)、色の識別子と割当(`ProjectColor`・`ColorAssigner`)、
//! ルートの参照(`RootReference`)、一覧と選択(`ProjectStore`)、画面へ渡す写し(`ProjectListSnapshot`)を置く。
//!
//! 入出力をしない。フォルダの実在の確認・パスの正規化(シンボリックリンクの解決)・ブックマークは
//! `crate::ports::RootLocatorPort`・`BookmarkPort`、ID の乱数は `crate::ports::IdSource` を呼び出し側が使い、結果を渡す。
//! 並び替えと表示名の変更(FR-005)は M6、ツリーの保存設定(展開・隠しファイル)は M3 で足す。

mod color;
mod id;
mod path;
mod store;

pub use color::{ColorAssigner, ProjectColor};
pub use id::{ProjectId, ProjectIdError};
pub use store::{
    Project, ProjectError, ProjectListSnapshot, ProjectSnapshot, ProjectStore, RootReference,
};

#[cfg(test)]
mod tests;
