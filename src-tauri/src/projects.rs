//! プロジェクトの操作の手順(計画書 T33c、FR-001・FR-002・FR-003・FR-007・FR-010・FR-059・FR-064)。
//!
//! フォルダ選択のダイアログ・Finder からのドロップ・画面のコマンドから呼ばれ、ポート(正規化・ブックマーク・乱数・
//! ペーストボード)を順に使って `SessionRegistry`(中の `Workspace`)を操作する。規則は `arin-core` が持つ。
//!
//! - 選んだパス・落としたパスは画面を経由させず、ここで登録まで行う(設計書 §12。画面はダイアログを開くよう頼むだけ)
//! - ダイアログは [`FolderPicker`] の裏に置く。本物([`NativeFolderPicker`])は `NSOpenPanel` をメインスレッドで開き、
//!   テストは偽物を渡す(GUI を出さない)
//! - 状態が変わると `SessionRegistry` が写しを送り、保存は自動で予約される(`lib.rs` の受け取り手)

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;

use arin_core::ports::pasteboard::PasteboardPort;
use arin_core::ports::{BookmarkPort, IdSource, RootError, RootLocatorPort};
use arin_core::project::{ProjectColor, ProjectError, ProjectId, RootReference};
use arin_core::workspace::{AddProjectOutcome, RemoveProjectOutcome, WorkspaceError};
use arin_platform::desktop::MacPasteboard;
use arin_platform::system::UrandomIdSource;
use tauri::{AppHandle, Runtime};

use crate::sessions::{SessionError, SessionRegistry};

/// 乱数で作った ID が既存と重なったときに取り直す回数。
const ID_ATTEMPTS: usize = 4;

/// ダイアログを何のために開くか(決定のボタンの文言が変わる)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderPurpose {
    /// フォルダを登録する(FR-001)。
    Add,
    /// 見つからないプロジェクトをつなぎ直す(FR-007)。
    Relink,
}

/// 本人にフォルダを 1 つ選ばせる口。
pub trait FolderPicker: Send + Sync {
    /// 選ばれたフォルダのパスを返す。取り消されたら `None`。閉じられるまで待つので、メインスレッドから呼ばない。
    ///
    /// # Errors
    ///
    /// ダイアログを開けなかったとき(理由の文言)。
    fn pick_folder(&self, purpose: FolderPurpose) -> Result<Option<PathBuf>, String>;
}

/// 本物のダイアログ(`NSOpenPanel`)。メインスレッドに開く処理を頼み、閉じられるまで待つ。
pub struct NativeFolderPicker<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> NativeFolderPicker<R> {
    #[must_use]
    pub fn new(app: AppHandle<R>) -> Self {
        Self { app }
    }
}

impl<R: Runtime> FolderPicker for NativeFolderPicker<R> {
    fn pick_folder(&self, purpose: FolderPurpose) -> Result<Option<PathBuf>, String> {
        let prompt = match purpose {
            FolderPurpose::Add => "追加",
            FolderPurpose::Relink => "再リンク",
        };
        let (tx, rx) = mpsc::channel();
        self.app
            .run_on_main_thread(move || {
                let _ = tx.send(arin_platform::desktop::choose_folder(prompt));
            })
            .map_err(|e| format!("フォルダ選択のダイアログを開けなかった: {e}"))?;
        rx.recv()
            .map_err(|_| "フォルダ選択のダイアログが結果を返さなかった".to_owned())?
            .map_err(text)
    }
}

/// プロジェクトの操作で差し替えられるポート(テストは偽物を渡す)。
#[derive(Clone)]
pub struct ProjectPorts {
    /// ID の乱数。
    pub ids: Arc<dyn IdSource>,
    /// 「絶対パスをコピー」の書き込み先。
    pub pasteboard: Arc<dyn PasteboardPort>,
    /// フォルダ選択のダイアログ。`None` なら本物([`NativeFolderPicker`])を組み立てのときに作る。
    pub picker: Option<Arc<dyn FolderPicker>>,
}

impl ProjectPorts {
    /// OS の実装で作る(乱数は `/dev/urandom`、ペーストボードはユーザーのクリップボード、ダイアログは `NSOpenPanel`)。
    #[must_use]
    pub fn native() -> Self {
        Self {
            ids: Arc::new(UrandomIdSource::new()),
            pasteboard: Arc::new(MacPasteboard::general()),
            picker: None,
        }
    }
}

/// プロジェクトの操作。Tauri の管理状態に登録する。
pub struct Projects {
    sessions: SessionRegistry,
    roots: Arc<dyn RootLocatorPort>,
    bookmarks: Arc<dyn BookmarkPort>,
    ids: Arc<dyn IdSource>,
    pasteboard: Arc<dyn PasteboardPort>,
    picker: Arc<dyn FolderPicker>,
}

/// プロジェクトの操作の失敗。画面へは文言で返す。
fn text(e: impl std::fmt::Display) -> String {
    e.to_string()
}

impl Projects {
    #[must_use]
    pub fn new(
        sessions: SessionRegistry,
        roots: Arc<dyn RootLocatorPort>,
        bookmarks: Arc<dyn BookmarkPort>,
        ids: Arc<dyn IdSource>,
        pasteboard: Arc<dyn PasteboardPort>,
        picker: Arc<dyn FolderPicker>,
    ) -> Self {
        Self {
            sessions,
            roots,
            bookmarks,
            ids,
            pasteboard,
            picker,
        }
    }

