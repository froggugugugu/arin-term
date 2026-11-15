//! ファイル操作の手順(設計書 §5.1 の `FileOperationService`、§5.2、§7.1「ファイル操作」、計画書 T39b)。
//!
//! 新規作成・名称変更・移動・コピー・ゴミ箱・ペーストボードへのコピーと切り取り・ペースト。どの操作も:
//!
//! 1. 名前を検査し(`validate_name`)、範囲を検査する(`PathScopeGuard`。操作のたびに作り直す)。ルート自体と、
//!    相対パスだけで決まる子孫への移動は、ポートを 1 回も呼ばずに拒否する
//! 2. 検査を通った**実パス**をポートに渡す。対象の存在は実行の直前に調べ(範囲の検査の `entry_kind`)、
//!    そのあと消えていればポートの「見つからない」で、同じ名前ができていればポートの「既にある」で判定する
//!    (事前の確認だけに頼らない。FR-036)
//! 3. 衝突したら、対処が無ければ何も変えずに `Conflict` を返す。画面が確認を出し、対処を付けて呼び直す(§5.2)
//!    - 置き換える: 既存を**ゴミ箱に入れてから**置く。ゴミ箱に入れられなければ置かずに失敗を返す。
//!      ゴミ箱に入れたあとにまた同じ名前ができていたら、もう一度 `Conflict` を返す
//!    - 両方残す: 置く先のフォルダの中の名前から番号付きの名前を作って置く(`keep_both_name`)
//!
//! 消す操作と上書きする操作は使わない(ポートにも無い。FR-028・FR-030)。移動は 1 回の移動で行い、
//! コピーしてから元を消すことはしない(別のボリュームへの移動は失敗。【仮定】14)。
//!
//! 応答(`FileOpOutcome`)には絶対パスを載せない。ポートの失敗の文言(絶対パスを含む)は使わず、決まった文言にする。

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use super::clipboard::{FileClipboard, PasteSource};
use super::name::validate_name;
use super::scope::{PathScopeGuard, ScopedEntry, ScopedFolder};
use super::types::{ConflictResolution, FileOpOutcome, NewEntryKind, NotAllowedReason};
use super::unique::keep_both_name;
use crate::paths::RelPath;
use crate::ports::fs::{EntryKind, FileSystemPort, FsError};
use crate::ports::pasteboard::FilePasteboardPort;
use crate::project::ProjectId;

/// 置く操作(作る・移す・写す)。置く先の絶対パスを受け、既にあれば `AlreadyExists` で何も変えずに失敗する。
type PlaceOp<'o> = dyn Fn(&Path) -> Result<(), FsError> + 'o;

/// 移すか写すか。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Transfer {
    Move,
    Copy,
}

/// 置く・移す・写すときの元の項目(検査を通ったもの)。
struct Source {
    /// 操作に使う絶対パス(親の実パス + 名前)。
    path: PathBuf,
    /// 種類(リンクはたどらない)。
    kind: EntryKind,
}

impl From<ScopedEntry> for Source {
    fn from(entry: ScopedEntry) -> Self {
        Self {
            path: entry.path,
            kind: entry.kind,
        }
    }
}

/// ファイル操作の手順。ファイルシステムのポートを借りるだけで状態を持たない(操作ごとに作ってよい)。
pub struct FileOperationService<'a> {
    fs: &'a dyn FileSystemPort,
}

impl<'a> FileOperationService<'a> {
    /// `fs` を使って操作する。
    #[must_use]
    pub fn new(fs: &'a dyn FileSystemPort) -> Self {
        Self { fs }
    }

    /// `root` の中のフォルダ `parent` に、空のファイルかフォルダ `name` を新しく作る(FR-025)。
    /// 済めば作った項目を選ぶ相対パスを返す。
    #[must_use]
    pub fn create(
        &self,
        root: &Path,
        parent: &RelPath,
        name: &str,
        kind: NewEntryKind,
        resolution: Option<ConflictResolution>,
    ) -> FileOpOutcome {
        if let Err(reason) = validate_name(name) {
            return FileOpOutcome::InvalidName { reason };
        }
        let guard = PathScopeGuard::new(self.fs, root);
        let folder = match guard.folder(parent) {
            Ok(folder) => folder,
            Err(error) => return error.into(),
        };
        let (entry_kind, op): (EntryKind, &PlaceOp<'_>) = match kind {
            NewEntryKind::File => (EntryKind::File, &|to| self.fs.create_file(to)),
            NewEntryKind::Folder => (EntryKind::Directory, &|to| self.fs.create_dir(to)),
        };
        self.place(&folder, name, entry_kind, None, resolution, op)
    }

