//! ローカルのファイルシステム(`FileSystemPort` の実装。計画書 T40a、設計書 §5.4「上書きしない移動・コピー」)。
//!
//! - 読み込み: `read_dir` の項目の種類(リンクをたどらない)で判定し、シンボリックリンクだけ先をたどって
//!   ファイルかフォルダかを調べる。先が無い・調べられないリンクは「その他」
//! - 新規作成: ファイルは `O_CREAT | O_EXCL`(最後の要素がシンボリックリンクでも失敗する)、フォルダは `mkdir(2)`
//! - 移動: `renamex_np(2)` の `RENAME_EXCL`。宛先に何かあれば(空のフォルダでも)何も変えずに `EEXIST` で失敗する。
//!   大文字・小文字だけを変える名称変更(【仮定】15)は、macOS 26 の APFS(大文字・小文字を区別しない)では
//!   `RENAME_EXCL` のままで成功し、表記が変わる(tests/fs.rs で確認。計画書の見込みと違った)。
//!   別の版やボリュームで `EEXIST` になったときに備え、宛先が元と同じ項目だと確かめられたときに限り、
//!   フラグなしで名前を変える
//! - コピー: 宛先と同じフォルダに作業用のフォルダを `mkdir(2)` で作り、その中へ `copyfile(3)` で写してから、
//!   `RENAME_EXCL` で宛先の名前に移す。宛先の名前に作りかけが現れることはなく、宛先が途中で作られても上書きしない。
//!   失敗したら作業用のフォルダ(このコピーが作った物だけが入る)を片付け、片付けられなければ残った場所を文言に含める
//!
//! どの呼び出しもその場で終わる同期の処理。メインスレッドの外(コマンドの非同期タスク)から呼ぶ(設計書 §7.1)。

use std::ffi::CString;
use std::fs::{self, OpenOptions};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use arin_core::ports::fs::{DirEntry, EntryKind, FileSystemPort, FsError};

/// コピーの作業用フォルダの名前の頭。宛先と同じフォルダに一時的に現れる。
/// ツリーの一覧はこの頭の名前を隠しファイル表示によらず出さない(`src-tauri/src/tree.rs`)。
pub const STAGING_PREFIX: &str = ".arin-copy-";
/// 作業用フォルダの名前が既に使われていたときに、別の名前で作り直す回数。
const STAGING_ATTEMPTS: usize = 16;
/// 作業用フォルダの権限(作る間だけ。中身の権限は写した元のまま)。
const STAGING_MODE: u32 = 0o700;

/// 作業用フォルダの名前の連番。
static STAGING_SEQ: AtomicU64 = AtomicU64::new(0);

/// 端末のファイルシステムを読み、上書きせずに作る・移す・写す。状態を持たない。
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalFileSystem;

impl LocalFileSystem {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl FileSystemPort for LocalFileSystem {
    fn read_dir(&self, dir: &Path) -> Result<Vec<DirEntry>, FsError> {
        let entries = fs::read_dir(dir).map_err(|e| fs_error(dir, &e))?;
        let mut out = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| fs_error(dir, &e))?;
            // 名前が UTF-8 でない項目は扱わない(【仮定】24)
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            let path = entry.path();
            let kind = match entry.file_type() {
                Ok(file_type) => kind_of(&path, file_type),
                // 読んだあとに消えたものは返さない(次の変更の通知で揃う)
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => return Err(fs_error(&path, &e)),
            };
            out.push(DirEntry { name, kind });
        }
        Ok(out)
    }

    fn entry_kind(&self, path: &Path) -> Result<EntryKind, FsError> {
        let metadata = fs::symlink_metadata(path).map_err(|e| fs_error(path, &e))?;
        Ok(kind_of(path, metadata.file_type()))
    }

    fn real_dir(&self, dir: &Path) -> Result<PathBuf, FsError> {
        let real = fs::canonicalize(dir).map_err(|e| fs_error(dir, &e))?;
        let metadata = fs::metadata(&real).map_err(|e| fs_error(&real, &e))?;
        if !metadata.is_dir() {
            return Err(FsError::NotADirectory(real.display().to_string()));
        }
        Ok(real)
    }

    fn create_file(&self, path: &Path) -> Result<(), FsError> {
        // create_new は O_CREAT | O_EXCL。既にあれば(リンクでも)EEXIST で失敗し、リンクの先にも作らない
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map(drop)
            .map_err(|e| fs_error(path, &e))
    }

    fn create_dir(&self, path: &Path) -> Result<(), FsError> {
        // mkdir(2) は既にあれば EEXIST で失敗する
        fs::create_dir(path).map_err(|e| fs_error(path, &e))
    }

    fn move_entry(&self, from: &Path, to: &Path) -> Result<(), FsError> {
        let from_c = c_path(from)?;
        let to_c = c_path(to)?;
        match rename_excl(&from_c, &to_c) {
            Ok(()) => Ok(()),
            Err(e) if e.raw_os_error() == Some(libc::EEXIST) && is_same_entry_renamed(from, to) => {
                // 宛先は元そのもの(大文字・小文字などの違いだけ)。ほかの項目は置き換わらない
                rename_plain(&from_c, &to_c).map_err(|e| move_error(from, to, &e))
            }
            Err(e) => Err(move_error(from, to, &e)),
        }
    }

    fn copy_entry(&self, from: &Path, to: &Path) -> Result<(), FsError> {
        let metadata = fs::symlink_metadata(from).map_err(|e| fs_error(from, &e))?;
        // 先に分かる衝突はすぐ返す(上書きしないことの保証は、最後の RENAME_EXCL が受け持つ)
        match fs::symlink_metadata(to) {
            Ok(_) => return Err(FsError::AlreadyExists(to.display().to_string())),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(fs_error(to, &e)),
        }
        let (Some(parent), Some(name)) = (to.parent(), to.file_name()) else {
            return Err(FsError::Io(format!("{}: 写す先の名前が無い", to.display())));
        };
        let is_dir = metadata.file_type().is_dir();
        if is_dir {
            reject_copy_into_itself(from, parent)?;
        }

        let staging = create_staging_dir(parent)?;
        let staged = staging.join(name);
        let result = copy_into(from, &staged, is_dir).and_then(|()| {
            rename_excl(&c_path(&staged)?, &c_path(to)?).map_err(|e| move_error(&staged, to, &e))
        });
        match result {
            Ok(()) => {
                // 中身は宛先へ移ったので、作業用フォルダは空。空でなければ消さずに残す
                let _ = fs::remove_dir(&staging);
                Ok(())
            }
            Err(error) => Err(clean_up_staging(&staging, error)),
        }
    }

    fn trash(&self, path: &Path) -> Result<Option<PathBuf>, FsError> {
        // T40c で実装する(ADR-007 の (a) の結果に従う)。それまでは何も変えずに失敗する。
        // 別の方式や削除に切り替えない
        Err(FsError::Io(format!(
            "{}: ゴミ箱はまだ使えない(未実装)",
            path.display()
        )))
    }
}

