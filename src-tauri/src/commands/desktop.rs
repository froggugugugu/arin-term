//! デスクトップとパスのコマンド(設計書 §5.2 の「デスクトップ」、§7.1「パスをターミナルへ」、§12、計画書 T42d)。
//! FR-015(パスのコピー)、FR-016・FR-024(ターミナルへの挿入)、FR-018(Finder で表示・開く)、FR-035。
//!
//! 画面からは「プロジェクト ID + ルート基準の相対パス(`RelPath`)」と、宛先のターミナルの ID と列挙の値だけを受け取る。
//! 絶対パスも、挿入する文字列も受け取らない。**ターミナルへ書く文字列はここで `insertion_text` から作る**(エスケープ済みで、
//! 末尾は空白 1 つ。Enter を含まない)。応答(`FileOpOutcome`)と失敗の文言に絶対パスを載せない
//! (「絶対パスをコピー」はペーストボードに書くだけ。ポートの失敗の文言は絶対パスを含むので使わない)。
//!
//! 対象は `PathScopeGuard` を通す: ルートの外を指すリンクを経由する経路は拒否する。項目そのもののリンクはたどらない
//! (Finder ではリンク自体を選んで表示する。「開く」はリンクを開かない【仮定】)。
//! 「開く」はふつうのファイルだけ(フォルダは画面が開閉する。`.app` もフォルダなので起動しない)。実行できるファイル
//! (実行の権限があるもの)は、既定のアプリ(ターミナル)で実行されうるので開かない【仮定】。実行の権限が無くても
//! 既定のアプリが実行・起動・インストールする種類(`UNOPENABLE_EXTENSIONS` の拡張子)も開かない。
//!
//! Finder・アプリへの受け渡し(`openURL:` は少し待つことがある)とペーストボードと PTY への書き込みは、待ってよいスレッドで行う。

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use arin_core::fileops::scope::{PathScopeGuard, ScopeError};
use arin_core::fileops::{FileOpOutcome, NotAllowedReason};
use arin_core::paths::format::{insertion_path_kind, insertion_text, path_text};
use arin_core::paths::{PathKind, RelPath};
use arin_core::ports::desktop::{DesktopError, DesktopPort};
use arin_core::ports::fs::{EntryKind, FileSystemPort};
use arin_core::ports::pasteboard::PasteboardPort;
use arin_core::project::{ProjectError, ProjectId};
use arin_core::terminal::{TerminalError, TerminalId};
use tauri::{AppHandle, Manager, Runtime};

use crate::sessions::{SessionError, SessionRegistry};

/// デスクトップとパスの操作。Tauri の管理状態に登録する。
pub struct Desktop {
    sessions: SessionRegistry,
    fs: Arc<dyn FileSystemPort>,
    desktop: Arc<dyn DesktopPort>,
    pasteboard: Arc<dyn PasteboardPort>,
}

impl Desktop {
    #[must_use]
    pub fn new(
        sessions: SessionRegistry,
        fs: Arc<dyn FileSystemPort>,
        desktop: Arc<dyn DesktopPort>,
        pasteboard: Arc<dyn PasteboardPort>,
    ) -> Self {
        Self {
            sessions,
            fs,
            desktop,
            pasteboard,
        }
    }

    /// 項目を Finder で選んだ状態で表示する(FR-018)。ルート(空の相対パス)も表示できる。リンクはリンク自体を選ぶ。
    ///
    /// # Errors
    ///
    /// その ID のプロジェクトが無いとき(文言。絶対パスを載せない)。以下の操作も同じ。
    pub fn reveal(&self, id: &ProjectId, path: &RelPath) -> Result<FileOpOutcome, String> {
        self.with_root(id, |guard, _root| {
            let target = match scoped_any(guard, path) {
                Ok(target) => target,
                Err(e) => return e.into(),
            };
            from_desktop(
                self.desktop.reveal_in_finder(&target),
                "Finder で表示できませんでした",
            )
        })
    }