    /// `root` の中の既存の項目 `target` の名前を `new_name` に変える(FR-026)。
    /// 大文字・小文字だけの変更は同じ項目なので衝突にしない(ポートの約束。【仮定】15)。
    #[must_use]
    pub fn rename(
        &self,
        root: &Path,
        target: &RelPath,
        new_name: &str,
        resolution: Option<ConflictResolution>,
    ) -> FileOpOutcome {
        if let Err(reason) = validate_name(new_name) {
            return FileOpOutcome::InvalidName { reason };
        }
        let guard = PathScopeGuard::new(self.fs, root);
        let entry = match guard.entry(target) {
            Ok(entry) => entry,
            Err(error) => return error.into(),
        };
        let Some((parent_rel, old_name)) = target.split_parent() else {
            // `entry` はルートを拒否するので来ない。
            return FileOpOutcome::NotAllowed {
                reason: NotAllowedReason::RootItself,
            };
        };
        if old_name == new_name {
            return FileOpOutcome::Done {
                select: Some(target.clone()),
            };
        }
        let Some(parent_real) = entry.path.parent() else {
            return failed("ファイルを扱えませんでした");
        };
        // 親は範囲の検査で解決した実パス(検査した場所と同じ場所に置く)。
        let folder = ScopedFolder {
            rel: parent_rel,
            real: parent_real.to_path_buf(),
        };
        let source = Source::from(entry);
        let from = source.path.clone();
        self.place(
            &folder,
            new_name,
            source.kind,
            Some(&source.path),
            resolution,
            &|to| self.fs.move_entry(&from, to),
        )
    }

    /// `root` の中の既存の項目 `source` を、フォルダ `dest_dir` の中へ移す(ツリー内のドロップ。FR-029)。
    /// 同じフォルダへの移動は何もしない。
    #[must_use]
    pub fn move_entry(
        &self,
        root: &Path,
        source: &RelPath,
        dest_dir: &RelPath,
        resolution: Option<ConflictResolution>,
    ) -> FileOpOutcome {
        self.transfer_in_root(root, source, dest_dir, Transfer::Move, resolution)
    }

    /// `root` の中の既存の項目 `source` を、フォルダ `dest_dir` の中へ写す(⌥ を押しながらのドロップ。FR-029)。
    /// 元は変えない。
    #[must_use]
    pub fn copy_entry(
        &self,
        root: &Path,
        source: &RelPath,
        dest_dir: &RelPath,
        resolution: Option<ConflictResolution>,
    ) -> FileOpOutcome {
        self.transfer_in_root(root, source, dest_dir, Transfer::Copy, resolution)
    }

    /// `root` の中の既存の項目 `target` をゴミ箱に入れる(FR-028)。ほかの方式や削除に切り替えない。
    #[must_use]
    pub fn trash(&self, root: &Path, target: &RelPath) -> FileOpOutcome {
        let guard = PathScopeGuard::new(self.fs, root);
        let entry = match guard.entry(target) {
            Ok(entry) => entry,
            Err(error) => return error.into(),
        };
        match self.fs.trash(&entry.path) {
            Ok(_) => FileOpOutcome::Done { select: None },
            Err(error) => fs_failure(error),
        }
    }

    /// `root` の中の項目をファイルの URL としてペーストボードに書く(ツリーでのコピー。Finder と互換。FR-027)。
    /// 切り取りの状態は捨てる。ファイルシステムは変えない。
    #[must_use]
    pub fn copy_to_pasteboard(
        &self,
        pasteboard: &dyn FilePasteboardPort,
        clipboard: &mut FileClipboard,
        root: &Path,
        items: &[RelPath],
    ) -> FileOpOutcome {
        let paths = match self.checked_entries(root, items) {
            Ok(paths) => paths,
            Err(outcome) => return outcome,
        };
        let refs: Vec<&Path> = paths.iter().map(PathBuf::as_path).collect();
        match pasteboard.write_file_urls(&refs) {
            Ok(_) => {
                clipboard.clear();
                FileOpOutcome::Done { select: None }
            }
            Err(_) => failed("ペーストボードに書けませんでした"),
        }
    }

