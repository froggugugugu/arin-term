//! ツリーの操作の手順(計画書 T42a、FR-013・FR-017・FR-035、設計書 §5.2・§7.1・§12)。
//!
//! 画面からは「プロジェクト ID + ルート基準の相対パス(`RelPath`)」だけを受け取り、絶対パスは受け取らない。
//! ルートの絶対パスはここで `SessionRegistry`(中の `Workspace`)から引き、範囲の検査(`PathScopeGuard`)を
//! 通した実パスだけを読む。シンボリックリンクでルートの外へ出るフォルダは読まずに `OutsideRoot` を返す。
//! 応答(`DirListing`)とエラーの文言に絶対パスを載せない。
//!
//! 一覧から隠すもの:
//! - 除外の規則(`ExclusionRule`。隠しファイル表示が OFF のときの `.git`・`node_modules`・`.DS_Store`)
//! - アプリが一時的に作る作業用の名前(コピーの作業用フォルダ `.arin-copy-`、文書の保存の一時ファイル `.arin-save-`。
//!   `arin_core::tree::exclusion::WORK_NAME_PREFIXES`)。隠しファイル表示が OFF のとき出さない(作りかけを本人に
//!   操作させないため)。ON のときは、片付けられずに残ったもの(標準エラーの `[arin:fileops]`・`[arin:editor]`)を
//!   本人が見つけて片付けられるよう出す
//!
//! フォルダの読み込みは止まりうるので、メインスレッドから呼ばない(コマンドは待ってよいスレッドで呼ぶ)。

use std::path::Path;
use std::sync::Arc;

use arin_core::fileops::scope::{PathScopeGuard, ScopeError};
use arin_core::paths::RelPath;
use arin_core::ports::fs::{DirEntry, FileSystemPort, FsError};
use arin_core::project::{ProjectError, ProjectId};
use arin_core::tree::DirListing;
use arin_core::tree::exclusion::ExclusionRule;
use arin_core::tree::order::sort_entries;

use crate::sessions::SessionRegistry;

/// 読めなかったときの文言(原因の文言には絶対パスが入るので使わない)。
const READ_FAILED: &str = "フォルダを読めませんでした";

/// ルートの中のフォルダ `rel` を読み、除外と並び順を当てて返す。
///
/// `root` はプロジェクトのルートの絶対パス(登録したパス)。範囲の検査を通した実パスだけを読む。
///
/// # Errors
///
/// 読めないとき(権限が無いなど)。文言に絶対パスを載せない。
pub fn list_directory(
    fs: &dyn FileSystemPort,
    root: &Path,
    rel: &RelPath,
    rule: ExclusionRule,
) -> Result<DirListing, String> {
    let folder = match PathScopeGuard::new(fs, root).folder(rel) {
        Ok(folder) => folder,
        Err(ScopeError::NotFound) => return Ok(DirListing::NotFound),
        Err(ScopeError::NotAllowed(_)) => return Ok(DirListing::OutsideRoot),
        Err(ScopeError::Fs(FsError::NotADirectory(_))) => return Ok(DirListing::NotADirectory),
        Err(ScopeError::Fs(_)) => return Err(READ_FAILED.to_owned()),
    };
    let entries = match fs.read_dir(&folder.real) {
        Ok(entries) => entries,
        // 検査のあとに消えた・置き換わった。
        Err(FsError::NotFound(_)) => return Ok(DirListing::NotFound),
        Err(FsError::NotADirectory(_)) => return Ok(DirListing::NotADirectory),
        Err(_) => return Err(READ_FAILED.to_owned()),
    };
    let mut entries = visible_entries(entries, rule);
    sort_entries(&mut entries);
    Ok(DirListing::Listed { entries })
}

/// 一覧に出す項目だけを残す(除外の規則と、隠しファイル表示が OFF のときの作業用の名前)。順は変えない。
#[must_use]
pub fn visible_entries(entries: Vec<DirEntry>, rule: ExclusionRule) -> Vec<DirEntry> {
    rule.filter_listing(entries)
}

/// ツリーの操作。Tauri の管理状態に登録する。
pub struct Tree {
    sessions: SessionRegistry,
    fs: Arc<dyn FileSystemPort>,
}

impl Tree {
    #[must_use]
    pub fn new(sessions: SessionRegistry, fs: Arc<dyn FileSystemPort>) -> Self {
        Self { sessions, fs }
    }

    /// プロジェクト `id` のルートの中のフォルダ `rel` を読む(FR-013)。ルートが見つからないプロジェクトは
    /// 読まずに `NotFound`。止まりうるので、メインスレッドから呼ばない。
    ///
    /// # Errors
    ///
    /// その ID のプロジェクトが無いとき、読めないとき(文言。絶対パスを載せない)。
    pub fn list_directory(&self, id: &ProjectId, rel: &RelPath) -> Result<DirListing, String> {
        let card = self
            .sessions
            .snapshot()
            .projects
            .into_iter()
            .find(|p| &p.id == id)
            .ok_or_else(|| ProjectError::NotFound(id.clone()).to_string())?;
        if card.missing {
            // 見つからない(ゴミ箱の中を含む)プロジェクトの最後のパスは読まない。
            return Ok(DirListing::NotFound);
        }
        let root = self
            .sessions
            .project_root_path(id)
            .map_err(|e| e.to_string())?;
        list_directory(
            self.fs.as_ref(),
            &root,
            rel,
            ExclusionRule::new(card.show_hidden),
        )
    }

    /// 隠しファイル表示を切り替える(FR-017)。写しを送り、保存が予約される。
    ///
    /// # Errors
    ///
    /// その ID のプロジェクトが無いとき。
    pub fn set_show_hidden(&self, id: &ProjectId, show_hidden: bool) -> Result<(), String> {
        self.sessions
            .set_project_show_hidden(id, show_hidden)
            .map_err(|e| e.to_string())
    }
}
