//! ファイル監視のポート(設計書 §1.3 #7、§5.4「ファイル監視」、FR-014、計画書 T38a)。
//!
//! 実装は `arin-platform/src/watch/`(T40b。FSEvents)。監視は選択中のプロジェクトのルートに 1 本だけ張る(NFR-003)。
//! 除外の判定と「変わったフォルダ」へのまとめは `tree`(T38c)を呼び出し側(T42b)が当てる。

use std::path::{Path, PathBuf};

use thiserror::Error;

/// 監視から届いた変更の束。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WatchBatch {
    /// 変わった項目の絶対パス(実装が短い待ち時間でまとめたもの。順と重複は決めない)。
    pub paths: Vec<PathBuf>,
    /// OS が個々の変更を伝えきれなかった(FSEvents の「下を調べ直す」の印など)。真なら全部読み直す。
    pub rescan: bool,
}

/// 変更の受け口。実装の監視のスレッドから呼ばれる。
pub type WatchSink = Box<dyn Fn(WatchBatch) + Send + Sync + 'static>;

/// 張った監視。**捨てると監視が止まり**、そのあと受け口は呼ばれない。
pub trait ActiveWatch: Send {}

/// 監視を張れなかった。原因は実装側の文言をそのまま持つ。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("フォルダを監視できなかった: {0}")]
pub struct WatchError(pub String);

/// フォルダに再帰の監視を張る口。
pub trait WatcherPort: Send + Sync {
    /// `root` とその下の変更を `sink` へ渡す監視を張る。返した値を捨てると止まる。
    ///
    /// # Errors
    ///
    /// 張れなかったとき(フォルダが無いなど)。
    fn watch(&self, root: &Path, sink: WatchSink) -> Result<Box<dyn ActiveWatch>, WatchError>;
}
