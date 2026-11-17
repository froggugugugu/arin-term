//! ツリーのコマンド(設計書 §5.2 の「ツリー」、計画書 T42a)。
//!
//! 引数を受けて `Tree`(`src-tauri/src/tree.rs`)を呼ぶだけにする。規則は `arin-core` が持つ。
//! フォルダは「プロジェクト ID + ルート基準の相対パス」で受ける。相対パスは `RelPath` の読み込みで検査され、
//! 絶対パス・`..`・`.`・空の要素・NUL はコマンドに届く前に拒否される。絶対パスは返さない(設計書 §12)。
//! ツリーの中身は写しに含めず、画面がフォルダを展開したときに問い合わせる。読み込みは待ってよいスレッドで行う。

use arin_core::paths::RelPath;
use arin_core::project::ProjectId;
use arin_core::tree::DirListing;
use tauri::{AppHandle, Manager, Runtime};

use crate::tree::Tree;

/// フォルダの中身を返す(FR-013)。除外と並び順を当てる。ルートは `path` を空の文字列にする。
/// プロジェクトが見つからない・フォルダが無いときは `notFound`、ファイルを指したら `notADirectory`、
/// シンボリックリンクでルートの外へ出るなら読まずに `outsideRoot`。
#[tauri::command]
pub async fn list_directory<R: Runtime>(
    app: AppHandle<R>,
    project_id: ProjectId,
    path: RelPath,
) -> Result<DirListing, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Tree>().list_directory(&project_id, &path)
    })
    .await
    .map_err(|e| format!("フォルダの読み込みを完了できなかった: {e}"))?
}

/// 隠しファイル表示を切り替える(FR-017。プロジェクトごとに保存する)。写しが変わり、保存が予約される。
#[tauri::command]
pub fn set_show_hidden<R: Runtime>(
    app: AppHandle<R>,
    project_id: ProjectId,
    show_hidden: bool,
) -> Result<(), String> {
    app.state::<Tree>()
        .set_show_hidden(&project_id, show_hidden)
}
