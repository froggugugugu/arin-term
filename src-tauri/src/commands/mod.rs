//! 画面からの操作を受ける薄い関数(機能ごとに 1 ファイル。設計書 §4)。
//!
//! コマンドは引数を受けて Core を呼び、結果を返すだけにする。規則を書かない(設計書 §3.2)。

pub mod app;
pub mod desktop;
pub mod editor;
pub mod fileops;
pub mod layout;
pub mod project;
pub mod terminal;
pub mod tree;
pub mod update;