    /// `project`(ルート `root`)の中の項目を切り取る(FR-027)。ペーストボードにファイルの URL を書き、
    /// 書いたあとの変更番号と項目を覚える。ルート自体は切り取れない(FR-035)。ファイルシステムは変えない。
    #[must_use]
    pub fn cut(
        &self,
        pasteboard: &dyn FilePasteboardPort,
        clipboard: &mut FileClipboard,
        project: &ProjectId,
        root: &Path,
        items: &[RelPath],
    ) -> FileOpOutcome {
        let paths = match self.checked_entries(root, items) {
            Ok(paths) => paths,
            Err(outcome) => return outcome,
        };
        let refs: Vec<&Path> = paths.iter().map(PathBuf::as_path).collect();
        match pasteboard.write_file_urls(&refs) {
            Ok(change_count) => {
                clipboard.record_cut(project.clone(), items.to_vec(), change_count);
                FileOpOutcome::Done { select: None }
            }
            Err(_) => failed("ペーストボードに書けませんでした"),
        }
    }

    /// ペーストボードのファイルを、`dest_root` の中のフォルダ `dest_dir` へ置く(FR-027)。
    ///
    /// 切り取りのときからペーストボードが変わっていなければ、切り取った項目を移す(移動 1 回。
    /// `project_root` で切り取ったプロジェクトのルートを引き直し、範囲を検査し直す。別のプロジェクトへも移せる)。
    /// 変わっていれば、ペーストボードのファイル(ルートの外でもよい。読むだけ)を写す。
    /// ファイルが無ければ何もしない(ファイルシステムに触れない)。
    ///
    /// 複数の項目で対処が無いときは、先に置く先の中の名前と比べ、1 つでも衝突すれば何も置かずに `Conflict` を返す。
    /// 対処はすべての衝突に当てる。途中の項目で失敗したら、その前の項目は置いたまま、その応答を返す。
    /// 移し終えたら切り取りの状態を捨てる。衝突のあいだは保つ(対処を付けて呼び直すと移動になる)。
    #[must_use]
    pub fn paste(
        &self,
        pasteboard: &dyn FilePasteboardPort,
        clipboard: &mut FileClipboard,
        project_root: &dyn Fn(&ProjectId) -> Option<PathBuf>,
        dest_root: &Path,
        dest_dir: &RelPath,
        resolution: Option<ConflictResolution>,
    ) -> FileOpOutcome {
        let source = match clipboard.source(pasteboard) {
            Ok(source) => source,
            Err(_) => return failed("ペーストボードを読めませんでした"),
        };
        let (mode, sources) = match &source {
            PasteSource::Nothing => return FileOpOutcome::Done { select: None },
            PasteSource::Move { project, items } => {
                let Some(source_root) = project_root(project) else {
                    clipboard.clear();
                    return FileOpOutcome::NotFound;
                };
                (Transfer::Move, Sources::Cut(source_root, items))
            }
            PasteSource::Copy { paths } => (Transfer::Copy, Sources::External(paths)),
        };

        let guard = PathScopeGuard::new(self.fs, dest_root);
        let folder = match guard.folder(dest_dir) {
            Ok(folder) => folder,
            Err(error) => return error.into(),
        };
        let checked = match self.checked_sources(&sources) {
            Ok(checked) => checked,
            Err(outcome) => return outcome,
        };
        for source in &checked {
            if source.kind == EntryKind::Directory && folder.real.starts_with(&source.path) {
                return FileOpOutcome::NotAllowed {
                    reason: NotAllowedReason::IntoDescendant,
                };
            }
        }
        if checked.len() > 1
            && resolution.is_none()
            && let Some(outcome) = self.precheck_conflicts(&folder, &checked)
        {
            return outcome;
        }

        let mut first = None;
        for source in &checked {
            match self.transfer(source, &folder, mode, resolution) {
                FileOpOutcome::Done { select } => {
                    if first.is_none() {
                        first = select;
                    }
                }
                other => return other,
            }
        }
        if mode == Transfer::Move {
            clipboard.clear();
        }
        FileOpOutcome::Done { select: first }
    }

    // ---- 内側 ----

    /// 同じルートの中での移動・コピー(相対パスで決まる拒否はポートを呼ぶ前に行う)。
    fn transfer_in_root(
        &self,
        root: &Path,
        source: &RelPath,
        dest_dir: &RelPath,
        mode: Transfer,
        resolution: Option<ConflictResolution>,
    ) -> FileOpOutcome {
        let guard = PathScopeGuard::new(self.fs, root);
        let (entry, folder) = match guard.transfer(source, dest_dir) {
            Ok(pair) => pair,
            Err(error) => return error.into(),
        };
        self.transfer(&Source::from(entry), &folder, mode, resolution)
    }