    /// ファイルを既定のアプリで開く(FR-018)。フォルダ・リンク・実行できるファイル・ルートは開かない。
    ///
    /// # Errors
    ///
    /// その ID のプロジェクトが無いとき。
    pub fn open(&self, id: &ProjectId, path: &RelPath) -> Result<FileOpOutcome, String> {
        self.with_root(id, |guard, _root| {
            let entry = match guard.entry(path) {
                Ok(entry) => entry,
                Err(e) => return e.into(),
            };
            match entry.kind {
                EntryKind::File => {}
                EntryKind::Directory => return failed("フォルダは開けません"),
                EntryKind::DirectoryLink | EntryKind::FileLink => {
                    return failed("リンクは開けません");
                }
                EntryKind::Other => return failed("この種類の項目は開けません"),
            }
            if has_unopenable_extension(&entry.path) {
                return failed("この種類のファイルは開けません。Finder で表示から開いてください");
            }
            match is_executable(&entry.path) {
                Ok(false) => {}
                Ok(true) => return failed("実行できるファイルは開けません"),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    return FileOpOutcome::NotFound;
                }
                Err(_) => return failed("ファイルを扱えませんでした"),
            }
            from_desktop(
                self.desktop.open_with_default_app(&entry.path),
                "既定のアプリで開けませんでした",
            )
        })
    }

    /// 項目のパス(相対か絶対か)をペーストボードに文字列で書く(FR-015)。エスケープせず、末尾に空白を付けない。
    /// ルートの絶対パスは相対パスを空にして呼ぶ。ルートの相対パス(空の文字列)は書かずに `NotAllowed(RootItself)`。
    ///
    /// # Errors
    ///
    /// その ID のプロジェクトが無いとき。
    pub fn copy_path(
        &self,
        id: &ProjectId,
        path: &RelPath,
        kind: PathKind,
    ) -> Result<FileOpOutcome, String> {
        if path.is_root() && kind == PathKind::Relative {
            // 範囲の検査の前に決める(空の文字列でペーストボードを消さない)。プロジェクトの有無は確かめる。
            return self.with_root(id, |_, _| FileOpOutcome::NotAllowed {
                reason: NotAllowedReason::RootItself,
            });
        }
        self.with_root(id, |guard, root| {
            if let Err(e) = scoped_any(guard, path) {
                return e.into();
            }
            let Ok(text) = path_text(root, path, kind) else {
                return failed("パスを文字列にできませんでした");
            };
            match self.pasteboard.write_text(&text) {
                Ok(()) => done(),
                Err(_) => failed("クリップボードに書けませんでした"),
            }
        })
    }

    /// 項目のパスを、エスケープして末尾に空白を 1 つ付けてターミナル `terminal` に書く(FR-016・FR-024)。Enter は送らない。
    /// 宛先が同じプロジェクトのターミナルなら相対パス、違えば(持ち主の無いターミナルを含む)絶対パス(§15.2 #6 の A)。
    /// パスが 1 つも無ければ何もしない。宛先が無い・動いていないときは何も書かずに `Failed`。
    ///
    /// # Errors
    ///
    /// その ID のプロジェクトが無いとき。
    pub fn insert_path(
        &self,
        id: &ProjectId,
        paths: &[RelPath],
        terminal: &TerminalId,
    ) -> Result<FileOpOutcome, String> {
        self.with_root(id, |guard, root| {
            for path in paths {
                if let Err(e) = scoped_any(guard, path) {
                    return e.into();
                }
            }
            if paths.is_empty() {
                return done();
            }
            let snapshot = self.sessions.snapshot();
            let Some(destination) = snapshot.terminals.iter().find(|t| &t.id == terminal) else {
                return failed(TERMINAL_NOT_FOUND);
            };
            let kind = destination
                .project_id
                .as_ref()
                .map_or(PathKind::Absolute, |dest| insertion_path_kind(id, dest));
            let Ok(text) = insertion_text(root, paths, kind) else {
                return failed("パスを文字列にできませんでした");
            };
            // `insertion_text` は制御文字を含まない形にする。念のため、改行などが混じっていたら書かない。
            if text.chars().any(char::is_control) {
                return failed("パスを文字列にできませんでした");
            }
            match self.sessions.write(terminal, text.as_bytes()) {
                Ok(()) => done(),
                Err(SessionError::Terminal(TerminalError::NotFound(_))) => {
                    failed(TERMINAL_NOT_FOUND)
                }
                Err(SessionError::Terminal(
                    TerminalError::NotStarted(_) | TerminalError::Exited(_),
                )) => failed("挿入先のターミナルが動いていません"),
                Err(_) => failed("ターミナルに書き込めませんでした"),
            }
        })
    }

    /// ルートを引いて範囲の検査を作り、`op` を呼ぶ。ルートが見つからないプロジェクトは何もせずに `NotFound`。
    fn with_root(
        &self,
        id: &ProjectId,
        op: impl FnOnce(&PathScopeGuard<'_>, &Path) -> FileOpOutcome,
    ) -> Result<FileOpOutcome, String> {
        let Some(root) = self.root(id)? else {
            return Ok(FileOpOutcome::NotFound);
        };
        Ok(op(&PathScopeGuard::new(self.fs.as_ref(), &root), &root))
    }

    /// プロジェクト `id` のルートの絶対パス(登録したパス)。見つからない(ゴミ箱の中を含む)プロジェクトは `None`
    /// (`FileOps::root` と同じ)。
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

const TERMINAL_NOT_FOUND: &str = "挿入先のターミナルが見つかりません";

/// 実行の権限が無くても、既定のアプリが実行・起動・インストールする種類の拡張子(小文字。比べるときは大文字・小文字を
/// 区別しない)。ターミナルのスクリプト・Java・アプリと配布物・AppleScript・Automator・URL を開く書類。
pub const UNOPENABLE_EXTENSIONS: &[&str] = &[
    "terminal",
    "command",
    "tool",
    "jar",
    "app",
    "pkg",
    "mpkg",
    "dmg",
    "scpt",
    "scptd",
    "applescript",
    "workflow",
    "action",
    "webloc",
    "inetloc",
    "fileloc",
    "url",
];

/// 名前の拡張子が `UNOPENABLE_EXTENSIONS` のどれかか。`.command` のように名前全体が `.` + 拡張子のものも含める。
fn has_unopenable_extension(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let extension = match name.rsplit_once('.') {
        Some((_, extension)) => extension,
        None => return false,
    };
    UNOPENABLE_EXTENSIONS
        .iter()
        .any(|unopenable| extension.eq_ignore_ascii_case(unopenable))
}

/// ルート(フォルダとして)か既存の項目かを検査し、操作に使う絶対パス(ルートは実パス、項目は親の実パス + 名前)を返す。
fn scoped_any(guard: &PathScopeGuard<'_>, path: &RelPath) -> Result<PathBuf, ScopeError> {
    if path.is_root() {
        guard.folder(path).map(|folder| folder.real)
    } else {
        guard.entry(path).map(|entry| entry.path)
    }
}

/// 実行の権限(所有者・グループ・その他のどれか)があるか。リンクはたどらない(呼ぶ側がふつうのファイルと確かめてある)。
fn is_executable(path: &Path) -> std::io::Result<bool> {
    let meta = std::fs::symlink_metadata(path)?;
    Ok(meta.permissions().mode() & 0o111 != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 実行につながる拡張子は大文字小文字を区別せずに見分ける() {
        for name in [
            "a.command",
            "B.COMMAND",
            "c.WebLoc",
            ".command",
            "x.tar.dmg",
            "s.applescript",
        ] {
            assert!(
                has_unopenable_extension(Path::new("/x").join(name).as_path()),
                "{name}"
            );
        }
        for name in ["a.md", "command", "a.command.md", "a.commands", "a.", "url"] {
            assert!(
                !has_unopenable_extension(Path::new("/x").join(name).as_path()),
                "{name}"
            );
        }
    }

    #[test]
    fn 一覧は小文字で重なりが無い() {
        for (i, extension) in UNOPENABLE_EXTENSIONS.iter().enumerate() {
            assert_eq!(*extension, extension.to_ascii_lowercase());
            assert!(!extension.contains('.'));
            assert!(
                !UNOPENABLE_EXTENSIONS[..i].contains(extension),
                "{extension}"
            );
        }
    }
}

/// デスクトップのポートの結果を応答にする。失敗の文言(絶対パスを含む)は使わず、決まった文言にする。
fn from_desktop(result: Result<(), DesktopError>, message: &str) -> FileOpOutcome {
    match result {
        Ok(()) => done(),
        Err(DesktopError::NotFound(_)) => FileOpOutcome::NotFound,
        Err(DesktopError::Failed(_)) => failed(message),
    }
}

fn done() -> FileOpOutcome {
    FileOpOutcome::Done { select: None }
}

fn failed(message: &str) -> FileOpOutcome {
    FileOpOutcome::Failed {
        message: message.to_owned(),
    }
}

/// `op` を待ってよいスレッドで `Desktop` に対して行う。
async fn on_blocking_thread<R: Runtime>(
    app: AppHandle<R>,
    op: impl FnOnce(&Desktop) -> Result<FileOpOutcome, String> + Send + 'static,
) -> Result<FileOpOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || op(&app.state::<Desktop>()))
        .await
        .map_err(|e| format!("操作を完了できなかった: {e}"))?
}

