//! ファイル操作(計画書 T42c、FR-025〜030・FR-035・FR-036、設計書 §5.2・§7.1「ファイル操作」・§12)。
//!
//! 画面からは「プロジェクト ID + ルート基準の相対パス(`RelPath`)」と名前・列挙の値だけを受け取り、絶対パスは受け取らない。
//! ルートの絶対パスはここで `SessionRegistry`(中の `Workspace`)から引き、手順(`arin_core::fileops::service`)に渡す。
//! 範囲の検査・名前の検査・衝突の扱いは手順の中で行い、ここは素通しする。応答(`FileOpOutcome`)に絶対パスは載らない。
//!
//! 切り取りの状態(`FileClipboard`)はアプリに 1 つだけ持つ(別のプロジェクトへの切り取り → ペーストも許す。【仮定】13)。
//! ペーストボードは組み立てで渡す(本物は一般のペーストボード、テストは名前付きのもの)。
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
        self.with_root(id, |service, root| {
            // 切り取りの状態はペーストが済むまで持つ(同時の切り取り・ペーストと混ざらないように)。
            let mut clipboard = lock(&self.clipboard);
            let source_root = |project: &ProjectId| self.root(project).ok().flatten();
            service.paste(
                self.pasteboard.as_ref(),
                &mut clipboard,
                &source_root,
                root,
                dest_dir,
                resolution,
            )
        })
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