    /// 検査を通った元を、検査を通ったフォルダの中へ移す・写す。
    fn transfer(
        &self,
        source: &Source,
        folder: &ScopedFolder,
        mode: Transfer,
        resolution: Option<ConflictResolution>,
    ) -> FileOpOutcome {
        let Some(name) = source.path.file_name().and_then(OsStr::to_str) else {
            return failed("ファイルを扱えませんでした");
        };
        if mode == Transfer::Move && source.path.parent() == Some(folder.real.as_path()) {
            // 同じフォルダへの移動は何もしない(置き換えると自分をゴミ箱に入れることになる)。
            return match folder.child(name) {
                Ok(child) => FileOpOutcome::Done {
                    select: Some(child.rel),
                },
                Err(reason) => FileOpOutcome::InvalidName { reason },
            };
        }
        let from = source.path.as_path();
        let op: &PlaceOp<'_> = match mode {
            Transfer::Move => &|to| self.fs.move_entry(from, to),
            Transfer::Copy => &|to| self.fs.copy_entry(from, to),
        };
        self.place(folder, name, source.kind, Some(from), resolution, op)
    }

    /// `folder` の中に `name` で置く。衝突したら対処に従う。`source` は移す・写す元(置き換えで守る)。
    fn place(
        &self,
        folder: &ScopedFolder,
        name: &str,
        kind: EntryKind,
        source: Option<&Path>,
        resolution: Option<ConflictResolution>,
        op: &PlaceOp<'_>,
    ) -> FileOpOutcome {
        let child = match folder.child(name) {
            Ok(child) => child,
            Err(reason) => return FileOpOutcome::InvalidName { reason },
        };
        match op(&child.path) {
            Ok(()) => {
                return FileOpOutcome::Done {
                    select: Some(child.rel),
                };
            }
            Err(FsError::AlreadyExists(_)) => {}
            Err(error) => return fs_failure(error),
        }
        match resolution {
            None => conflict(name),
            Some(ConflictResolution::Replace) => {
                if source.is_some_and(|source| would_take_source(&child.path, source)) {
                    // 既存が元そのもの・元を含むフォルダなら、ゴミ箱に入れると元も失う。
                    return failed("置き換える項目の中に元の項目があるため、置き換えられません");
                }
                match self.fs.trash(&child.path) {
                    // 見つからない=そのあいだに外から消された。置くだけでよい。
                    Ok(_) | Err(FsError::NotFound(_)) => {}
                    Err(_) => {
                        return failed(
                            "既存の項目をゴミ箱に入れられなかったため、置き換えませんでした",
                        );
                    }
                }
                match op(&child.path) {
                    Ok(()) => FileOpOutcome::Done {
                        select: Some(child.rel),
                    },
                    // ゴミ箱に入れたあとにまた同じ名前ができた。元は変えていない。
                    Err(FsError::AlreadyExists(_)) => conflict(name),
                    // 既存はゴミ箱にあり(戻せる)、元は動いていない(ポートの約束)。自動で戻すことはしない。
                    Err(FsError::CrossDevice(_)) => {
                        failed("別のディスクへは移動できません(既存の項目はゴミ箱に入れました)")
                    }
                    Err(FsError::NotFound(_)) => {
                        failed("元の項目が見つかりません(既存の項目はゴミ箱に入れました)")
                    }
                    Err(_) => failed("置けませんでした(既存の項目はゴミ箱に入れました)"),
                }
            }
            Some(ConflictResolution::KeepBoth) => {
                let names: Vec<String> = match self.fs.read_dir(&folder.real) {
                    Ok(entries) => entries.into_iter().map(|e| e.name).collect(),
                    Err(error) => return fs_failure(error),
                };
                let unique = match keep_both_name(name, kind, &names) {
                    Ok(unique) => unique,
                    Err(reason) => return FileOpOutcome::InvalidName { reason },
                };
                let child = match folder.child(&unique) {
                    Ok(child) => child,
                    Err(reason) => return FileOpOutcome::InvalidName { reason },
                };
                match op(&child.path) {
                    Ok(()) => FileOpOutcome::Done {
                        select: Some(child.rel),
                    },
                    // 番号付きの名前もそのあいだに作られた。もう一度確かめてもらう。
                    Err(FsError::AlreadyExists(_)) => conflict(name),
                    Err(error) => fs_failure(error),
                }
            }
        }
    }

    /// ペーストボードに書く項目を検査し、操作に使う絶対パスを返す。
    fn checked_entries(
        &self,
        root: &Path,
        items: &[RelPath],
    ) -> Result<Vec<PathBuf>, FileOpOutcome> {
        let guard = PathScopeGuard::new(self.fs, root);
        items
            .iter()
            .map(|item| guard.entry(item).map(|entry| entry.path))
            .collect::<Result<_, _>>()
            .map_err(FileOpOutcome::from)
    }

    /// ペーストの元を検査する。切り取ったものは元のルートで範囲を検査し直す。ペーストボードのファイルは
    /// 親の実パスを解決して存在を確かめる(読むだけなので範囲は問わない)。
    fn checked_sources(&self, sources: &Sources<'_>) -> Result<Vec<Source>, FileOpOutcome> {
        match sources {
            Sources::Cut(root, items) => {
                let guard = PathScopeGuard::new(self.fs, root);
                items
                    .iter()
                    .map(|item| guard.entry(item).map(Source::from))
                    .collect::<Result<_, _>>()
                    .map_err(FileOpOutcome::from)
            }
            Sources::External(paths) => paths
                .iter()
                .map(|path| self.external_source(path))
                .collect(),
        }
    }

    /// ペーストボードのファイル 1 つ(絶対パス)を、親の実パス + 名前にして種類を調べる。
    fn external_source(&self, path: &Path) -> Result<Source, FileOpOutcome> {
        let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
            return Err(failed("ペーストできない項目です"));
        };
        if !path.is_absolute() || name.to_str().is_none() {
            return Err(failed("ペーストできない項目です"));
        }
        let real = self.fs.real_dir(parent).map_err(fs_failure)?;
        let path = real.join(name);
        let kind = self.fs.entry_kind(&path).map_err(fs_failure)?;
        Ok(Source { path, kind })
    }

    /// 複数の項目を置く前に、置く先の中の名前と比べる(大文字・小文字を区別しない)。衝突があればその応答。
    fn precheck_conflicts(
        &self,
        folder: &ScopedFolder,
        sources: &[Source],
    ) -> Option<FileOpOutcome> {
        let taken: Vec<String> = match self.fs.read_dir(&folder.real) {
            Ok(entries) => entries.into_iter().map(|e| e.name.to_lowercase()).collect(),
            Err(error) => return Some(fs_failure(error)),
        };
        sources
            .iter()
            .filter_map(|s| s.path.file_name().and_then(OsStr::to_str))
            .find(|name| taken.contains(&name.to_lowercase()))
            .map(conflict)
    }
}