/// 項目 `path` を Finder で表示する(ルートは空の文字列。FR-018)。
#[tauri::command]
pub async fn reveal_entry<R: Runtime>(
    app: AppHandle<R>,
    project_id: ProjectId,
    path: RelPath,
) -> Result<FileOpOutcome, String> {
    on_blocking_thread(app, move |d| d.reveal(&project_id, &path)).await
}

/// ファイル `path` を既定のアプリで開く(FR-018。フォルダは画面が開閉する)。
#[tauri::command]
pub async fn open_entry<R: Runtime>(
    app: AppHandle<R>,
    project_id: ProjectId,
    path: RelPath,
) -> Result<FileOpOutcome, String> {
    on_blocking_thread(app, move |d| d.open(&project_id, &path)).await
}

/// 項目 `path` のパス(`kind` が相対か絶対か)をクリップボードに書く(FR-015)。パスは画面に返さない。
#[tauri::command]
pub async fn copy_entry_path<R: Runtime>(
    app: AppHandle<R>,
    project_id: ProjectId,
    path: RelPath,
    kind: PathKind,
) -> Result<FileOpOutcome, String> {
    on_blocking_thread(app, move |d| d.copy_path(&project_id, &path, kind)).await
}

/// 項目 `paths` のパスを、Rust がエスケープしてターミナル `terminal_id` の入力行に書く(FR-016・FR-024)。Enter は送らない。
#[tauri::command]
pub async fn insert_entry_path<R: Runtime>(
    app: AppHandle<R>,
    project_id: ProjectId,
    paths: Vec<RelPath>,
    terminal_id: TerminalId,
) -> Result<FileOpOutcome, String> {
    on_blocking_thread(app, move |d| {
        d.insert_path(&project_id, &paths, &terminal_id)
    })
    .await
}
