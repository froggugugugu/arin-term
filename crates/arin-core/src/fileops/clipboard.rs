//! 切り取りの状態(設計書 §5.1 の `FileClipboard`、§9.1、FR-027、計画書 M3 の【仮定】13、計画書 T39b)。
//!
//! Finder には「切り取り」が無いので、アプリの中の状態で表す。切り取ったときはペーストボードにファイルの URL を
//! 書き(ほかのアプリからはコピーに見える)、そのときの変更番号を覚える。ペーストのときに変更番号が同じなら
//! 移動、変わっていれば(ほかでコピーした・文字列を書いた)切り取りを捨ててペーストボードの中身のコピーにする。
//!
//! 切り取った項目は「プロジェクトの ID + 相対パス」で持つ(ルートが動いても、ペーストのときにルートを引き直して
//! 検査し直すため)。絶対パスは持たない(ペーストボードに書いた絶対パスは OS の側にある)。

use std::path::PathBuf;

use crate::paths::RelPath;
use crate::ports::pasteboard::{FilePasteboardPort, PasteboardError};
use crate::project::ProjectId;

/// 切り取った項目と、切り取ったときのペーストボードの変更番号。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CutItems {
    /// 切り取った項目のプロジェクト。
    pub project: ProjectId,
    /// 切り取った項目(ルート基準の相対パス)。
    pub items: Vec<RelPath>,
    /// 書いたあとの変更番号(`FilePasteboardPort::write_file_urls` の戻り値)。
    pub change_count: i64,
}

/// ペーストで何をするか。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasteSource {
    /// ペーストボードにファイルが無い。何もしない(【仮定】8)。
    Nothing,
    /// 切り取ったものを移す(移動 1 回。コピーしてから消さない)。
    Move {
        /// 切り取った項目のプロジェクト。
        project: ProjectId,
        /// 切り取った項目。
        items: Vec<RelPath>,
    },
    /// ペーストボードのファイル(絶対パス。ルートの外でもよい)を写す。元は読むだけ。
    Copy {
        /// 置かれた順のファイルの絶対パス。
        paths: Vec<PathBuf>,
    },
}

/// 切り取りの状態。アプリに 1 つ持つ(T42c)。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileClipboard {
    cut: Option<CutItems>,
}

impl FileClipboard {
    /// 何も切り取っていない状態。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 切り取ったことを覚える。`change_count` はペーストボードに書いたあとの変更番号。
    pub fn record_cut(&mut self, project: ProjectId, items: Vec<RelPath>, change_count: i64) {
        self.cut = Some(CutItems {
            project,
            items,
            change_count,
        });
    }

    /// 切り取りの状態を捨てる(コピーした・移し終えた)。
    pub fn clear(&mut self) {
        self.cut = None;
    }

    /// いま切り取っている項目。
    #[must_use]
    pub fn cut_items(&self) -> Option<&CutItems> {
        self.cut.as_ref()
    }

    /// ペーストで何をするかを決める。ペーストボードが切り取りのときから変わっていれば、切り取りを捨てる。
    /// 移動になったときは切り取りを保つ(衝突で呼び直すため)。移し終えたら呼び出し側が `clear` する。
    ///
    /// # Errors
    ///
    /// ペーストボードを読めなかったとき。
    pub fn source(
        &mut self,
        pasteboard: &dyn FilePasteboardPort,
    ) -> Result<PasteSource, PasteboardError> {
        if let Some(cut) = &self.cut {
            if cut.change_count == pasteboard.change_count() {
                return Ok(PasteSource::Move {
                    project: cut.project.clone(),
                    items: cut.items.clone(),
                });
            }
            self.cut = None;
        }
        let paths = pasteboard.read_file_urls()?;
        if paths.is_empty() {
            Ok(PasteSource::Nothing)
        } else {
            Ok(PasteSource::Copy { paths })
        }
    }
}
