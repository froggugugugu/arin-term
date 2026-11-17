//! ルートフォルダのポート(設計書 §1.4 #12・#13、§5.1、§7.1「ルートフォルダへの追従」、FR-007・FR-010)。
//!
//! - `RootLocatorPort`: 登録するフォルダを調べて正規化したパスを返す口と、開いたままにしたフォルダの現在位置を調べる口
//! - `BookmarkPort`: macOS のブックマークを作る口と解決する口
//!
//! 実装は `arin-platform`(T31b)。ブックマークの中身は `arin-core` には不透明な文字列(base64)として渡す。
//! バイト列から文字列への変換は実装側が行い、どのクレートで作るかを型に出さない(T28 の決定)。

use std::path::{Path, PathBuf};

use thiserror::Error;

/// ルートフォルダの操作の失敗。原因は実装側の文言をそのまま持つ。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RootError {
    /// そのパスに何も無い、またはブックマークを解決できなかった(削除された)。
    #[error("フォルダが見つからない: {0}")]
    NotFound(String),
    /// そのパスはフォルダではない。
    #[error("フォルダではない: {0}")]
    NotADirectory(String),
    /// ブックマークを作れなかった、または中身が壊れていた。
    #[error("ブックマークを扱えなかった: {0}")]
    Bookmark(String),
    /// それ以外の入出力の失敗(権限が無いなど)。
    #[error("フォルダを調べられなかった: {0}")]
    Io(String),
}

/// ブックマークを解決した結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedBookmark {
    /// いまのフォルダの絶対パス(シンボリックリンクを解決した実パス)。
    pub path: PathBuf,
    /// 古くなっている(改名・移動のあと)。真なら `path` から作り直して保存し直す。
    pub stale: bool,
}

/// macOS のブックマークを作って解決する口。
pub trait BookmarkPort: Send + Sync {
    /// フォルダのブックマークを作り、base64 の文字列で返す。
    ///
    /// # Errors
    ///
    /// フォルダが無いとき(`RootError::NotFound`)、作れなかったとき(`RootError::Bookmark`)。
    fn create(&self, path: &Path) -> Result<String, RootError>;

    /// `create` が返した文字列を解決する。
    ///
    /// # Errors
    ///
    /// 指していたフォルダが無いとき(`RootError::NotFound`)、中身が壊れているとき(`RootError::Bookmark`)。
    fn resolve(&self, bookmark: &str) -> Result<ResolvedBookmark, RootError>;
}

/// 開いたままにしたフォルダ。改名・移動されても同じフォルダを指し続ける。
pub trait OpenRoot: Send {
    /// いまのフォルダの絶対パス。
    ///
    /// # Errors
    ///
    /// フォルダが削除されて取れないとき(`RootError::NotFound`)、それ以外で取れないとき(`RootError::Io`)。
    fn current_path(&self) -> Result<PathBuf, RootError>;
}

/// 登録するフォルダを調べる口と、ルートを開いたままにして追う口。
pub trait RootLocatorPort: Send + Sync {
    /// フォルダであることを確かめ、シンボリックリンクを解決した絶対パス(ディスク上の大文字・小文字の表記)を返す。
    ///
    /// 二重登録の判定(FR-010)はこのパスどうしで行うので、同じフォルダは必ず同じ文字列になるようにする。
    ///
    /// # Errors
    ///
    /// 何も無いとき(`RootError::NotFound`)、フォルダでないとき(`RootError::NotADirectory`)、
    /// 調べられないとき(`RootError::Io`)。
    fn canonical_dir(&self, path: &Path) -> Result<PathBuf, RootError>;

    /// フォルダを開いたままにする。見回りが `OpenRoot::current_path` で現在位置を調べる。
    ///
    /// # Errors
    ///
    /// 開けないとき(`RootError::NotFound`・`RootError::Io`)。
    fn open(&self, path: &Path) -> Result<Box<dyn OpenRoot>, RootError>;
}
