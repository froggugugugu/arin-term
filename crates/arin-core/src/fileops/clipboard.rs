//! 切り取りの状態(設計書 §5.1 の `FileClipboard`、§9.1、FR-027、計画書 M3 の【仮定】13、計画書 T39b)。
//!
//! Finder には「切り取り」が無いので、アプリの中の状態で表す。切り取ったときはペーストボードにファイルの URL を
//! 書き(ほかのアプリからはコピーに見える)、そのときの変更番号を覚える。ペーストのときに変更番号が同じなら
//! 移動、変わっていれば(ほかでコピーした・文字列を書いた)切り取りを捨ててペーストボードの中身のコピーにする。
//!
//! 切り取った項目は「プロジェクトの ID + 相対パス」で持つ(ルートが動いても、ペーストのときにルートを引き直して
//! 検査し直すため)。絶対パスは持たない(ペーストボードに書いた絶対パスは OS の側にある)。
//!
//! ペーストは「決める(`plan`)→ 置く(手順の `run_paste`。この状態に触れない)→ 片付ける(`settle`)」に分ける。
//! アプリは決めると片付けるの間だけロックを持ち、長いコピーのあいだは持たない(T42c の `FileOps::paste`)。
//!
//! 複数の項目のペーストが途中の項目で衝突したら(事前の検査をすり抜けた衝突)、置き終えた項目を覚えておき、
//! 対処を付けた呼び直しでは飛ばす: 切り取りは移し終えた項目を切り取りの状態から外し、コピーは置き終えた数を
//! (ペーストボードの変更番号と置く先と一緒に)覚える。対処の無いペースト(新しいペースト)は覚えた数を使わない。

use std::path::PathBuf;

use super::types::{ConflictResolution, FileOpOutcome};
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

/// 途中で衝突したコピーのペーストの、置き終えた数(呼び直しで飛ばす)。
#[derive(Debug, Clone, PartialEq, Eq)]
struct CopyProgress {
    /// そのときのペーストボードの変更番号(変わっていれば使わない)。
    change_count: i64,
    /// 置く先のフォルダの実パス(違うフォルダへのペーストでは使わない)。
    dest: PathBuf,
    /// ペーストボードの先頭から数えて、置き終えた数。
    done: usize,
}

/// ペーストで何をするかを決めたもの(`FileClipboard::plan`)。置く手順(`run_paste`)に渡し、
/// 終わったら `FileClipboard::settle` に戻す。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PastePlan {
    /// 移すか写すか、何もしないか。
    pub source: PasteSource,
    /// 決めたときのペーストボードの変更番号。
    change_count: i64,
    /// コピーの呼び直しで飛ばす数と、その置く先(実パス)。
    copy_done: Option<(PathBuf, usize)>,
}

impl PastePlan {
    /// コピーの呼び直しで、置く先 `dest`(実パス)のときに飛ばす先頭の数。
    #[must_use]
    pub fn skip_for(&self, dest: &std::path::Path) -> usize {
        match (&self.source, &self.copy_done) {
            (PasteSource::Copy { paths }, Some((done_dest, done))) if done_dest == dest => {
                (*done).min(paths.len())
            }
            _ => 0,
        }
    }
}

/// 置いた結果(`run_paste` が返す。`settle` で切り取りの状態に映す)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasteReport {
    /// 画面へ返す応答。
    pub outcome: FileOpOutcome,
    /// 計画の一覧の先頭から数えて、置き終えた数(前の呼び出しで飛ばした数を含む)。
    pub done: usize,
    /// 置く先のフォルダの実パス(範囲の検査を通る前に終わったときは `None`)。
    pub dest: Option<PathBuf>,
    /// 切り取ったプロジェクトが見つからなかった(切り取りの状態を捨てる)。
    pub source_gone: bool,
}

impl PasteReport {
    /// 何も置かずに終わった結果。
    #[must_use]
    pub fn nothing(outcome: FileOpOutcome) -> Self {
        Self {
            outcome,
            done: 0,
            dest: None,
            source_gone: false,
        }
    }
}

/// 切り取りの状態。アプリに 1 つ持つ(T42c)。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileClipboard {
    cut: Option<CutItems>,
    copy_progress: Option<CopyProgress>,
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
        self.copy_progress = None;
    }

    /// 切り取りの状態を捨てる(コピーした・移し終えた)。途中のコピーの覚えも捨てる。
    pub fn clear(&mut self) {
        self.cut = None;
        self.copy_progress = None;
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

    /// ペーストで何をするかを決める(`source` と同じ判定)。対処を付けた呼び直しで、ペーストボードが
    /// 変わっていなければ、途中で衝突したコピーの置き終えた数も渡す。対処の無いペーストでは捨てる。
    ///
    /// # Errors
    ///
    /// ペーストボードを読めなかったとき。
    pub fn plan(
        &mut self,
        pasteboard: &dyn FilePasteboardPort,
        resolution: Option<ConflictResolution>,
    ) -> Result<PastePlan, PasteboardError> {
        let change_count = pasteboard.change_count();
        let progress = self.copy_progress.take();
        let source = self.source(pasteboard)?;
        let copy_done = match (&source, progress, resolution) {
            (PasteSource::Copy { .. }, Some(progress), Some(_))
                if progress.change_count == change_count =>
            {
                Some((progress.dest, progress.done))
            }
            _ => None,
        };
        Ok(PastePlan {
            source,
            change_count,
            copy_done,
        })
    }

    /// 置いた結果を切り取りの状態に映す。決めたあとに別の切り取り・コピーが記録されていたら(変更番号が違う)、
    /// その状態には触れない。
    ///
    /// - 移動: 済めば切り取りを捨てる。途中で止まったら、移し終えた項目を切り取りの状態から外す
    ///   (衝突のあいだは残りを保ち、対処を付けて呼び直すと続きを移す)
    /// - コピー: 途中で衝突したら置き終えた数を覚える。それ以外は覚えを捨てる
    pub fn settle(&mut self, plan: &PastePlan, report: &PasteReport) {
        match &plan.source {
            PasteSource::Nothing => {}
            PasteSource::Move { project, items } => {
                let Some(cut) = self.cut.as_mut() else {
                    return;
                };
                if cut.change_count != plan.change_count
                    || &cut.project != project
                    || &cut.items != items
                {
                    return;
                }
                let finished = matches!(report.outcome, FileOpOutcome::Done { .. });
                if finished || report.source_gone || report.done >= cut.items.len() {
                    self.cut = None;
                } else {
                    cut.items.drain(..report.done);
                }
            }
            PasteSource::Copy { .. } => {
                if self.cut.is_some() {
                    // 決めたあとに切り取りが記録された。覚えを残さない。
                    self.copy_progress = None;
                    return;
                }
                self.copy_progress = match (&report.outcome, &report.dest) {
                    (FileOpOutcome::Conflict { .. }, Some(dest)) if report.done > 0 => {
                        Some(CopyProgress {
                            change_count: plan.change_count,
                            dest: dest.clone(),
                            done: report.done,
                        })
                    }
                    _ => None,
                };
            }
        }
    }
}