/// 項目の種類を、リンクをたどらない種類から決める。リンクだけ先をたどる。
fn kind_of(path: &Path, file_type: fs::FileType) -> EntryKind {
    if file_type.is_symlink() {
        match fs::metadata(path) {
            Ok(target) if target.is_dir() => EntryKind::DirectoryLink,
            Ok(target) if target.is_file() => EntryKind::FileLink,
            // 先が無い(壊れた)リンク、先がソケットなど、調べられないもの
            _ => EntryKind::Other,
        }
    } else if file_type.is_dir() {
        EntryKind::Directory
    } else if file_type.is_file() {
        EntryKind::File
    } else {
        EntryKind::Other
    }
}

/// `to` が `from` と同じ項目を別の表記で指しているか(APFS の大文字・小文字や正規化の違いだけの名称変更)。
///
/// 同じフォルダの中で、同じ装置・inode を指し、名前が違うこと。ハードリンク(別の名前の別の項目)を
/// 名称変更と取り違えないよう、フォルダ以外はリンク数が 1 のものに限る。
fn is_same_entry_renamed(from: &Path, to: &Path) -> bool {
    let (Some(from_parent), Some(to_parent)) = (from.parent(), to.parent()) else {
        return false;
    };
    if from.file_name() == to.file_name() {
        return false;
    }
    let (Ok(a), Ok(b)) = (fs::symlink_metadata(from), fs::symlink_metadata(to)) else {
        return false;
    };
    let (Ok(pa), Ok(pb)) = (fs::metadata(from_parent), fs::metadata(to_parent)) else {
        return false;
    };
    let same_entry = a.dev() == b.dev() && a.ino() == b.ino();
    let same_parent = pa.dev() == pb.dev() && pa.ino() == pb.ino();
    let not_hard_link = a.file_type().is_dir() || a.nlink() == 1;
    same_entry && same_parent && not_hard_link
}

/// フォルダを自分自身の中へ写すこと(終わらない複製)を拒否する。
fn reject_copy_into_itself(from: &Path, to_parent: &Path) -> Result<(), FsError> {
    let from_real = fs::canonicalize(from).map_err(|e| fs_error(from, &e))?;
    let parent_real = fs::canonicalize(to_parent).map_err(|e| fs_error(to_parent, &e))?;
    if parent_real.starts_with(&from_real) {
        return Err(FsError::Io(format!(
            "{}: フォルダを自分の中へは写せない",
            from.display()
        )));
    }
    Ok(())
}