    /// ダイアログでフォルダを選ばせて登録する(FR-001)。取り消されたら何も変えず `Cancelled`。
    /// 閉じられるまで待つ。メインスレッドから呼ばない。
    ///
    /// # Errors
    ///
    /// ダイアログを開けないとき、選んだものを登録できないとき(文言)。
    pub fn add_by_dialog(&self) -> Result<AddProjectOutcome, String> {
        match self.picker.pick_folder(FolderPurpose::Add)? {
            None => Ok(AddProjectOutcome::Cancelled),
            Some(path) => self.add_folder(&path).map_err(text),
        }
    }

    /// Finder から落とされたパスのうち、フォルダだけを順に登録する(FR-001、計画書 M2 の【仮定】7)。
    /// ファイル・無いもの・登録できないものは黙って飛ばす。登録した(または既存を選んだ)結果を返す。
    pub fn add_dropped(&self, paths: &[PathBuf]) -> Vec<AddProjectOutcome> {
        paths
            .iter()
            .filter_map(|path| self.add_folder(path).ok())
            .collect()
    }

    /// 1 つのフォルダを登録する。正規化したパスにブックマークを作り(作れなければ無しで登録し、次の起動で作り直す)、
    /// 乱数で ID を作る。同じフォルダが登録済みならそれを選ぶ(FR-010)。
    fn add_folder(&self, path: &Path) -> Result<AddProjectOutcome, ProjectFailure> {
        let root = self.root_reference(path)?;
        for _ in 0..ID_ATTEMPTS {
            let bytes = self
                .ids
                .random_16()
                .map_err(|e| ProjectFailure(e.to_string()))?;
            match self
                .sessions
                .register_project(ProjectId::from_random_bytes(bytes), root.clone())
            {
                Err(SessionError::Workspace(WorkspaceError::Project(
                    ProjectError::DuplicateId(_),
                ))) => {}
                other => return other.map_err(ProjectFailure::from),
            }
        }
        Err(ProjectFailure(
            "プロジェクトの ID を作れなかった(乱数が既存の ID と重なり続けた)".into(),
        ))
    }

    /// フォルダを調べて正規化し、ブックマークを作る。フォルダでなければ失敗。
    fn root_reference(&self, path: &Path) -> Result<RootReference, ProjectFailure> {
        let canonical = self
            .roots
            .canonical_dir(path)
            .map_err(|e: RootError| ProjectFailure(e.to_string()))?;
        let bookmark = self.bookmarks.create(&canonical).ok();
        Ok(RootReference::new(canonical, bookmark))
    }

    /// プロジェクトを選ぶ(FR-002)。
    ///
    /// # Errors
    ///
    /// その ID が無いとき。
    pub fn select(&self, id: &ProjectId) -> Result<(), String> {
        self.sessions.select_project(id).map_err(text)
    }

    /// プロジェクトを一覧から外す(FR-003)。動作中があり `confirmed` が偽なら確認を求める。
    ///
    /// # Errors
    ///
    /// その ID が無いとき。
    pub fn remove(&self, id: &ProjectId, confirmed: bool) -> Result<RemoveProjectOutcome, String> {
        self.sessions.remove_project(id, confirmed).map_err(text)
    }

    /// 色を変える(FR-059)。
    ///
    /// # Errors
    ///
    /// その ID が無いとき。
    pub fn set_color(&self, id: &ProjectId, color: ProjectColor) -> Result<(), String> {
        self.sessions.set_project_color(id, color).map_err(text)
    }

    /// ダイアログでフォルダを選ばせてつなぎ直す(FR-007)。取り消されたら何も変えず `false`、つなぎ直したら `true`。
    /// 閉じられるまで待つ。メインスレッドから呼ばない。
    ///
    /// # Errors
    ///
    /// その ID が無いとき(ダイアログを開く前に確かめる)、ダイアログを開けないとき、選んだものにつなぎ直せないとき
    /// (フォルダでない、別のプロジェクトのフォルダと同じ、など)。
    pub fn relink_by_dialog(&self, id: &ProjectId) -> Result<bool, String> {
        self.sessions.project_root_path(id).map_err(text)?;
        let Some(path) = self.picker.pick_folder(FolderPurpose::Relink)? else {
            return Ok(false);
        };
        let root = self.root_reference(&path).map_err(text)?;
        self.sessions.relink_project(id, root).map_err(text)?;
        Ok(true)
    }

    /// ルートの絶対パスをペーストボードに書く(FR-064)。選択は変えない。
    ///
    /// # Errors
    ///
    /// その ID が無いとき、ペーストボードに書けないとき。
    pub fn copy_path(&self, id: &ProjectId) -> Result<(), String> {
        let path = self.sessions.project_root_path(id).map_err(text)?;
        let path = path
            .to_str()
            .ok_or_else(|| "UTF-8 でないパスはコピーできない".to_owned())?;
        self.pasteboard.write_text(path).map_err(text)
    }
}

/// 登録・再リンクの途中の失敗(文言)。
#[derive(Debug)]
struct ProjectFailure(String);

impl std::fmt::Display for ProjectFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<SessionError> for ProjectFailure {
    fn from(e: SessionError) -> Self {
        Self(e.to_string())
    }
}
