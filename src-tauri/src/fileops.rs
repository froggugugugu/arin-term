//! ファイル操作(計画書 T42c、FR-025〜030・FR-035・FR-036、設計書 §5.2・§7.1「ファイル操作」・§12)。
//!
//! 画面からは「プロジェクト ID + ルート基準の相対パス(`RelPath`)」と名前・列挙の値だけを受け取り、絶対パスは受け取らない。
//! ルートの絶対パスはここで `SessionRegistry`(中の `Workspace`)から引き、手順(`arin_core::fileops::service`)に渡す。
//! 範囲の検査・名前の検査・衝突の扱いは手順の中で行い、ここは素通しする。応答(`FileOpOutcome`)に絶対パスは載らない。
//!
//! 切り取りの状態(`FileClipboard`)はアプリに 1 つだけ持つ(別のプロジェクトへの切り取り → ペーストも許す。【仮定】13)。
//! ペーストボードは組み立てで渡す(本物は一般のペーストボード、テストは名前付きのもの)。
//! ペーストは、移すか写すかを決めるあいだと、終わって切り取りの状態を片付けるあいだだけロックを持ち、
//! ファイルの移動・コピー(長くかかりうる)はロックの外で行う。そのあいだの別の切り取り・コピーは片付けで消さない。
//!
//! コピーの作業用フォルダ(`.arin-copy-…`)を片付けられなかったときは、標準エラーに `[arin:fileops]` の 1 行を出す
//! (`report_staging_leftover`。`LocalFileSystem::with_leftover_report` に渡す)。画面への応答には絶対パスを載せない。
//!
//! ファイルの移動・コピー・ゴミ箱は止まりうるので、メインスレッドから呼ばない(コマンドは待ってよいスレッドで呼ぶ)。
//! 操作のあとのツリーの更新は監視(`tree-changed`)と画面の読み直し(【仮定】20)に任せる。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use arin_core::fileops::clipboard::FileClipboard;
use arin_core::fileops::service::FileOperationService;
use arin_core::fileops::{ConflictResolution, FileOpOutcome, NewEntryKind};
use arin_core::paths::RelPath;
use arin_core::ports::fs::FileSystemPort;
use arin_core::ports::pasteboard::FilePasteboardPort;
use arin_core::project::{ProjectError, ProjectId};

use crate::sessions::SessionRegistry;

/// ファイル操作。Tauri の管理状態に登録する。
pub struct FileOps {
    sessions: SessionRegistry,
    fs: Arc<dyn FileSystemPort>,
    pasteboard: Arc<dyn FilePasteboardPort>,
    clipboard: Mutex<FileClipboard>,
}

impl FileOps {
    #[must_use]
    pub fn new(
        sessions: SessionRegistry,
        fs: Arc<dyn FileSystemPort>,
        pasteboard: Arc<dyn FilePasteboardPort>,
    ) -> Self {
        Self {
            sessions,
            fs,
            pasteboard,
            clipboard: Mutex::new(FileClipboard::new()),
        }
    }

    /// フォルダ `parent` に空のファイルかフォルダ `name` を作る(FR-025)。済めば作った項目を選ぶ。
    ///
    /// # Errors
    ///
    /// その ID のプロジェクトが無いとき(文言。絶対パスを載せない)。以下の操作も同じ。
    pub fn create(
        &self,
        id: &ProjectId,
        parent: &RelPath,
        name: &str,
        kind: NewEntryKind,
        resolution: Option<ConflictResolution>,
    ) -> Result<FileOpOutcome, String> {
        self.with_root(id, |service, root| {
            service.create(root, parent, name, kind, resolution)
        })
    }

    /// 項目 `target` の名前を `new_name` に変える(FR-026)。
    ///
    /// # Errors
    ///
    /// その ID のプロジェクトが無いとき。
    pub fn rename(
        &self,
        id: &ProjectId,
        target: &RelPath,
        new_name: &str,
        resolution: Option<ConflictResolution>,
    ) -> Result<FileOpOutcome, String> {
        self.with_root(id, |service, root| {
            service.rename(root, target, new_name, resolution)
        })
    }

    /// 項目 `source` をフォルダ `dest_dir` の中へ移す(ツリー内のドロップ。FR-029)。
    ///
    /// # Errors
    ///
    /// その ID のプロジェクトが無いとき。
    pub fn move_entry(
        &self,
        id: &ProjectId,
        source: &RelPath,
        dest_dir: &RelPath,
        resolution: Option<ConflictResolution>,
    ) -> Result<FileOpOutcome, String> {
        self.with_root(id, |service, root| {
            service.move_entry(root, source, dest_dir, resolution)
        })
    }

    /// 項目 `source` をフォルダ `dest_dir` の中へ写す(⌥ を押しながらのドロップ。FR-029)。
    ///
    /// # Errors
    ///
    /// その ID のプロジェクトが無いとき。
    pub fn copy_entry(
        &self,
        id: &ProjectId,
        source: &RelPath,
        dest_dir: &RelPath,
        resolution: Option<ConflictResolution>,
    ) -> Result<FileOpOutcome, String> {
        self.with_root(id, |service, root| {
            service.copy_entry(root, source, dest_dir, resolution)
        })
    }

    /// 項目 `target` をゴミ箱に入れる(FR-028)。
    ///
    /// # Errors
    ///
    /// その ID のプロジェクトが無いとき。
    pub fn trash(&self, id: &ProjectId, target: &RelPath) -> Result<FileOpOutcome, String> {
        self.with_root(id, |service, root| service.trash(root, target))
    }