/// 宛先と同じフォルダに、このコピーだけが使う作業用フォルダを作る。
fn create_staging_dir(parent: &Path) -> Result<PathBuf, FsError> {
    let mut builder = fs::DirBuilder::new();
    builder.mode(STAGING_MODE);
    for _ in 0..STAGING_ATTEMPTS {
        let n = STAGING_SEQ.fetch_add(1, Ordering::Relaxed);
        let staging = parent.join(format!("{STAGING_PREFIX}{}-{n}", std::process::id()));
        match builder.create(&staging) {
            Ok(()) => return Ok(staging),
            // 同じ名前が既にある(前に落ちたプロセスの残りなど)。触らずに別の名前にする
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(fs_error(parent, &e)),
        }
    }
    Err(FsError::Io(format!(
        "{}: 作業用のフォルダの名前が空いていない",
        parent.display()
    )))
}

/// `from` を、まだ無い `to` へ `copyfile(3)` で写す。リンクはリンクのまま写し、フォルダは中身ごと写す。
fn copy_into(from: &Path, to: &Path, is_dir: bool) -> Result<(), FsError> {
    // COPYFILE_EXCL: 写す先が既にあれば失敗する。COPYFILE_NOFOLLOW: リンクの先をたどらない。
    // COPYFILE_CLONE: 同じ APFS ならクローン(容量を使わない)を試し、できなければふつうに写す
    // COPYFILE_METADATA | COPYFILE_DATA は copyfile.h の COPYFILE_ALL(libc に定義が無い)。
    let mut flags = libc::COPYFILE_METADATA
        | libc::COPYFILE_DATA
        | libc::COPYFILE_EXCL
        | libc::COPYFILE_NOFOLLOW
        | libc::COPYFILE_CLONE;
    if is_dir {
        flags |= libc::COPYFILE_RECURSIVE;
    }
    let from_c = c_path(from)?;
    let to_c = c_path(to)?;
    // SAFETY: どちらも NUL で終わる有効な C 文字列で、この呼び出しの間は生きている。
    // 状態(copyfile_state_t)は渡さない(NULL は許されている)
    let rc = unsafe { libc::copyfile(from_c.as_ptr(), to_c.as_ptr(), std::ptr::null_mut(), flags) };
    if rc < 0 {
        return Err(fs_error(from, &io::Error::last_os_error()));
    }
    Ok(())
}

/// 失敗したコピーの作業用フォルダを片付ける。片付けられなければ、残った場所を文言に足す。
fn clean_up_staging(staging: &Path, error: FsError) -> FsError {
    match fs::remove_dir_all(staging) {
        Ok(()) => error,
        Err(e) if e.kind() == io::ErrorKind::NotFound => error,
        Err(e) => FsError::Io(format!(
            "{error}。作りかけが {} に残った(片付けられなかった: {e})",
            staging.display()
        )),
    }
}

/// `renamex_np(from, to, RENAME_EXCL)`。宛先に何かあれば何も変えずに `EEXIST` で失敗する。
fn rename_excl(from: &CString, to: &CString) -> io::Result<()> {
    renamex(from, to, libc::RENAME_EXCL)
}

/// フラグなしの `renamex_np`。宛先が元と同じ項目であると確かめたときだけ使う。
fn rename_plain(from: &CString, to: &CString) -> io::Result<()> {
    renamex(from, to, 0)
}

fn renamex(from: &CString, to: &CString, flags: libc::c_uint) -> io::Result<()> {
    // SAFETY: どちらも NUL で終わる有効な C 文字列で、この呼び出しの間は生きている
    let rc = unsafe { libc::renamex_np(from.as_ptr(), to.as_ptr(), flags) };
    if rc == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn c_path(path: &Path) -> Result<CString, FsError> {
    CString::new(path.as_os_str().as_bytes())
        .map_err(|_| FsError::Io(format!("{}: パスに NUL が含まれる", path.display())))
}

/// 移動の失敗を振り分ける。元が無いときは見つからない、宛先があるときは衝突。
fn move_error(from: &Path, to: &Path, e: &io::Error) -> FsError {
    match e.raw_os_error() {
        Some(libc::ENOENT) if fs::symlink_metadata(from).is_err() => {
            FsError::NotFound(format!("{}: {e}", from.display()))
        }
        _ => fs_error(to, e),
    }
}

/// 入出力のエラーを [`FsError`] に振り分ける。
fn fs_error(path: &Path, e: &io::Error) -> FsError {
    let message = format!("{}: {e}", path.display());
    match e.raw_os_error() {
        Some(libc::ENOENT) => FsError::NotFound(message),
        // ENOTEMPTY は空でないフォルダへの改名。上書きしないので衝突として扱う
        Some(libc::EEXIST | libc::ENOTEMPTY) => FsError::AlreadyExists(message),
        Some(libc::ENOTDIR) => FsError::NotADirectory(message),
        Some(libc::EXDEV) => FsError::CrossDevice(message),
        _ => match e.kind() {
            io::ErrorKind::NotFound => FsError::NotFound(message),
            io::ErrorKind::AlreadyExists => FsError::AlreadyExists(message),
            io::ErrorKind::NotADirectory => FsError::NotADirectory(message),
            _ => FsError::Io(message),
        },
    }
}
