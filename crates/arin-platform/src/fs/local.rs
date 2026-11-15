//! ローカルのファイルシステム(`FileSystemPort` の実装。計画書 T40a、設計書 §5.4「上書きしない移動・コピー」)。
//!
//! - 読み込み: `read_dir` の項目の種類(リンクをたどらない)で判定し、シンボリックリンクだけ先をたどって
//!   ファイルかフォルダかを調べる。先が無い・調べられないリンクは「その他」
//! - 新規作成: ファイルは `openat(2)` の `O_CREAT | O_EXCL`(最後の要素がシンボリックリンクでも失敗する)、
//!   フォルダは `mkdirat(2)`
//! - 移動: `renameatx_np(2)` の `RENAME_EXCL`。宛先に何かあれば(空のフォルダでも)何も変えずに `EEXIST` で失敗する。
//!   大文字・小文字だけを変える名称変更(【仮定】15)は、macOS 26 の APFS(大文字・小文字を区別しない)では
//!   `RENAME_EXCL` のままで成功し、表記が変わる(tests/fs.rs で確認。計画書の見込みと違った)。
//!   別の版やボリュームで `EEXIST` になったときに備え、宛先が元と同じ項目だと確かめられたときに限り、
//!   フラグなしで名前を変える
//! - コピー: 宛先と同じフォルダに作業用のフォルダを `mkdirat(2)` で作り、その中へ `copyfile(3)` で写してから、
//!   `RENAME_EXCL` で宛先の名前に移す。宛先の名前に作りかけが現れることはなく、宛先が途中で作られても上書きしない。
//!   失敗したら作業用のフォルダ(このコピーが作った物だけが入る)を片付け、片付けられなければ残った場所を文言に含める
//!
//! **書き込む操作(作成・移動・コピー・ゴミ箱)はパスの途中のシンボリックリンクを拒否する**(FR-035。T44a の
//! 差し替えのテスト)。渡されるパスは範囲の検査が解決した実パス + 名前なので、正しい呼び出しでは途中にリンクは無い。
//! 検査のあとに外から途中のフォルダをルートの外へのリンクに差し替えられても、外に書かないためのもの。
//!
//! - 親のフォルダを `open(O_SEARCH | O_DIRECTORY | O_NOFOLLOW_ANY)` で開いて fd で押さえ、そこからの相対で
//!   `openat`・`mkdirat`・`renameatx_np` を呼ぶ。途中か親そのものがリンクなら `ELOOP` で失敗する(`FsError::Io`)。
//!   開いたあとに名前を差し替えられても、確かめたフォルダそのものに対して操作する
//! - 最後の要素はたどらない(リンク自体を作る先・移す元・写す元にできる。これまでどおり)
//! - ゴミ箱に入れる口(`NSFileManager`)はパスしか受け取れないので、OS に頼む前に親を同じ開き方で確かめるだけ
//!   (確かめてから OS がパスをたどるまでの間の差し替えは防げない)
//! - `O_NOFOLLOW_ANY`(0x20000000)は libc 0.2.191 の定義を使う。値と挙動は macOS 26 の SDK の
//!   `sys/fcntl.h` と `man 2 open`(「any component of the path … is a symbolic link then the open() will fail」)で確かめた。
//!   `O_SEARCH` は読む権限の無いフォルダでも開ける(`mkdir(2)` と同じく、書く権限と検索の権限だけで足りる)
//! - コピーの `copyfile(3)` はパスしか受け取れない。作業用フォルダは押さえた親の fd の下に `mkdirat` で作って fd で
//!   開き直し、写す直前に「そのパスがリンクを通らずに同じフォルダを指す」ことを確かめ、写したあとに中身が
//!   作業用フォルダ(fd)にあることを確かめ、最後の移動は fd 相対の `renameatx_np` で行う。確かめてから `copyfile` が
//!   パスをたどるまでのごく短い間に祖先を差し替えられると、外に新しい項目を作りうる(上書きはしない。
//!   `COPYFILE_EXCL`)。そのときは失敗を返し、写した物を宛先には置かない
//!
//! 読み込みの操作(`read_dir`・`entry_kind`・`real_dir`)はリンクを含むパスもこれまでどおり扱う。
//!
//! どの呼び出しもその場で終わる同期の処理。メインスレッドの外(コマンドの非同期タスク)から呼ぶ(設計書 §7.1)。

