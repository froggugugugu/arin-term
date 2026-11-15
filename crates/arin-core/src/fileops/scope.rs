//! 範囲の検査(設計書 §5.1・§7.2 の `PathScopeGuard`、§15.2 #8、FR-035、計画書 T39a)。
//!
//! 画面から届く経路は `RelPath` で、絶対パス・`..`・`.`・空の要素・NUL はそこで弾かれている。ここでは
//! **シンボリックリンクを経由した脱出**を防ぐ: ルートの実パスと、対象の親フォルダ(置き先ではそのフォルダ)の
//! 実パスをファイルシステムのポート(`real_dir`)で解決し、要素ごとに比べる(文字列の前方一致にしない。
//! `/x/app2` は `/x/app` の中ではない)。対象そのものはリンクをたどらない(リンク自体を操作する)。
//!
//! 検査を通った経路は、**解決した実パスの下に名前を足した絶対パス**で返す。手順(T39b)はこの絶対パスを
//! ポートに渡す(検査した場所と操作する場所を同じにするため。リンクを含む元の経路を渡し直さない)。
//!
//! そのほかの拒否: ルート自体を既存の項目として対象にしない(名称変更・移動・切り取り・ゴミ箱。FR-035)。
//! 移す・写すときに、フォルダを自分自身かその子孫へ置かない。
//!
//! 検査は読むだけ(`real_dir`・`entry_kind`)で、ファイルシステムを変えない。ルート自体の拒否と
//! 相対パスだけで決まる子孫の拒否は、ポートに問い合わせる前に決める。

use std::cell::OnceCell;
use std::path::{Component, Path, PathBuf};

use thiserror::Error;

use super::name::validate_name;
use super::types::{FileOpOutcome, InvalidNameReason, NotAllowedReason};
use crate::paths::RelPath;
use crate::ports::fs::{EntryKind, FileSystemPort, FsError};

/// 範囲の検査を通らなかった理由。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ScopeError {
    /// 対象にできない(ルートの外・ルート自体・自分の子孫)。
    #[error("対象にできない: {0:?}")]
    NotAllowed(NotAllowedReason),
    /// 対象(またはその親・置き先・ルート)が無い。
    #[error("見つからない")]
    NotFound,
    /// それ以外のファイルシステムの失敗(フォルダでない、調べられない、ルートが絶対パスでない)。
    #[error("範囲を確かめられなかった: {0}")]
    Fs(FsError),
}

impl From<ScopeError> for FileOpOutcome {
    /// 画面へ返す応答にする。絶対パスは載せない(`FsError` の文言は使わない)。
    fn from(error: ScopeError) -> Self {
        match error {
            ScopeError::NotAllowed(reason) => Self::NotAllowed { reason },
            ScopeError::NotFound => Self::NotFound,
            ScopeError::Fs(FsError::NotADirectory(_)) => Self::Failed {
                message: "フォルダではありません".to_owned(),
            },
            ScopeError::Fs(_) => Self::Failed {
                message: "ファイルを扱えませんでした".to_owned(),
            },
        }
    }
}

/// 検査を通った既存の項目(名称変更・移動・コピーの元・切り取り・ゴミ箱の対象)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedEntry {
    /// ルート基準の相対パス(画面から届いたもの)。ルートではない。
    pub rel: RelPath,
    /// 操作に使う絶対パス(親フォルダの実パス + 名前)。最後の要素がリンクならリンク自体を指す。
    pub path: PathBuf,
    /// 項目の種類(リンクはたどらない)。
    pub kind: EntryKind,
}

/// 検査を通った置き先のフォルダ(新規作成・ペースト・移動・コピーの宛先)。ルートでもよい。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedFolder {
    /// ルート基準の相対パス(画面から届いたもの)。
    pub rel: RelPath,
    /// フォルダの実パス(リンクをすべて解決した、ルートの実パスの中の絶対パス)。
    pub real: PathBuf,
}

/// 置き先のフォルダの中の、これから置く項目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedChild {
    /// ルート基準の相対パス(置き先の相対パス + 名前)。
    pub rel: RelPath,
    /// 操作に使う絶対パス(置き先の実パス + 名前)。
    pub path: PathBuf,
}

impl ScopedFolder {
    /// このフォルダの中に `name` で置くときの経路。名前を検査する。
    ///
    /// # Errors
    ///
    /// 名前が正しくないとき(`validate_name` の理由)。
    pub fn child(&self, name: &str) -> Result<ScopedChild, InvalidNameReason> {
        validate_name(name)?;
        let rel = self
            .rel
            .join(name)
            .expect("検査を通った名前は相対パスに足せる");
        Ok(ScopedChild {
            rel,
            path: self.real.join(name),
        })
    }
}

/// 1 つのプロジェクトのルートを基準にした範囲の検査。
///
/// 作るときはファイルシステムに触れない。ルートの実パスは最初の検査で 1 回だけ解決し、覚えておく
/// (1 回の操作ごとに作り直す使い方を想定する。ルートが動いたあとも古い実パスで比べ続けないため)。
pub struct PathScopeGuard<'a> {
    fs: &'a dyn FileSystemPort,
    root: PathBuf,
    root_real: OnceCell<PathBuf>,
}

impl<'a> PathScopeGuard<'a> {
    /// `root` はプロジェクトのルートの絶対パス(登録したパス。リンクを含んでもよい)。
    #[must_use]
    pub fn new(fs: &'a dyn FileSystemPort, root: &Path) -> Self {
        Self {
            fs,
            root: root.to_path_buf(),
            root_real: OnceCell::new(),
        }
    }