    /// 項目をファイルの URL としてペーストボードに書く(ツリーでのコピー。FR-027)。切り取りの状態は捨てる。
    /// 項目が 1 つも無ければ何もしない(ペーストボードの中身を消さない)。
    ///
    /// # Errors
    ///
    /// その ID のプロジェクトが無いとき。
    pub fn copy_to_pasteboard(
        &self,
        id: &ProjectId,
        items: &[RelPath],
    ) -> Result<FileOpOutcome, String> {
        if items.is_empty() {
            return Ok(FileOpOutcome::Done { select: None });
        }
        self.with_root(id, |service, root| {
            let mut clipboard = lock(&self.clipboard);
            service.copy_to_pasteboard(self.pasteboard.as_ref(), &mut clipboard, root, items)
        })
    }

    /// 項目を切り取る(FR-027)。ペーストボードにファイルの URL を書き、切り取った項目を覚える。
    /// 項目が 1 つも無ければ何もしない。
    ///
    /// # Errors
    ///
    /// その ID のプロジェクトが無いとき。
    pub fn cut(&self, id: &ProjectId, items: &[RelPath]) -> Result<FileOpOutcome, String> {
        if items.is_empty() {
            return Ok(FileOpOutcome::Done { select: None });
        }
        self.with_root(id, |service, root| {
            let mut clipboard = lock(&self.clipboard);
            service.cut(self.pasteboard.as_ref(), &mut clipboard, id, root, items)
        })
    }

    /// ペーストボードのファイルを、フォルダ `dest_dir` の中へ置く(FR-027)。切り取りのときからペーストボードが
    /// 変わっていなければ切り取った項目を移し(別のプロジェクトからでもよい)、変わっていればコピーする。
    ///
    /// # Errors
    ///
    /// その ID のプロジェクトが無いとき。
    pub fn paste(
        &self,
        id: &ProjectId,
        dest_dir: &RelPath,
        resolution: Option<ConflictResolution>,
    ) -> Result<FileOpOutcome, String> {
        let Some(root) = self.root(id)? else {
            return Ok(FileOpOutcome::NotFound);
        };
        let service = FileOperationService::new(self.fs.as_ref());
        // 決めるあいだだけロックを持つ(ペーストボードを読み、切り取りの状態から元の一覧を取り出す)。
        let plan = service.plan_paste(
            self.pasteboard.as_ref(),
            &mut lock(&self.clipboard),
            resolution,
        );
        let plan = match plan {
            Ok(plan) => plan,
            Err(outcome) => return Ok(outcome),
        };
        // 移動・コピーはロックの外で行う。
        let source_root = |project: &ProjectId| self.root(project).ok().flatten();
        let report = service.run_paste(&plan, &source_root, &root, dest_dir, resolution);
        // 取り直して片付ける(そのあいだに別の切り取り・コピーがあれば、その状態は消さない)。
        lock(&self.clipboard).settle(&plan, &report);
        Ok(report.outcome)
    }

    /// ルートを引いて手順を呼ぶ。ルートが見つからないプロジェクトは何もせずに `NotFound`。
    fn with_root(
        &self,
        id: &ProjectId,
        op: impl FnOnce(&FileOperationService<'_>, &Path) -> FileOpOutcome,
    ) -> Result<FileOpOutcome, String> {
        let Some(root) = self.root(id)? else {
            return Ok(FileOpOutcome::NotFound);
        };
        Ok(op(&FileOperationService::new(self.fs.as_ref()), &root))
    }

    /// プロジェクト `id` のルートの絶対パス。見つからない(ゴミ箱の中を含む)プロジェクトは `None`
    /// (最後のパスに触れない。`Tree::list_directory` と同じ)。
    fn root(&self, id: &ProjectId) -> Result<Option<PathBuf>, String> {
        let card = self
            .sessions
            .snapshot()
            .projects
            .into_iter()
            .find(|p| &p.id == id)
            .ok_or_else(|| ProjectError::NotFound(id.clone()).to_string())?;
        if card.missing {
            return Ok(None);
        }
        self.sessions
            .project_root_path(id)
            .map(Some)
            .map_err(|e| e.to_string())
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// ファイル操作の行の頭(標準エラー)。
pub const FILEOPS_PREFIX: &str = "[arin:fileops]";

/// 片付けられなかったコピーの作業用フォルダの場所を、頭の文字列を付けた 1 行にする(制御文字は空白にする)。
#[must_use]
pub fn staging_leftover_line(path: &Path) -> String {
    let place: String = path
        .display()
        .to_string()
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    format!("{FILEOPS_PREFIX} コピーの作業用フォルダを片付けられませんでした: {place}")
}

/// 片付けられなかったコピーの作業用フォルダを標準エラーに知らせる(`LocalFileSystem::with_leftover_report` に渡す)。
/// 消すことはしない(隠しファイル表示を ON にするとツリーに出る)。
pub fn report_staging_leftover(path: &Path) {
    eprintln!("{}", staging_leftover_line(path));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 片付けられなかった作業用フォルダは頭の文字列を付けた_1_行で知らせる() {
        let line = staging_leftover_line(Path::new("/x/app/.arin-copy-1-0"));
        assert_eq!(
            line,
            "[arin:fileops] コピーの作業用フォルダを片付けられませんでした: /x/app/.arin-copy-1-0"
        );
    }

    #[test]
    fn 名前に改行があっても_1_行に収める() {
        let line = staging_leftover_line(Path::new("/x/a\nb/.arin-copy-1-0"));
        assert!(!line.contains('\n'), "{line}");
    }
}