use std::ffi::{CStr, CString};
use std::fs;
use std::io;
use std::mem::MaybeUninit;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use arin_core::ports::fs::{DirEntry, EntryKind, FileSystemPort, FsError};

/// コピーの作業用フォルダの名前の頭。宛先と同じフォルダに一時的に現れる。
/// ツリーの一覧はこの頭の名前を隠しファイル表示によらず出さない(`src-tauri/src/tree.rs`)。
pub const STAGING_PREFIX: &str = ".arin-copy-";
/// 作業用フォルダの名前が既に使われていたときに、別の名前で作り直す回数。
const STAGING_ATTEMPTS: usize = 16;
/// 作業用フォルダの権限(作る間だけ。中身の権限は写した元のまま)。
const STAGING_MODE: libc::mode_t = 0o700;
/// 新しいファイルの権限(umask を引く前。`std::fs::OpenOptions` の既定と同じ)。
const NEW_FILE_MODE: libc::c_uint = 0o666;
/// 新しいフォルダの権限(umask を引く前。`std::fs::create_dir` の既定と同じ)。
const NEW_DIR_MODE: libc::mode_t = 0o777;
/// 親のフォルダを押さえるときの開き方。途中か親そのものがシンボリックリンクなら `ELOOP` で失敗する。
const OPEN_PARENT_FLAGS: libc::c_int =
    libc::O_SEARCH | libc::O_DIRECTORY | libc::O_NOFOLLOW_ANY | libc::O_CLOEXEC;

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
        let at = ParentDir::open(path)?;
        // O_CREAT | O_EXCL: 既にあれば(リンクでも)EEXIST で失敗し、リンクの先にも作らない
        let flags =
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC;
        // SAFETY: fd は開いているフォルダ、名前は NUL で終わる有効な C 文字列(`/` を含まない)。
        // O_CREAT なので 3 つ目の引数(権限)を渡す
        let fd = unsafe { libc::openat(at.fd.as_raw_fd(), at.name.as_ptr(), flags, NEW_FILE_MODE) };
        if fd < 0 {
            return Err(fs_error(path, &io::Error::last_os_error()));
        }
        // SAFETY: openat が返した、ほかに持ち主のいない fd。ここで閉じる
        drop(unsafe { OwnedFd::from_raw_fd(fd) });
        Ok(())
    }

    fn create_dir(&self, path: &Path) -> Result<(), FsError> {
        let at = ParentDir::open(path)?;
        // mkdirat(2) は既にあれば(リンクでも)EEXIST で失敗する
        mkdir_at(at.fd.as_raw_fd(), &at.name, NEW_DIR_MODE).map_err(|e| fs_error(path, &e))
    }

    fn move_entry(&self, from: &Path, to: &Path) -> Result<(), FsError> {
        let src = ParentDir::open(from)?;
        let dst = ParentDir::open(to)?;
        match rename_at(src.at(), dst.at(), libc::RENAME_EXCL) {
            Ok(()) => Ok(()),
            Err(e)
                if e.raw_os_error() == Some(libc::EEXIST) && is_same_entry_renamed(&src, &dst) =>
            {
                // 宛先は元そのもの(大文字・小文字などの違いだけ)。ほかの項目は置き換わらない
                rename_at(src.at(), dst.at(), 0).map_err(|e| move_error(&src, from, to, &e))
            }
            Err(e) => Err(move_error(&src, from, to, &e)),
        }
    }

    fn copy_entry(&self, from: &Path, to: &Path) -> Result<(), FsError> {
        let src = ParentDir::open(from)?;
        let from_stat = src.stat_child().map_err(|e| fs_error(from, &e))?;
        let dst = ParentDir::open(to)?;
        // 先に分かる衝突はすぐ返す(上書きしないことの保証は、最後の RENAME_EXCL が受け持つ)
        match dst.stat_child() {
            Ok(_) => return Err(FsError::AlreadyExists(to.display().to_string())),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(fs_error(to, &e)),
        }
        let is_dir = from_stat.st_mode & libc::S_IFMT == libc::S_IFDIR;
        if is_dir {
            reject_copy_into_itself(from, &dst.dir_path)?;
        }

        let staging = Staging::create(&dst)?;
        let staged = staging.path.join(dst.name_os());
        let result = copy_into(from, &staged, &staging, is_dir).and_then(|()| {
            rename_at(
                (staging.fd.as_raw_fd(), &dst.name),
                dst.at(),
                libc::RENAME_EXCL,
            )
            .map_err(|e| fs_error(to, &e))
        });
        match result {
            Ok(()) => {
                // 中身は宛先へ移ったので、作業用フォルダは空。空でなければ(rmdir なので)消さずに残す
                // SAFETY: fd は開いているフォルダ、名前は NUL で終わる有効な C 文字列
                let _ = unsafe {
                    libc::unlinkat(
                        dst.fd.as_raw_fd(),
                        staging.name.as_ptr(),
                        libc::AT_REMOVEDIR,
                    )
                };
                Ok(())
            }
            Err(error) => Err(clean_up_staging(&staging, error)),
        }
    }

    fn trash(&self, path: &Path) -> Result<Option<PathBuf>, FsError> {
        // 途中にリンクがあれば OS に頼まない(ゴミ箱に入れる口はパスしか受け取れないため、確かめるだけ)
        if path.is_absolute() && !path.components().any(|c| c == Component::ParentDir) {
            drop(ParentDir::open(path)?);
        }
        // 中身は `trash.rs`(T40c、ADR-007 の (a))。入れられなければ失敗を返し、削除に切り替えない
        super::trash::trash(path)
    }
}