/// ペーストの元(検査の前)。
enum Sources<'s> {
    /// 切り取った項目(元のルートと相対パス)。
    Cut(PathBuf, &'s [RelPath]),
    /// ペーストボードのファイル(絶対パス)。
    External(&'s [PathBuf]),
}

/// 既存 `existing` をゴミ箱に入れると元 `source` も失うか(同じ項目か、`source` を含むフォルダか)。
/// 大文字・小文字を区別しないボリュームを考え、区別せずに比べる(区別するボリュームでは慎重すぎる側に倒れる)。
fn would_take_source(existing: &Path, source: &Path) -> bool {
    let fold = |path: &Path| PathBuf::from(path.to_string_lossy().to_lowercase());
    fold(source).starts_with(fold(existing))
}

fn conflict(name: &str) -> FileOpOutcome {
    FileOpOutcome::Conflict {
        name: name.to_owned(),
    }
}

fn failed(message: &str) -> FileOpOutcome {
    FileOpOutcome::Failed {
        message: message.to_owned(),
    }
}

/// ポートの失敗を応答にする。ポートの文言(絶対パスを含む)は載せない。
fn fs_failure(error: FsError) -> FileOpOutcome {
    match error {
        FsError::NotFound(_) => FileOpOutcome::NotFound,
        FsError::AlreadyExists(_) => failed("同じ名前の項目がすでにあります"),
        FsError::NotADirectory(_) => failed("フォルダではありません"),
        FsError::CrossDevice(_) => failed("別のディスクへは移動できません"),
        FsError::Io(_) => failed("ファイルを扱えませんでした"),
    }
}
