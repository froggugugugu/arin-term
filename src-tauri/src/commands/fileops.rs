//! ファイル操作のコマンド(設計書 §5.2 の「ファイル操作」、計画書 T42c)。
//!
//! 引数を受けて `FileOps`(`src-tauri/src/fileops.rs`)を呼ぶだけにする。規則は `arin-core` が持つ。
//! 項目は「プロジェクト ID + ルート基準の相対パス」で受ける。相対パスは `RelPath` の読み込みで検査され、
//! 絶対パス・`..`・`.`・空の要素・NUL はコマンドに届く前に拒否される。絶対パスは返さない(設計書 §12)。
//!
//! 衝突は応答(`conflict`)で返し、画面が確認を出して対処(`resolution`)を付けて呼び直す。`resolution` は省略できる。
//! ファイルの移動・コピー・ゴミ箱は止まりうるので、どの操作も待ってよいスレッドで行う。

use arin_core::fileops::{ConflictResolution, FileOpOutcome, NewEntryKind};
use arin_core::paths::RelPath;
use arin_core::project::ProjectId;
use tauri::{AppHandle, Manager, Runtime};

use crate::fileops::FileOps;

/// `op` を待ってよいスレッドで `FileOps` に対して行う。
async fn on_blocking_thread<R: Runtime>(
    app: AppHandle<R>,
    op: impl FnOnce(&FileOps) -> Result<FileOpOutcome, String> + Send + 'static,
) -> Result<FileOpOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || op(&app.state::<FileOps>()))
        .await
        .map_err(|e| format!("ファイル操作を完了できなかった: {e}"))?
}

/// フォルダ `parent`(ルートは空の文字列)に空のファイルかフォルダ `name` を作る(FR-025)。
#[tauri::command]
pub async fn create_entry<R: Runtime>(
    app: AppHandle<R>,
    project_id: ProjectId,
    parent: RelPath,
    name: String,
    kind: NewEntryKind,
    resolution: Option<ConflictResolution>,
) -> Result<FileOpOutcome, String> {
    on_blocking_thread(app, move |ops| {
        ops.create(&project_id, &parent, &name, kind, resolution)
    })
    .await
}

/// 項目 `path` の名前を `new_name` に変える(FR-026)。
#[tauri::command]
pub async fn rename_entry<R: Runtime>(
    app: AppHandle<R>,
    project_id: ProjectId,
    path: RelPath,
    new_name: String,
    resolution: Option<ConflictResolution>,
) -> Result<FileOpOutcome, String> {
    on_blocking_thread(app, move |ops| {
        ops.rename(&project_id, &path, &new_name, resolution)
    })
    .await
}

/// 項目 `source` をフォルダ `dest_dir` の中へ移す(ツリー内のドロップ。FR-029)。
#[tauri::command]
pub async fn move_entry<R: Runtime>(
    app: AppHandle<R>,
    project_id: ProjectId,
    source: RelPath,
    dest_dir: RelPath,
    resolution: Option<ConflictResolution>,
) -> Result<FileOpOutcome, String> {
    on_blocking_thread(app, move |ops| {
        ops.move_entry(&project_id, &source, &dest_dir, resolution)
    })
    .await
}

/// 項目 `source` をフォルダ `dest_dir` の中へ写す(⌥ を押しながらのドロップ。FR-029)。
#[tauri::command]
pub async fn copy_entry<R: Runtime>(
    app: AppHandle<R>,
    project_id: ProjectId,
    source: RelPath,
    dest_dir: RelPath,
    resolution: Option<ConflictResolution>,
) -> Result<FileOpOutcome, String> {
    on_blocking_thread(app, move |ops| {
        ops.copy_entry(&project_id, &source, &dest_dir, resolution)
    })
    .await
}

/// 項目 `path` をゴミ箱に入れる(FR-028。ゴミ箱を経由しない削除は無い)。
#[tauri::command]
pub async fn trash_entry<R: Runtime>(
    app: AppHandle<R>,
    project_id: ProjectId,
    path: RelPath,
) -> Result<FileOpOutcome, String> {
    on_blocking_thread(app, move |ops| ops.trash(&project_id, &path)).await
}

/// 項目 `paths` をファイルの URL としてペーストボードに書く(ツリーでのコピー。Finder にペーストできる。FR-027)。
#[tauri::command]
pub async fn copy_entries<R: Runtime>(
    app: AppHandle<R>,
    project_id: ProjectId,
    paths: Vec<RelPath>,
) -> Result<FileOpOutcome, String> {
    on_blocking_thread(app, move |ops| ops.copy_to_pasteboard(&project_id, &paths)).await
}

/// 項目 `paths` を切り取る(FR-027)。ファイルはペーストまで動かない。
#[tauri::command]
pub async fn cut_entries<R: Runtime>(
    app: AppHandle<R>,
    project_id: ProjectId,
    paths: Vec<RelPath>,
) -> Result<FileOpOutcome, String> {
    on_blocking_thread(app, move |ops| ops.cut(&project_id, &paths)).await
}

/// ペーストボードのファイルをフォルダ `dest_dir` の中へ置く(FR-027)。切り取ったものなら移し、ほかはコピーする。
/// ペーストボードにファイルが無ければ何もしない。
#[tauri::command]
pub async fn paste_entries<R: Runtime>(
    app: AppHandle<R>,
    project_id: ProjectId,
    dest_dir: RelPath,
    resolution: Option<ConflictResolution>,
) -> Result<FileOpOutcome, String> {
    on_blocking_thread(app, move |ops| {
        ops.paste(&project_id, &dest_dir, resolution)
    })
    .await
}