/// 書き込む操作の対象の、押さえた親のフォルダと最後の名前。
struct ParentDir {
    /// 親のフォルダ(`O_SEARCH`。途中にリンクが無いことを確かめて開いた)。
    fd: OwnedFd,
    /// 最後の名前(`/` を含まない)。
    name: CString,
    /// 親のフォルダのパス(文言と、パスしか受け取れない呼び出しのため)。
    dir_path: PathBuf,
}

impl ParentDir {
    /// `path` の親を、途中にシンボリックリンクが無いことを確かめて開く。
    ///
    /// `path` は絶対パスで `..` を含まないこと(末尾の `/` と途中の `.` は要素に分けたときに落ちる)。
    fn open(path: &Path) -> Result<Self, FsError> {
        if !path.is_absolute() {
            return Err(FsError::Io(format!("絶対パスではない: {}", path.display())));
        }
        if path.components().any(|c| c == Component::ParentDir) {
            return Err(FsError::Io(format!("`..` を含む: {}", path.display())));
        }
        let normalized: PathBuf = path.components().collect();
        let (Some(dir), Some(name)) = (normalized.parent(), normalized.file_name()) else {
            return Err(FsError::Io(format!("名前が無い: {}", path.display())));
        };
        let dir_c = c_path(dir)?;
        let name = CString::new(name.as_bytes())
            .map_err(|_| FsError::Io(format!("{}: パスに NUL が含まれる", path.display())))?;
        // SAFETY: NUL で終わる有効な C 文字列。O_CREAT を含まないので 3 つ目の引数は要らない
        let fd = unsafe { libc::open(dir_c.as_ptr(), OPEN_PARENT_FLAGS) };
        if fd < 0 {
            let e = io::Error::last_os_error();
            return Err(match e.raw_os_error() {
                Some(libc::ELOOP) => FsError::Io(format!(
                    "{}: パスの途中にシンボリックリンクがある({e})",
                    dir.display()
                )),
                _ => fs_error(dir, &e),
            });
        }
        Ok(Self {
            // SAFETY: open が返した、ほかに持ち主のいない fd
            fd: unsafe { OwnedFd::from_raw_fd(fd) },
            name,
            dir_path: dir.to_path_buf(),
        })
    }

    /// fd 相対の呼び出しに渡す、親の fd と名前。
    fn at(&self) -> (RawFd, &CStr) {
        (self.fd.as_raw_fd(), &self.name)
    }

    fn name_os(&self) -> &std::ffi::OsStr {
        std::ffi::OsStr::from_bytes(self.name.as_bytes())
    }

    /// 最後の名前の項目を調べる(リンクはたどらない)。
    fn stat_child(&self) -> io::Result<libc::stat> {
        stat_at(self.fd.as_raw_fd(), &self.name, libc::AT_SYMLINK_NOFOLLOW)
    }
}