    /// 置き先のフォルダを検査する。ルート(空の相対パス)も通す。
    ///
    /// # Errors
    ///
    /// 実パスがルートの外(`NotAllowed(OutsideRoot)`)、無い(`NotFound`)、フォルダでない・調べられない(`Fs`)。
    pub fn folder(&self, rel: &RelPath) -> Result<ScopedFolder, ScopeError> {
        let root_real = self.root_real()?;
        let real = self.real_inside(root_real, rel)?;
        Ok(ScopedFolder {
            rel: rel.clone(),
            real,
        })
    }

    /// 既存の項目を操作の対象として検査する。ルート自体は拒否する。
    ///
    /// 親フォルダの実パスがルートの中であることを確かめ、項目そのものはリンクをたどらずに種類を調べる。
    ///
    /// # Errors
    ///
    /// ルート自体(`NotAllowed(RootItself)`。ポートに問い合わせない)、親の実パスがルートの外
    /// (`NotAllowed(OutsideRoot)`)、項目か親が無い(`NotFound`)、調べられない(`Fs`)。
    pub fn entry(&self, rel: &RelPath) -> Result<ScopedEntry, ScopeError> {
        let Some((parent, name)) = rel.split_parent() else {
            return Err(ScopeError::NotAllowed(NotAllowedReason::RootItself));
        };
        let root_real = self.root_real()?;
        let parent_real = match self.real_inside(root_real, &parent) {
            // 親がフォルダでなければ、その中の項目は無い。
            Err(ScopeError::Fs(FsError::NotADirectory(_))) => return Err(ScopeError::NotFound),
            other => other?,
        };
        let path = parent_real.join(name);
        let kind = self.fs.entry_kind(&path).map_err(from_fs)?;
        Ok(ScopedEntry {
            rel: rel.clone(),
            path,
            kind,
        })
    }

    /// 既存の項目 `source` を、このルートの中のフォルダ `dest` へ移す・写すときの検査。
    ///
    /// `entry(source)` と `folder(dest)` に加えて、フォルダを自分自身かその子孫へ置かないことを確かめる。
    /// 相対パスで同じか子孫なら(種類によらず)ポートに問い合わせる前に拒否する。リンクを経由して
    /// 実パスで子孫になる場合も、元がフォルダなら拒否する(元がリンクならリンク自体を動かすので通す)。
    ///
    /// # Errors
    ///
    /// `entry`・`folder` の失敗、自分自身か子孫への移動・コピー(`NotAllowed(IntoDescendant)`)。
    pub fn transfer(
        &self,
        source: &RelPath,
        dest: &RelPath,
    ) -> Result<(ScopedEntry, ScopedFolder), ScopeError> {
        if source.is_root() {
            return Err(ScopeError::NotAllowed(NotAllowedReason::RootItself));
        }
        if is_same_or_descendant(dest, source) {
            return Err(ScopeError::NotAllowed(NotAllowedReason::IntoDescendant));
        }
        let entry = self.entry(source)?;
        let folder = self.folder(dest)?;
        if entry.kind == EntryKind::Directory && folder.real.starts_with(&entry.path) {
            return Err(ScopeError::NotAllowed(NotAllowedReason::IntoDescendant));
        }
        Ok((entry, folder))
    }

    /// ルートの実パス(初回だけ解決する)。
    fn root_real(&self) -> Result<&Path, ScopeError> {
        if let Some(real) = self.root_real.get() {
            return Ok(real);
        }
        if !self.root.is_absolute() {
            return Err(ScopeError::Fs(FsError::Io(
                "ルートが絶対パスではない".to_owned(),
            )));
        }
        let real = self.fs.real_dir(&self.root).map_err(from_fs)?;
        if !is_normal_absolute(&real) {
            return Err(ScopeError::Fs(FsError::Io(
                "ルートの実パスが正規化されていない".to_owned(),
            )));
        }
        Ok(self.root_real.get_or_init(|| real))
    }

    /// `root_real` の下の相対パス `rel` をフォルダとして解決し、実パスがルートの中であることを確かめる。
    fn real_inside(&self, root_real: &Path, rel: &RelPath) -> Result<PathBuf, ScopeError> {
        if rel.is_root() {
            return Ok(root_real.to_path_buf());
        }
        let mut path = root_real.to_path_buf();
        path.extend(rel.components());
        let real = self.fs.real_dir(&path).map_err(from_fs)?;
        // 要素ごとに比べる(`Path::starts_with` は要素単位)。`..`・`.` が残る形は信用せず外とみなす。
        if is_normal_absolute(&real) && real.starts_with(root_real) {
            Ok(real)
        } else {
            Err(ScopeError::NotAllowed(NotAllowedReason::OutsideRoot))
        }
    }
}

/// ポートの失敗を検査の失敗にする(見つからないだけを分ける)。
fn from_fs(error: FsError) -> ScopeError {
    match error {
        FsError::NotFound(_) => ScopeError::NotFound,
        other => ScopeError::Fs(other),
    }
}

/// 絶対パスで、要素が `/` とふつうの名前だけ(`..`・`.` が無い)か。
fn is_normal_absolute(path: &Path) -> bool {
    path.is_absolute()
        && path
            .components()
            .all(|c| matches!(c, Component::RootDir | Component::Normal(_)))
}

/// `path` が `base` と同じか、その子孫か(相対パスの要素で比べる)。
fn is_same_or_descendant(path: &RelPath, base: &RelPath) -> bool {
    let mut path = path.components();
    base.components().all(|b| path.next() == Some(b))
}
