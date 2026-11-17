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

use super::clipboard::{FileClipboard, PastePlan, PasteReport, PasteSource};
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
        self.place(&folder, name, entry_kind, None, None, resolution, op)
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
            None,
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
    /// 決める(`plan_paste`)・置く(`run_paste`)・片付ける(`FileClipboard::settle`)を続けて行う。
    /// アプリは置くあいだ切り取りの状態のロックを持たないよう、3 つを分けて呼ぶ(T42c)。
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
        let plan = match self.plan_paste(pasteboard, clipboard, resolution) {
            Ok(plan) => plan,
            Err(outcome) => return outcome,
        };
        let report = self.run_paste(&plan, project_root, dest_root, dest_dir, resolution);
        clipboard.settle(&plan, &report);
        report.outcome
    }

    /// ペーストで何をするかを決める(切り取りの状態とペーストボードを読むだけ。ファイルシステムに触れない)。
    ///
    /// # Errors
    ///
    /// ペーストボードを読めなかったときは、その応答。
    pub fn plan_paste(
        &self,
        pasteboard: &dyn FilePasteboardPort,
        clipboard: &mut FileClipboard,
        resolution: Option<ConflictResolution>,
    ) -> Result<PastePlan, FileOpOutcome> {
        clipboard
            .plan(pasteboard, resolution)
            .map_err(|_| failed("ペーストボードを読めませんでした"))
    }

    /// 決めたペーストを行う(切り取りの状態には触れない。結果は `FileClipboard::settle` で映す)。
    ///
    /// 複数の項目で対処が無いときは、先に置く先の中の名前と、ペーストする項目どうしの名前を比べ
    /// (大文字・小文字を区別しない。置く先に同じ名前があるかはファイルシステムにも問い合わせる)、
    /// 1 つでも衝突すれば何も置かずに、衝突した数を付けた `Conflict` を返す。対処はすべての衝突に当てる。
    /// ペーストする項目どうしで名前が同じときの「置き換える」は、同じペーストで置いた項目をゴミ箱に入れることになるので、
    /// 何も置かずに失敗を返す。写せない種類(`EntryKind::Other`)の項目があるコピーも、何も置かずに失敗を返す。
    ///
    /// 途中の項目で衝突したら(事前の検査をすり抜けた衝突)、その前の項目は置いたまま、残りの衝突を数え直して返す。
    /// 呼び直すと置き終えた項目は飛ばす(`PastePlan`)。それ以外の失敗も、その前の項目は置いたまま、その応答を返す。
    #[must_use]
    pub fn run_paste(
        &self,
        plan: &PastePlan,
        project_root: &dyn Fn(&ProjectId) -> Option<PathBuf>,
        dest_root: &Path,
        dest_dir: &RelPath,
        resolution: Option<ConflictResolution>,
    ) -> PasteReport {
        let (mode, sources) = match &plan.source {
            PasteSource::Nothing => {
                return PasteReport::nothing(FileOpOutcome::Done { select: None });
            }
            PasteSource::Move { project, items } => {
                let Some(source_root) = project_root(project) else {
                    return PasteReport {
                        source_gone: true,
                        ..PasteReport::nothing(FileOpOutcome::NotFound)
                    };
                };
                (Transfer::Move, Sources::Cut(source_root, items))
            }
            PasteSource::Copy { paths } => (Transfer::Copy, Sources::External(paths)),
        };

        let guard = PathScopeGuard::new(self.fs, dest_root);
        let folder = match guard.folder(dest_dir) {
            Ok(folder) => folder,
            Err(error) => return PasteReport::nothing(error.into()),
        };
        let skip = plan.skip_for(&folder.real);
        let report = |outcome, placed: usize| PasteReport {
            outcome,
            done: skip + placed,
            dest: Some(folder.real.clone()),
            source_gone: false,
        };
        let checked = match self.checked_sources(&sources, skip) {
            Ok(checked) => checked,
            Err(outcome) => return report(outcome, 0),
        };
        for source in &checked {
            if source.kind == EntryKind::Directory && folder.real.starts_with(&source.path) {
                return report(
                    FileOpOutcome::NotAllowed {
                        reason: NotAllowedReason::IntoDescendant,
                    },
                    0,
                );
            }
            if mode == Transfer::Copy
                && let Some(outcome) = refuse_uncopyable(source)
            {
                return report(outcome, 0);
            }
        }
        if checked.len() > 1 {
            match resolution {
                None => match self.count_conflicts(&folder, &checked, mode) {
                    Ok(None) => {}
                    Ok(Some(outcome)) | Err(outcome) => return report(outcome, 0),
                },
                Some(ConflictResolution::Replace) if has_duplicate_names(&checked) => {
                    return report(
                        failed(
                            "ペーストする項目に同じ名前のものがあるため、置き換えられません(「両方残す」を選んでください)",
                        ),
                        0,
                    );
                }
                Some(_) => {}
            }
        }

        let mut first = None;
        for (placed, source) in checked.iter().enumerate() {
            match self.transfer(source, &folder, mode, resolution) {
                FileOpOutcome::Done { select } => {
                    if first.is_none() {
                        first = select;
                    }
                }
                FileOpOutcome::Conflict { name, total } => {
                    // 事前の検査をすり抜けた衝突(正規化の違い・そのあいだに外で作られた)。残りを数え直す。
                    let outcome = match self.count_conflicts(&folder, &checked[placed..], mode) {
                        Ok(Some(recounted @ FileOpOutcome::Conflict { .. })) => recounted,
                        _ => FileOpOutcome::Conflict { name, total },
                    };
                    return report(outcome, placed);
                }
                other => return report(other, placed),
            }
        }
        report(FileOpOutcome::Done { select: first }, checked.len())
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
        let (op, moving): (&PlaceOp<'_>, Option<&Path>) = match mode {
            Transfer::Move => (&|to| self.fs.move_entry(from, to), Some(from)),
            Transfer::Copy => {
                if let Some(outcome) = refuse_uncopyable(source) {
                    return outcome;
                }
                (&|to| self.fs.copy_entry(from, to), None)
            }
        };
        self.place(
            folder,
            name,
            source.kind,
            Some(from),
            moving,
            resolution,
            op,
        )
    }

    /// `folder` の中に `name` で置く。衝突したら対処に従う。`source` は移す・写す元(置き換えで守る)。
    /// `moving` は別のフォルダへ移すときの元。衝突したら、確認を出す前に同じボリュームかを確かめ、
    /// 移せないと分かっていれば(既存をゴミ箱に入れずに)失敗を返す。
    #[allow(clippy::too_many_arguments)]
    fn place(
        &self,
        folder: &ScopedFolder,
        name: &str,
        kind: EntryKind,
        source: Option<&Path>,
        moving: Option<&Path>,
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
        if let Some(from) = moving {
            // 移動の失敗の順(衝突と別のボリュームのどちらが先に返るか)には頼らず、先に確かめる。
            match self.fs.same_volume(from, &folder.real) {
                Ok(true) => {}
                Ok(false) => return fs_failure(FsError::CrossDevice(String::new())),
                Err(error) => return fs_failure(error),
            }
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
    /// `skip` は先頭から飛ばす数(途中で衝突したコピーの呼び直しで、置き終えた項目)。
    fn checked_sources(
        &self,
        sources: &Sources<'_>,
        skip: usize,
    ) -> Result<Vec<Source>, FileOpOutcome> {
        match sources {
            Sources::Cut(root, items) => {
                let guard = PathScopeGuard::new(self.fs, root);
                items
                    .iter()
                    .skip(skip)
                    .map(|item| guard.entry(item).map(Source::from))
                    .collect::<Result<_, _>>()
                    .map_err(FileOpOutcome::from)
            }
            Sources::External(paths) => paths
                .iter()
                .skip(skip)
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

    /// 複数の項目を置く前に、衝突する項目を数える。衝突があれば、最初の名前と数を付けた `Conflict`。
    ///
    /// 衝突とみなすもの: 置く先の中の名前と大文字・小文字を区別せずに同じ、置く先にその名前の項目がある
    /// (ファイルシステムに問い合わせる。APFS は正規化の違いも同じ名前とみなすので、NFC と NFD の違いも見つかる)、
    /// 先に置くペーストの項目と大文字・小文字を区別せずに同じ。同じフォルダへの移動(何もしない)は数えない。
    /// 名前が正しくない項目は数えない(置くときに名前の失敗になる)。
    fn count_conflicts(
        &self,
        folder: &ScopedFolder,
        sources: &[Source],
        mode: Transfer,
    ) -> Result<Option<FileOpOutcome>, FileOpOutcome> {
        let taken: Vec<String> = match self.fs.read_dir(&folder.real) {
            Ok(entries) => entries.into_iter().map(|e| e.name.to_lowercase()).collect(),
            Err(error) => return Err(fs_failure(error)),
        };
        let mut seen: Vec<String> = Vec::new();
        let mut first: Option<&str> = None;
        let mut total: u32 = 0;
        for source in sources {
            let Some(name) = source.path.file_name().and_then(OsStr::to_str) else {
                continue;
            };
            let folded = name.to_lowercase();
            let stays =
                mode == Transfer::Move && source.path.parent() == Some(folder.real.as_path());
            if !stays && validate_name(name).is_ok() {
                let clash = seen.contains(&folded)
                    || taken.contains(&folded)
                    || match self.fs.entry_kind(&folder.real.join(name)) {
                        Ok(_) => true,
                        Err(FsError::NotFound(_)) => false,
                        Err(error) => return Err(fs_failure(error)),
                    };
                if clash {
                    total = total.saturating_add(1);
                    first.get_or_insert(name);
                }
            }
            seen.push(folded);
        }
        Ok(first.map(|name| FileOpOutcome::Conflict {
            name: name.to_owned(),
            total,
        }))
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

/// 単一の項目の衝突(数は 1)。
fn conflict(name: &str) -> FileOpOutcome {
    FileOpOutcome::Conflict {
        name: name.to_owned(),
        total: 1,
    }
}

/// 写せない種類(FIFO・ソケット・デバイス・壊れたリンクなど)なら、その失敗。`copyfile(3)` が開いたまま
/// 戻らない恐れがあるので写さない。
fn refuse_uncopyable(source: &Source) -> Option<FileOpOutcome> {
    if source.kind != EntryKind::Other {
        return None;
    }
    let name = source
        .path
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("項目");
    Some(FileOpOutcome::Failed {
        message: format!(
            "「{name}」はコピーできない種類の項目です(名前付きパイプ・ソケット・壊れたリンクなど)"
        ),
    })
}

/// ペーストする項目どうしに、大文字・小文字を区別せずに同じ名前があるか。
fn has_duplicate_names(sources: &[Source]) -> bool {
    let mut seen: Vec<String> = Vec::new();
    for source in sources {
        if let Some(name) = source.path.file_name().and_then(OsStr::to_str) {
            let folded = name.to_lowercase();
            if seen.contains(&folded) {
                return true;
            }
            seen.push(folded);
        }
    }
    false
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