/// `from` が `to` の名前に変わるとき、`to` が `from` と同じ項目を別の表記で指しているか
/// (APFS の大文字・小文字や正規化の違いだけの名称変更)。
///
/// 同じフォルダの中で、同じ装置・inode を指し、名前が違うこと。ハードリンク(別の名前の別の項目)を
/// 名称変更と取り違えないよう、フォルダ以外はリンク数が 1 のものに限る。
fn is_same_entry_renamed(from: &ParentDir, to: &ParentDir) -> bool {
    if from.name == to.name {
        return false;
    }
    let (Ok(a), Ok(b)) = (from.stat_child(), to.stat_child()) else {
        return false;
    };
    let (Ok(pa), Ok(pb)) = (fstat(from.fd.as_raw_fd()), fstat(to.fd.as_raw_fd())) else {
        return false;
    };
    let same_entry = a.st_dev == b.st_dev && a.st_ino == b.st_ino;
    let same_parent = pa.st_dev == pb.st_dev && pa.st_ino == pb.st_ino;
    let not_hard_link = a.st_mode & libc::S_IFMT == libc::S_IFDIR || a.st_nlink == 1;
    same_entry && same_parent && not_hard_link
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

/// コピーの作業用フォルダ。宛先の親(押さえた fd)の下に作り、fd で開いて持つ。
struct Staging {
    /// 作業用フォルダ(作った直後に、押さえた親の fd からリンクをたどらずに開いた)。
    fd: OwnedFd,
    /// 作業用フォルダの名前(宛先の親の中)。
    name: CString,
    /// 作業用フォルダのパス(`copyfile(3)` と片付けのため。使う前に fd と同じフォルダか確かめる)。
    path: PathBuf,
}

impl Staging {
    /// 宛先と同じフォルダ(`dst` の親)に、このコピーだけが使う作業用フォルダを作る。
    fn create(dst: &ParentDir) -> Result<Self, FsError> {
        let parent_fd = dst.fd.as_raw_fd();
        for _ in 0..STAGING_ATTEMPTS {
            let n = STAGING_SEQ.fetch_add(1, Ordering::Relaxed);
            let file_name = format!("{STAGING_PREFIX}{}-{n}", std::process::id());
            let path = dst.dir_path.join(&file_name);
            let name = CString::new(file_name).expect("作業用フォルダの名前に NUL は無い");
            match mkdir_at(parent_fd, &name, STAGING_MODE) {
                Ok(()) => {}
                // 同じ名前が既にある(前に落ちたプロセスの残りなど)。触らずに別の名前にする
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(fs_error(&dst.dir_path, &e)),
            }
            // 作った名前を、押さえた親からリンクをたどらずに開く
            let flags = libc::O_SEARCH | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
            // SAFETY: fd は開いているフォルダ、名前は NUL で終わる有効な C 文字列。O_CREAT を含まない
            let fd = unsafe { libc::openat(parent_fd, name.as_ptr(), flags) };
            if fd < 0 {
                let e = io::Error::last_os_error();
                // 作った直後に差し替えられた。作った物は消さずに残し、場所を文言に含める
                return Err(FsError::Io(format!(
                    "{}: 作業用のフォルダを開けなかった({e})。{} が残った可能性がある",
                    dst.dir_path.display(),
                    path.display()
                )));
            }
            return Ok(Self {
                // SAFETY: openat が返した、ほかに持ち主のいない fd
                fd: unsafe { OwnedFd::from_raw_fd(fd) },
                name,
                path,
            });
        }
        Err(FsError::Io(format!(
            "{}: 作業用のフォルダの名前が空いていない",
            dst.dir_path.display()
        )))
    }

    /// 作業用フォルダのパスが、いまもリンクを通らずにこの作業用フォルダ(fd)を指しているか。
    fn path_is_still_here(&self) -> bool {
        let Ok(path_c) = c_path(&self.path) else {
            return false;
        };
        // SAFETY: NUL で終わる有効な C 文字列。O_CREAT を含まない
        let fd = unsafe { libc::open(path_c.as_ptr(), OPEN_PARENT_FLAGS) };
        if fd < 0 {
            return false;
        }
        // SAFETY: open が返した、ほかに持ち主のいない fd
        let opened = unsafe { OwnedFd::from_raw_fd(fd) };
        match (fstat(opened.as_raw_fd()), fstat(self.fd.as_raw_fd())) {
            (Ok(a), Ok(b)) => a.st_dev == b.st_dev && a.st_ino == b.st_ino,
            _ => false,
        }
    }
}

/// `from` を、まだ無い `to`(作業用フォルダの中)へ `copyfile(3)` で写す。リンクはリンクのまま写し、
/// フォルダは中身ごと写す。写す直前と直後に、`to` が作業用フォルダ(fd)の中であることを確かめる。
fn copy_into(from: &Path, to: &Path, staging: &Staging, is_dir: bool) -> Result<(), FsError> {
    if !staging.path_is_still_here() {
        return Err(FsError::Io(format!(
            "{}: パスの途中が変わったため写さなかった",
            to.display()
        )));
    }
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
    // 写した物が作業用フォルダ(fd)の中にあること。無ければ、写している間にパスの途中を差し替えられた
    let Some(name) = to.file_name() else {
        return Err(FsError::Io(format!("{}: 写す先の名前が無い", to.display())));
    };
    let name_c = CString::new(name.as_bytes())
        .map_err(|_| FsError::Io(format!("{}: パスに NUL が含まれる", to.display())))?;
    match stat_at(staging.fd.as_raw_fd(), &name_c, libc::AT_SYMLINK_NOFOLLOW) {
        Ok(_) => Ok(()),
        Err(e) => Err(FsError::Io(format!(
            "{}: 写した物が作業用のフォルダに無い(パスの途中が変わった可能性がある): {e}",
            to.display()
        ))),
    }
}

/// 失敗したコピーの作業用フォルダを片付ける。片付けられなければ、残った場所を文言に足す。
///
/// パスが作業用フォルダそのものを指していると確かめられたときだけ消す(途中を差し替えられていたら、
/// 差し替えた先を消さないよう、触らずに残った場所を知らせる)。
fn clean_up_staging(staging: &Staging, error: FsError) -> FsError {
    if !staging.path_is_still_here() {
        return FsError::Io(format!(
            "{error}。作りかけが {} に残った可能性がある(パスの途中が変わったため片付けなかった)",
            staging.path.display()
        ));
    }
    match fs::remove_dir_all(&staging.path) {
        Ok(()) => error,
        Err(e) if e.kind() == io::ErrorKind::NotFound => error,
        Err(e) => FsError::Io(format!(
            "{error}。作りかけが {} に残った(片付けられなかった: {e})",
            staging.path.display()
        )),
    }
}

/// `renameatx_np(from_fd, from_name, to_fd, to_name, flags)`。fd はどちらも押さえた親のフォルダ。
/// `RENAME_EXCL` なら、宛先に何かあれば何も変えずに `EEXIST` で失敗する。
fn rename_at(from: (RawFd, &CStr), to: (RawFd, &CStr), flags: libc::c_uint) -> io::Result<()> {
    // SAFETY: fd はどちらも開いているフォルダ、名前はどちらも NUL で終わる有効な C 文字列で、
    // この呼び出しの間は生きている
    let rc = unsafe { libc::renameatx_np(from.0, from.1.as_ptr(), to.0, to.1.as_ptr(), flags) };
    if rc == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// `mkdirat(fd, name, mode)`。既にあれば(リンクでも)`EEXIST` で失敗する。
fn mkdir_at(fd: RawFd, name: &CStr, mode: libc::mode_t) -> io::Result<()> {
    // SAFETY: fd は開いているフォルダ、名前は NUL で終わる有効な C 文字列
    if unsafe { libc::mkdirat(fd, name.as_ptr(), mode) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// `fstatat(fd, name, flags)`。
fn stat_at(fd: RawFd, name: &CStr, flags: libc::c_int) -> io::Result<libc::stat> {
    let mut st = MaybeUninit::<libc::stat>::uninit();
    // SAFETY: fd は開いているフォルダ、名前は NUL で終わる有効な C 文字列、st は書き込める領域
    if unsafe { libc::fstatat(fd, name.as_ptr(), st.as_mut_ptr(), flags) } == -1 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fstatat が成功したので st は埋まっている
    Ok(unsafe { st.assume_init() })
}

/// `fstat(fd)`。
fn fstat(fd: RawFd) -> io::Result<libc::stat> {
    let mut st = MaybeUninit::<libc::stat>::uninit();
    // SAFETY: fd は開いている fd、st は書き込める領域
    if unsafe { libc::fstat(fd, st.as_mut_ptr()) } == -1 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fstat が成功したので st は埋まっている
    Ok(unsafe { st.assume_init() })
}

fn c_path(path: &Path) -> Result<CString, FsError> {
    CString::new(path.as_os_str().as_bytes())
        .map_err(|_| FsError::Io(format!("{}: パスに NUL が含まれる", path.display())))
}

/// 移動の失敗を振り分ける。元が無いときは見つからない、宛先があるときは衝突。
fn move_error(src: &ParentDir, from: &Path, to: &Path, e: &io::Error) -> FsError {
    match e.raw_os_error() {
        Some(libc::ENOENT) if src.stat_child().is_err() => {
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
