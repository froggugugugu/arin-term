//! Markdown のエディタのコマンド(ADR-015 の 4・5・6、計画書 T81d・T81e)。
//!
//! 画面からは「プロジェクト ID + ルート基準の相対パス」か「エディタの ID」だけを受け取り、絶対パスは受け取らない・返さない。
//! ルートの絶対パスはここで `SessionRegistry` から引き、範囲の検査(`PathScopeGuard`)を通した実パスだけを
//! `DocumentPort`(本物は `arin_platform::fs::LocalDocuments`)に渡す。ポートの失敗の文言は絶対パスを含むので、
//! 画面にも標準エラーにも出さず、種類だけを決まった応答・文言に直す。
//!
//! - 開く(`open_document`): 拡張子 → ルートの範囲 → 項目の種類(リンク・フォルダは開かない)→ 読む(1 MiB まで)→
//!   文字の検査(UTF-8・NUL)の順に調べ、通ったらエディタのタブを開いて本文を返す。同じファイルが開いていれば表示だけ
//! - 保存(`save_document`): 本人の保存の操作のときだけ画面が呼ぶ。開いたとき(最後に保存・読み直したとき)の版と比べ、
//!   外で変わっていれば `overwriteChanged` が真のときだけ置き換える。無ければ `overwriteChanged` が真のときだけ作る(上書きしない)。
//!   BOM は開いたときにあれば付け直す。保存できたら版を持ち直し、未保存の印を消す
//! - 確認(`check_document`): 外で変わったかを調べる。`withText` が真なら新しい本文を返して版を持ち直す
//! - 片付けられなかった保存の一時ファイル(`.arin-save-…`)は消さずに残し、標準エラーに `[arin:editor]` で場所を出す
//!   (`[arin:fileops]` と同じ扱い)
//!
//! 読み書きは止まりうるので、ファイルに触れるコマンドは待ってよいスレッドで行う。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use arin_core::editor::{
    CheckDocumentOutcome, DOCUMENT_LIMIT_BYTES, DocumentError, DocumentNotAllowedReason,
    EditorError, EditorId, EditorTab, LineEnding, OpenDocumentOutcome, SaveDocumentOutcome,
    decode_document, encode_document, is_markdown_path, same_content,
};
use arin_core::fileops::NotAllowedReason;
use arin_core::fileops::scope::{PathScopeGuard, ScopeError};
use arin_core::paths::RelPath;
use arin_core::ports::document::{DocumentAccessError, DocumentPort, ReplaceOutcome};
use arin_core::ports::fs::{EntryKind, FileSystemPort};
use arin_core::project::{ProjectError, ProjectId};
use arin_core::workspace::CloseEditorOutcome;
use tauri::{AppHandle, Manager, Runtime, State};

use crate::sessions::SessionRegistry;

/// 標準エラーの頭の文字列(保存の一時ファイルを片付けられずに残した)。
pub const EDITOR_LOG_PREFIX: &str = "[arin:editor]";

const ROOT_MISSING: &str = "プロジェクトのフォルダが見つかりません";
const OPEN_FAILED: &str = "ファイルを開けませんでした";
const NOT_A_FILE: &str = "この種類の項目は開けません";
const SAVE_FAILED: &str = "保存できませんでした";
const SAVE_NOT_A_FILE: &str = "ふつうのファイルではなくなったので保存できません";
const SAVE_OUTSIDE: &str = "プロジェクトのフォルダの外になったので保存できません";
const CHECK_FAILED: &str = "ファイルを調べられませんでした";

/// 保存の一時ファイルを片付けられずに残したことを、場所つきの 1 行で知らせる(`LocalDocuments` の知らせ先)。
pub fn report_save_leftover(path: &Path) {
    eprintln!("{}", save_leftover_line(path));
}

