//! プロジェクトのコマンド(設計書 §5.2 の「プロジェクト」、計画書 T33c)。
//!
//! 引数を受けて `Projects`(`src-tauri/src/projects.rs`)を呼ぶだけにする。規則は `arin-core` が持つ。
//! ダイアログを開くコマンドは、画面からは「開いて」と頼むだけで、選んだパスは画面に返さない(設計書 §12)。
//! 待ちうる操作(ダイアログ、シェルの終了)は Tauri の待ってよいスレッドで行い、非同期の実行環境とメインスレッドを塞がない。
//! 失敗は日本語の文言にして画面へ返す。

use arin_core::project::{ProjectColor, ProjectId};
use arin_core::workspace::{AddProjectOutcome, RemoveProjectOutcome};
use tauri::{AppHandle, Manager, Runtime};

use crate::projects::Projects;

/// 待ってよいスレッドで `f` を行う。
async fn blocking<R, T, F>(app: AppHandle<R>, f: F) -> Result<T, String>
where
    R: Runtime,
    T: Send + 'static,
    F: FnOnce(&Projects) -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || f(&app.state::<Projects>()))
        .await
        .map_err(|e| format!("プロジェクトの操作を完了できなかった: {e}"))?
}

/// フォルダ選択のダイアログを開き、選ばれたフォルダを登録して選び、タブを 1 枚開く(FR-001)。
/// 同じフォルダが登録済みなら増やさずそれを選ぶ(`alreadyRegistered`。FR-010)。取り消されたら `cancelled`。
#[tauri::command]
pub async fn add_project_by_dialog<R: Runtime>(
    app: AppHandle<R>,
) -> Result<AddProjectOutcome, String> {
    blocking(app, Projects::add_by_dialog).await
}

/// プロジェクトを選ぶ(FR-002)。ターミナルには触れない。
#[tauri::command]
pub fn select_project<R: Runtime>(app: AppHandle<R>, project_id: ProjectId) -> Result<(), String> {
    app.state::<Projects>().select(&project_id)
}

/// プロジェクトを一覧から外す(FR-003。フォルダには触れない)。動作中のターミナルがあり `confirmed` が偽なら、
/// 何も変えずに `needsConfirmation` と一覧を返す(画面が確認を出し、確認したら `confirmed` を真にして呼び直す)。
/// 外したらそのプロジェクトのシェルを終わらせる。
#[tauri::command]
pub async fn remove_project<R: Runtime>(
    app: AppHandle<R>,
    project_id: ProjectId,
    confirmed: bool,
) -> Result<RemoveProjectOutcome, String> {
    blocking(app, move |projects| projects.remove(&project_id, confirmed)).await
}

/// フォルダ選択のダイアログを開き、選ばれたフォルダにつなぎ直す(FR-007)。ID・色を保ち、タブは開かない。
/// つなぎ直したら `true`、取り消されたら `false`。
#[tauri::command]
pub async fn relink_project<R: Runtime>(
    app: AppHandle<R>,
    project_id: ProjectId,
) -> Result<bool, String> {
    blocking(app, move |projects| projects.relink_by_dialog(&project_id)).await
}

/// 色を変える(FR-059)。
#[tauri::command]
pub fn set_project_color<R: Runtime>(
    app: AppHandle<R>,
    project_id: ProjectId,
    color: ProjectColor,
) -> Result<(), String> {
    app.state::<Projects>().set_color(&project_id, color)
}

/// ルートの絶対パスをクリップボードに書く(FR-064)。選択は変えない。パスは画面に返さない。
#[tauri::command]
pub fn copy_project_path<R: Runtime>(
    app: AppHandle<R>,
    project_id: ProjectId,
) -> Result<(), String> {
    app.state::<Projects>().copy_path(&project_id)
}