fn save_leftover_line(path: &Path) -> String {
    format!(
        "{EDITOR_LOG_PREFIX} 保存の一時ファイルを片付けられませんでした: {}",
        path.display()
    )
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// 文書の置き場所を引いた結果。
enum Located {
    /// 操作に使う絶対パス(親の実パス + 名前)。ファイルがあるとは限らない。
    Path(PathBuf),
    /// 親のフォルダが無い(ファイルも無い)。
    Missing,
    /// ルートが見つからない・ルートの外になったなど(画面に出す文言)。
    Failed(&'static str),
}

/// エディタの文書の操作。Tauri の管理状態に登録する。
pub struct Editors {
    sessions: SessionRegistry,
    fs: Arc<dyn FileSystemPort>,
    documents: Arc<dyn DocumentPort>,
    /// エディタごとの、読んだ文書の先頭に BOM があったか(保存で付け直す)。写しには載せない。
    boms: Mutex<HashMap<EditorId, bool>>,
}

impl Editors {
    #[must_use]
    pub fn new(
        sessions: SessionRegistry,
        fs: Arc<dyn FileSystemPort>,
        documents: Arc<dyn DocumentPort>,
    ) -> Self {
        Self {
            sessions,
            fs,
            documents,
            boms: Mutex::new(HashMap::new()),
        }
    }

    /// `project` の `path` をエディタで開く(ADR-015 の 4・5)。
    ///
    /// # Errors
    ///
    /// その ID のプロジェクトが無いとき(文言。絶対パスを載せない)。
    pub fn open(&self, project: &ProjectId, path: &RelPath) -> Result<OpenDocumentOutcome, String> {
        let root = self.root(project)?;
        if !is_markdown_path(path) {
            return Ok(not_allowed(DocumentNotAllowedReason::NotMarkdown));
        }
        let Some(root) = root else {
            return Ok(open_failed(ROOT_MISSING));
        };
        if let Some(id) = self.find_open(project, path) {
            self.sessions
                .select_editor(&id)
                .map_err(|e| e.to_string())?;
            return Ok(OpenDocumentOutcome::AlreadyOpen { editor_id: id });
        }
        let guard = PathScopeGuard::new(self.fs.as_ref(), &root);
        let entry = match guard.entry(path) {
            Ok(entry) => entry,
            Err(ScopeError::NotFound) => return Ok(OpenDocumentOutcome::NotFound),
            Err(ScopeError::NotAllowed(NotAllowedReason::OutsideRoot)) => {
                return Ok(not_allowed(DocumentNotAllowedReason::OutsideRoot));
            }
            Err(_) => return Ok(open_failed(OPEN_FAILED)),
        };
        match entry.kind {
            EntryKind::File => {}
            EntryKind::Directory => return Ok(not_allowed(DocumentNotAllowedReason::Directory)),
            EntryKind::DirectoryLink | EntryKind::FileLink => {
                return Ok(not_allowed(DocumentNotAllowedReason::Link));
            }
            EntryKind::Other => return Ok(open_failed(NOT_A_FILE)),
        }
        let read = match self.documents.read(&entry.path) {
            Ok(read) => read,
            Err(e) => return Ok(open_error(&e)),
        };
        let decoded = match decode_document(&read.bytes) {
            Ok(decoded) => decoded,
            Err(DocumentError::TooLarge { .. }) => {
                return Ok(not_allowed(DocumentNotAllowedReason::TooLarge));
            }
            Err(DocumentError::NotText) => {
                return Ok(not_allowed(DocumentNotAllowedReason::NotText));
            }
        };
        let (editor_id, created) = self
            .sessions
            .open_editor(project, path.clone(), read.stamp, read.read_only)
            .map_err(|e| e.to_string())?;
        if !created {
            // 調べているあいだに同じファイルが開かれた。そちらを表示しただけ。
            return Ok(OpenDocumentOutcome::AlreadyOpen { editor_id });
        }
        self.set_bom(&editor_id, decoded.bom);
        Ok(OpenDocumentOutcome::Opened {
            editor_id,
            text: decoded.text,
            line_ending: decoded.line_ending,
            read_only: read.read_only,
        })
    }

    /// エディタ `id` の文書を `text` で保存する(ADR-015 の 5)。`overwrite_changed` は本人が確認で
    /// 「上書き保存」(外で変わっていた)か「同じ名前で作る」(無かった)を選んだとき真。
    ///
    /// # Errors
    ///
    /// その ID のエディタが無いとき。
    pub fn save(
        &self,
        id: &EditorId,
        text: &str,
        overwrite_changed: bool,
    ) -> Result<SaveDocumentOutcome, String> {
        let tab = self.tab(id)?;
        if tab.read_only.is_some() {
            return Ok(SaveDocumentOutcome::ReadOnly);
        }
        let bytes = encode_document(text, self.bom(id));
        if bytes.len() as u64 > DOCUMENT_LIMIT_BYTES {
            return Ok(SaveDocumentOutcome::TooLarge);
        }
        let path = match self.locate(&tab)? {
            Located::Path(path) => path,
            Located::Missing => return Ok(SaveDocumentOutcome::Missing),
            Located::Failed(message) => return Ok(save_failed(message)),
        };
        let expected = (!overwrite_changed).then_some(&tab.stamp);
        let stamp = match self.documents.replace(&path, &bytes, expected) {
            Ok(ReplaceOutcome::Replaced(stamp)) => stamp,
            Ok(ReplaceOutcome::Changed(_)) => return Ok(SaveDocumentOutcome::ChangedOutside),
            Ok(ReplaceOutcome::Missing) if overwrite_changed => {
                match self.documents.create_new(&path, &bytes) {
                    Ok(stamp) => stamp,
                    Err(e) => return Ok(save_error(&e)),
                }
            }
            Ok(ReplaceOutcome::Missing) => return Ok(SaveDocumentOutcome::Missing),
            Err(e) => return Ok(save_error(&e)),
        };
        self.sessions
            .mark_editor_synced(id, stamp)
            .map_err(|e| e.to_string())?;
        Ok(SaveDocumentOutcome::Saved)
    }

    /// エディタ `id` の文書が外で変わったかを調べる(ADR-015 の 5)。`with_text` が真で変わっていれば、
    /// 新しい本文を返して版を持ち直す(未保存の印も消す。画面は未保存の変更が無いときだけ真で呼ぶ)。
    ///
    /// # Errors
    ///
    /// その ID のエディタが無いとき、調べられなかったとき。
    pub fn check(&self, id: &EditorId, with_text: bool) -> Result<CheckDocumentOutcome, String> {
        let tab = self.tab(id)?;
        let path = match self.locate(&tab)? {
            Located::Path(path) => path,
            Located::Missing | Located::Failed(_) => return Ok(CheckDocumentOutcome::Missing),
        };
        let read = match self.documents.read(&path) {
            Ok(read) => read,
            Err(
                DocumentAccessError::NotFound(_)
                | DocumentAccessError::Link(_)
                | DocumentAccessError::Directory(_)
                | DocumentAccessError::NotAFile(_),
            ) => return Ok(CheckDocumentOutcome::Missing),
            Err(DocumentAccessError::TooLarge { .. }) => return Ok(changed_without_text()),
            Err(_) => return Err(CHECK_FAILED.to_owned()),
        };
        if same_content(&tab.stamp, &read.stamp) {
            if tab.stamp != read.stamp {
                // `touch` だけ。版を新しくする(次に比べるとき中身を読まずに済む)。
                self.sessions
                    .set_editor_stamp(id, read.stamp)
                    .map_err(|e| e.to_string())?;
            }
            return Ok(CheckDocumentOutcome::Unchanged);
        }
        let Ok(decoded) = decode_document(&read.bytes) else {
            return Ok(changed_without_text());
        };
        if !with_text {
            return Ok(CheckDocumentOutcome::Changed {
                text: None,
                line_ending: decoded.line_ending,
            });
        }
        self.set_bom(id, decoded.bom);
        self.sessions
            .mark_editor_synced(id, read.stamp)
            .map_err(|e| e.to_string())?;
        Ok(CheckDocumentOutcome::Changed {
            text: Some(decoded.text),
            line_ending: decoded.line_ending,
        })
    }

    /// エディタを閉じる。閉じたら BOM の覚えも捨てる。
    ///
    /// # Errors
    ///
    /// その ID のエディタが無いとき。
    pub fn close(&self, id: &EditorId, discard: bool) -> Result<CloseEditorOutcome, String> {
        let outcome = self
            .sessions
            .close_editor(id, discard)
            .map_err(|e| e.to_string())?;
        if outcome == CloseEditorOutcome::Closed {
            lock(&self.boms).remove(id);
        }
        Ok(outcome)
    }

    fn tab(&self, id: &EditorId) -> Result<EditorTab, String> {
        self.sessions
            .editor(id)
            .ok_or_else(|| EditorError::NotFound(id.clone()).to_string())
    }

    fn bom(&self, id: &EditorId) -> bool {
        lock(&self.boms).get(id).copied().unwrap_or(false)
    }

    /// BOM の有無を覚える。ついでに、閉じた(⌘W・プロジェクトの削除で外れた)エディタの覚えを捨てる。
    fn set_bom(&self, id: &EditorId, bom: bool) {
        let open: Vec<EditorId> = self
            .sessions
            .snapshot()
            .editors
            .into_iter()
            .map(|e| e.id)
            .collect();
        let mut boms = lock(&self.boms);
        boms.retain(|known, _| open.contains(known));
        boms.insert(id.clone(), bom);
    }

    /// 同じプロジェクトの同じ相対パスで開いているエディタ。
    fn find_open(&self, project: &ProjectId, path: &RelPath) -> Option<EditorId> {
        self.sessions
            .snapshot()
            .editors
            .into_iter()
            .find(|e| &e.project_id == project && &e.path == path)
            .map(|e| e.id)
    }

    /// エディタの文書の置き場所(親のフォルダを範囲の検査に通した実パス + 名前)。ファイルの有無は見ない。
    fn locate(&self, tab: &EditorTab) -> Result<Located, String> {
        let Some(root) = self.root(&tab.project)? else {
            return Ok(Located::Failed(ROOT_MISSING));
        };
        let Some((parent, name)) = tab.path.split_parent() else {
            return Ok(Located::Failed(SAVE_NOT_A_FILE));
        };
        let guard = PathScopeGuard::new(self.fs.as_ref(), &root);
        Ok(match guard.folder(&parent) {
            Ok(folder) => Located::Path(folder.real.join(name)),
            Err(ScopeError::NotFound) => Located::Missing,
            Err(ScopeError::NotAllowed(_)) => Located::Failed(SAVE_OUTSIDE),
            Err(ScopeError::Fs(_)) => Located::Missing,
        })
    }

    /// プロジェクト `id` のルートの絶対パス(登録したパス)。見つからないプロジェクトは `None`(`Desktop::root` と同じ)。
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

fn not_allowed(reason: DocumentNotAllowedReason) -> OpenDocumentOutcome {
    OpenDocumentOutcome::NotAllowed { reason }
}

fn open_failed(message: &str) -> OpenDocumentOutcome {
    OpenDocumentOutcome::Failed {
        message: message.to_owned(),
    }
}

fn save_failed(message: &str) -> SaveDocumentOutcome {
    SaveDocumentOutcome::Failed {
        message: message.to_owned(),
    }
}

fn changed_without_text() -> CheckDocumentOutcome {
    CheckDocumentOutcome::Changed {
        text: None,
        line_ending: LineEnding::Lf,
    }
}

/// 読めなかった理由を応答にする(文言は使わない。絶対パスを含むため)。
fn open_error(error: &DocumentAccessError) -> OpenDocumentOutcome {
    match error {
        DocumentAccessError::NotFound(_) => OpenDocumentOutcome::NotFound,
        DocumentAccessError::Link(_) => not_allowed(DocumentNotAllowedReason::Link),
        DocumentAccessError::Directory(_) => not_allowed(DocumentNotAllowedReason::Directory),
        DocumentAccessError::TooLarge { .. } => not_allowed(DocumentNotAllowedReason::TooLarge),
        DocumentAccessError::NotAFile(_) => open_failed(NOT_A_FILE),
        DocumentAccessError::AlreadyExists(_)
        | DocumentAccessError::ReadOnly(_)
        | DocumentAccessError::Io(_) => open_failed(OPEN_FAILED),
    }
}

/// 保存できなかった理由を応答にする(文言は使わない。絶対パスを含むため。一時ファイルを残したときの場所は
/// 知らせ先 `report_save_leftover` が標準エラーに出している)。
fn save_error(error: &DocumentAccessError) -> SaveDocumentOutcome {
    match error {
        DocumentAccessError::NotFound(_) => SaveDocumentOutcome::Missing,
        // 作ろうとしたら同じ名前ができていた(外で作られた)。本人が「上書き保存」を選べば置き換える。
        DocumentAccessError::AlreadyExists(_) => SaveDocumentOutcome::ChangedOutside,
        DocumentAccessError::ReadOnly(_) => SaveDocumentOutcome::ReadOnly,
        DocumentAccessError::TooLarge { .. } => SaveDocumentOutcome::TooLarge,
        DocumentAccessError::Link(_)
        | DocumentAccessError::Directory(_)
        | DocumentAccessError::NotAFile(_) => save_failed(SAVE_NOT_A_FILE),
        DocumentAccessError::Io(_) => save_failed(SAVE_FAILED),
    }
}

/// `op` を待ってよいスレッドで `Editors` に対して行う。
async fn on_blocking_thread<R: Runtime, T: Send + 'static>(
    app: AppHandle<R>,
    op: impl FnOnce(&Editors) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || op(&app.state::<Editors>()))
        .await
        .map_err(|e| format!("操作を完了できなかった: {e}"))?
}

/// `path`(`.md`・`.markdown`)をエディタのタブで開いて表示する。同じファイルが開いていればそれを表示するだけ。
#[tauri::command]
pub async fn open_document<R: Runtime>(
    app: AppHandle<R>,
    project_id: ProjectId,
    path: RelPath,
) -> Result<OpenDocumentOutcome, String> {
    on_blocking_thread(app, move |e| e.open(&project_id, &path)).await
}

/// エディタの文書を保存する(本人の保存の操作のときだけ呼ぶ)。
#[tauri::command]
pub async fn save_document<R: Runtime>(
    app: AppHandle<R>,
    editor_id: EditorId,
    text: String,
    overwrite_changed: bool,
) -> Result<SaveDocumentOutcome, String> {
    on_blocking_thread(app, move |e| e.save(&editor_id, &text, overwrite_changed)).await
}

/// エディタの文書が外で変わったかを調べる。`with_text` が真なら新しい本文を返す(未保存の変更が無いときだけ真にする)。
#[tauri::command]
pub async fn check_document<R: Runtime>(
    app: AppHandle<R>,
    editor_id: EditorId,
    with_text: bool,
) -> Result<CheckDocumentOutcome, String> {
    on_blocking_thread(app, move |e| e.check(&editor_id, with_text)).await
}

/// 未保存の印を替える(画面は変わり目だけ送る)。変わったら写しが届く。
#[tauri::command]
pub fn set_document_dirty(
    sessions: State<'_, SessionRegistry>,
    editor_id: EditorId,
    dirty: bool,
) -> Result<(), String> {
    sessions
        .set_editor_dirty(&editor_id, dirty)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// エディタのタブを表示する(タブバーのクリック)。
#[tauri::command]
pub fn select_editor(
    sessions: State<'_, SessionRegistry>,
    editor_id: EditorId,
) -> Result<(), String> {
    sessions
        .select_editor(&editor_id)
        .map_err(|e| e.to_string())
}

/// エディタを閉じる。未保存の変更があり `discard` が偽なら閉じずに `needs-confirmation`。
#[tauri::command]
pub fn close_editor(
    editors: State<'_, Editors>,
    editor_id: EditorId,
    discard: bool,
) -> Result<CloseEditorOutcome, String> {
    editors.close(&editor_id, discard)
}

/// エディタの Vim のキー操作を切り替える(ADR-015 の 6)。変わったら写しが届き、`ui.editorVim` に残る。
#[tauri::command]
pub fn set_editor_vim(sessions: State<'_, SessionRegistry>, enabled: bool) {
    sessions.set_editor_vim(enabled);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 片付けられなかった保存の一時ファイルは頭の文字列を付けた_1_行で知らせる() {
        assert_eq!(
            save_leftover_line(Path::new("/x/app/.arin-save-1-0")),
            "[arin:editor] 保存の一時ファイルを片付けられませんでした: /x/app/.arin-save-1-0"
        );
    }
}
