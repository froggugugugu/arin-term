//! 文書の読み書き(ADR-015 の 5、計画書 T81c)。[`LocalDocuments`] が `arin_core::ports::document::DocumentPort` を実装する。
//!
//! `local.rs` と同じく、親のフォルダを `O_NOFOLLOW_ANY` で開き(途中にリンクがあれば `ELOOP` で失敗)、
//! その fd から最後の名前を `fstatat(AT_SYMLINK_NOFOLLOW)`・`openat(O_NOFOLLOW)` で扱う(リンク自体は開かない)。
//!
//! 置き換え(`replace`)の手順:
//! 1. 対象を調べる。無い → `Missing`。ふつうのファイルでない → 失敗。読み取り専用 → 失敗。版が違い中身も違う → `Changed`
//! 2. 同じフォルダに `.arin-save-<pid>-<連番>` を `O_CREAT | O_EXCL | O_NOFOLLOW` で作り、元のファイルから
//!    `fcopyfile(COPYFILE_METADATA)` で権限・ACL・拡張属性を写す
//! 3. 中身を書いて `fsync` する
//! 4. もう一度 1 の `stat` を調べ、変わっていなければ `renameatx_np(…, 0)` で置き換える
//! 5. 置き換えたファイルを `fstat` して新しい版を返す
//!
//! 2〜4 で失敗したら一時ファイルは消さずに残し、場所を知らせ先(`LeftoverReport`)に渡して文言に含める
//! (削除の経路を作らない。元のファイルは変わっていない)。新しく作る(`create_new`)は 2 の写しを飛ばし、
//! 4 を `RENAME_EXCL`(既にあれば失敗)で行う。

use std::ffi::{CStr, CString};
use std::fs::File;
use std::io::{self, Read, Write};
use std::mem::MaybeUninit;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use arin_core::editor::{DOCUMENT_LIMIT_BYTES, FileStamp, FileStat, ReadOnlyReason, same_content};
use arin_core::ports::document::{DocumentAccessError, DocumentPort, ReadDocument, ReplaceOutcome};

use super::LeftoverReport;

/// 保存の一時ファイルの名前の頭。対象と同じフォルダに一時的に現れる。ツリーの一覧と監視は
/// `arin_core::tree::exclusion::WORK_NAME_PREFIXES` で `.arin-copy-` と同じに扱う。
pub const SAVE_PREFIX: &str = ".arin-save-";
/// 一時ファイルの名前が既に使われていたときに、別の名前で作り直す回数。
const SAVE_ATTEMPTS: usize = 16;
/// 一時ファイルの権限(umask を引く前。置き換えでは作った直後に元のファイルの権限を写す)。
const NEW_FILE_MODE: libc::c_uint = 0o666;
/// 親のフォルダを押さえるときの開き方。途中か親そのものがシンボリックリンクなら `ELOOP` で失敗する。
const OPEN_PARENT_FLAGS: libc::c_int =
    libc::O_SEARCH | libc::O_DIRECTORY | libc::O_NOFOLLOW_ANY | libc::O_CLOEXEC;
/// 対象を読むときの開き方。最後の要素がリンクなら `ELOOP`。FIFO に差し替えられても待たない。
const OPEN_READ_FLAGS: libc::c_int =
    libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC;

/// 一時ファイルの名前の連番。
static SAVE_SEQ: AtomicU64 = AtomicU64::new(0);

/// 端末のファイルシステムの文書を読み、置き換え、作る。状態を持たない
/// (片付けられなかった一時ファイルの知らせ先だけを持つ)。
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalDocuments {
    leftover_report: Option<LeftoverReport>,
}

impl LocalDocuments {
    /// 一時ファイルが残っても、文言に含めるだけで知らせない。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 一時ファイルを片付けられなかったとき、その場所を `report` に渡す(消す処理は足さない)。
    #[must_use]
    pub fn with_leftover_report(report: LeftoverReport) -> Self {
        Self {
            leftover_report: Some(report),
        }
    }

    /// 一時ファイルを残した。知らせ先に渡し、文言に場所を足す。
    fn left(&self, temp: &TempFile, error: DocumentAccessError) -> DocumentAccessError {
        self.note_left(temp);
        DocumentAccessError::Io(format!(
            "{error}。一時ファイル {} が残った",
            temp.path.display()
        ))
    }

    fn note_left(&self, temp: &TempFile) {
        if let Some(report) = self.leftover_report {
            report(&temp.path);
        }
    }
}

impl DocumentPort for LocalDocuments {
    fn read(&self, path: &Path) -> Result<ReadDocument, DocumentAccessError> {
        let target = Target::open(path)?;
        let (bytes, st) = target.read_regular()?;
        let read_only = target.read_only_reason(&st);
        Ok(ReadDocument {
            stamp: FileStamp::new(file_stat(&st), &bytes),
            bytes,
            read_only,
        })
    }

    fn stat(&self, path: &Path) -> Result<FileStamp, DocumentAccessError> {
        let target = Target::open(path)?;
        let (bytes, st) = target.read_regular()?;
        Ok(FileStamp::new(file_stat(&st), &bytes))
    }

    fn replace(
        &self,
        path: &Path,
        bytes: &[u8],
        expected: Option<&FileStamp>,
    ) -> Result<ReplaceOutcome, DocumentAccessError> {
        check_size(bytes.len() as u64)?;
        let target = Target::open(path)?;
        // 1. 対象を調べる
        let st = match target.lstat() {
            Ok(st) => st,
            Err(e) if e.raw_os_error() == Some(libc::ENOENT) => return Ok(ReplaceOutcome::Missing),
            Err(e) => return Err(io_error(path, &e)),
        };
        classify(path, &st)?;
        if let Some(reason) = target.read_only_reason(&st) {
            return Err(DocumentAccessError::ReadOnly(reason));
        }
        if let Some(expected) = expected
            && !expected.same_stat(&file_stat(&st))
        {
            let current = match target.read_regular() {
                Ok((current, cst)) => FileStamp::new(file_stat(&cst), &current),
                Err(DocumentAccessError::NotFound(_)) => return Ok(ReplaceOutcome::Missing),
                Err(e) => return Err(e),
            };
            if !same_content(expected, &current) {
                return Ok(ReplaceOutcome::Changed(current));
            }
        }
        let source = target.open_read()?;
        let source_st = fstat(source.as_raw_fd()).map_err(|e| io_error(path, &e))?;
        if !same_stat(&st, &source_st) {
            return Err(DocumentAccessError::Io(format!(
                "{}: 調べている間に差し替えられた",
                path.display()
            )));
        }

        // 2. 一時ファイルを作り、権限・ACL・拡張属性を写す
        let temp = TempFile::create(&target)?;
        // SAFETY: どちらも開いている fd。状態(copyfile_state_t)は渡さない(NULL は許されている)
        let rc = unsafe {
            libc::fcopyfile(
                source.as_raw_fd(),
                temp.file.as_raw_fd(),
                std::ptr::null_mut(),
                libc::COPYFILE_METADATA,
            )
        };
        if rc < 0 {
            let e = io::Error::last_os_error();
            return Err(self.left(&temp, io_error(path, &e)));
        }
        // 3. 中身を書いて fsync する
        if let Err(e) = temp.write_all(bytes) {
            return Err(self.left(&temp, io_error(path, &e)));
        }
        // 4. もう一度調べてから置き換える
        match target.lstat() {
            Ok(now) if same_stat(&st, &now) => {}
            Ok(_) => {
                self.note_left(&temp);
                return match target.read_regular() {
                    Ok((current, cst)) => Ok(ReplaceOutcome::Changed(FileStamp::new(
                        file_stat(&cst),
                        &current,
                    ))),
                    Err(DocumentAccessError::NotFound(_)) => Ok(ReplaceOutcome::Missing),
                    Err(e) => Err(self.left(&temp, e)),
                };
            }
            Err(e) if e.raw_os_error() == Some(libc::ENOENT) => {
                self.note_left(&temp);
                return Ok(ReplaceOutcome::Missing);
            }
            Err(e) => return Err(self.left(&temp, io_error(path, &e))),
        }
        if let Err(e) = rename_at((target.dir.as_raw_fd(), &temp.name), target.at(), 0) {
            return Err(self.left(&temp, io_error(path, &e)));
        }
        // 5. 新しい版
        let st = fstat(temp.file.as_raw_fd()).map_err(|e| io_error(path, &e))?;
        Ok(ReplaceOutcome::Replaced(FileStamp::new(
            file_stat(&st),
            bytes,
        )))
    }

    fn create_new(&self, path: &Path, bytes: &[u8]) -> Result<FileStamp, DocumentAccessError> {
        check_size(bytes.len() as u64)?;
        let target = Target::open(path)?;
        // 先に分かる衝突はすぐ返す(上書きしないことの保証は、最後の RENAME_EXCL が受け持つ)
        match target.lstat() {
            Ok(_) => {
                return Err(DocumentAccessError::AlreadyExists(
                    path.display().to_string(),
                ));
            }
            Err(e) if e.raw_os_error() == Some(libc::ENOENT) => {}
            Err(e) => return Err(io_error(path, &e)),
        }
        let temp = TempFile::create(&target)?;
        if let Err(e) = temp.write_all(bytes) {
            return Err(self.left(&temp, io_error(path, &e)));
        }
        if let Err(e) = rename_at(
            (target.dir.as_raw_fd(), &temp.name),
            target.at(),
            libc::RENAME_EXCL,
        ) {
            let error = io_error(path, &e);
            self.note_left(&temp);
            return Err(match error {
                DocumentAccessError::AlreadyExists(message) => DocumentAccessError::AlreadyExists(
                    format!("{message}。一時ファイル {} が残った", temp.path.display()),
                ),
                other => DocumentAccessError::Io(format!(
                    "{other}。一時ファイル {} が残った",
                    temp.path.display()
                )),
            });
        }
        let st = fstat(temp.file.as_raw_fd()).map_err(|e| io_error(path, &e))?;
        Ok(FileStamp::new(file_stat(&st), bytes))
    }
}

/// 対象の、押さえた親のフォルダと最後の名前。
struct Target {
    /// 親のフォルダ(`O_SEARCH`。途中にリンクが無いことを確かめて開いた)。
    dir: OwnedFd,
    /// 最後の名前(`/` を含まない)。
    name: CString,
    /// 親のフォルダのパス(一時ファイルの場所の文言のため)。
    dir_path: PathBuf,
    /// 対象のパス(文言のため)。
    path: PathBuf,
}

impl Target {
    /// `path` の親を、途中にシンボリックリンクが無いことを確かめて開く。`path` は絶対パスで `..` を含まないこと。
    fn open(path: &Path) -> Result<Self, DocumentAccessError> {
        if !path.is_absolute() {
            return Err(DocumentAccessError::Io(format!(
                "絶対パスではない: {}",
                path.display()
            )));
        }
        if path.components().any(|c| c == Component::ParentDir) {
            return Err(DocumentAccessError::Io(format!(
                "`..` を含む: {}",
                path.display()
            )));
        }
        let normalized: PathBuf = path.components().collect();
        let (Some(dir), Some(name)) = (normalized.parent(), normalized.file_name()) else {
            return Err(DocumentAccessError::Io(format!(
                "名前が無い: {}",
                path.display()
            )));
        };
        let dir_c = c_string(dir.as_os_str().as_bytes(), path)?;
        let name = c_string(name.as_bytes(), path)?;
        // SAFETY: NUL で終わる有効な C 文字列。O_CREAT を含まないので 3 つ目の引数は要らない
        let fd = unsafe { libc::open(dir_c.as_ptr(), OPEN_PARENT_FLAGS) };
        if fd < 0 {
            let e = io::Error::last_os_error();
            return Err(match e.raw_os_error() {
                Some(libc::ELOOP) => DocumentAccessError::Io(format!(
                    "{}: パスの途中にシンボリックリンクがある({e})",
                    dir.display()
                )),
                Some(libc::ENOENT) => {
                    DocumentAccessError::NotFound(format!("{}: {e}", dir.display()))
                }
                _ => DocumentAccessError::Io(format!("{}: {e}", dir.display())),
            });
        }
        Ok(Self {
            // SAFETY: open が返した、ほかに持ち主のいない fd
            dir: unsafe { OwnedFd::from_raw_fd(fd) },
            name,
            dir_path: dir.to_path_buf(),
            path: normalized,
        })
    }

    fn at(&self) -> (RawFd, &CStr) {
        (self.dir.as_raw_fd(), &self.name)
    }

    /// 最後の名前の項目を調べる(リンクはたどらない)。
    fn lstat(&self) -> io::Result<libc::stat> {
        stat_at(self.dir.as_raw_fd(), &self.name, libc::AT_SYMLINK_NOFOLLOW)
    }

    /// 対象を読むために開く(リンクは開かない)。
    fn open_read(&self) -> Result<OwnedFd, DocumentAccessError> {
        // SAFETY: fd は開いているフォルダ、名前は NUL で終わる有効な C 文字列。O_CREAT を含まない
        let fd = unsafe { libc::openat(self.dir.as_raw_fd(), self.name.as_ptr(), OPEN_READ_FLAGS) };
        if fd < 0 {
            return Err(io_error(&self.path, &io::Error::last_os_error()));
        }
        // SAFETY: openat が返した、ほかに持ち主のいない fd
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }

    /// ふつうのファイルであることを確かめてから、上限までの中身と、開いた fd の `stat` を返す。
    fn read_regular(&self) -> Result<(Vec<u8>, libc::stat), DocumentAccessError> {
        let st = self.lstat().map_err(|e| io_error(&self.path, &e))?;
        classify(&self.path, &st)?;
        check_size(st.st_size as u64)?;
        let fd = self.open_read()?;
        let st = fstat(fd.as_raw_fd()).map_err(|e| io_error(&self.path, &e))?;
        classify(&self.path, &st)?;
        check_size(st.st_size as u64)?;
        let mut bytes = Vec::new();
        File::from(fd)
            .take(DOCUMENT_LIMIT_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| io_error(&self.path, &e))?;
        check_size(bytes.len() as u64)?;
        Ok((bytes, st))
    }

    /// 読み取り専用で開く理由(ハードリンク → 持ち主 → 書けない → フォルダに書けない の順で最初のもの)。
    fn read_only_reason(&self, st: &libc::stat) -> Option<ReadOnlyReason> {
        if st.st_nlink > 1 {
            return Some(ReadOnlyReason::HardLink);
        }
        // SAFETY: 引数の無い呼び出し
        if st.st_uid != unsafe { libc::geteuid() } {
            return Some(ReadOnlyReason::NotOwner);
        }
        if !self.writable(&self.name) {
            return Some(ReadOnlyReason::NotWritable);
        }
        if !self.writable(c".") {
            return Some(ReadOnlyReason::FolderNotWritable);
        }
        None
    }

    /// 親のフォルダから見た `name` に、実効の利用者で書けるか。
    fn writable(&self, name: &CStr) -> bool {
        // SAFETY: fd は開いているフォルダ、名前は NUL で終わる有効な C 文字列
        unsafe {
            libc::faccessat(
                self.dir.as_raw_fd(),
                name.as_ptr(),
                libc::W_OK,
                libc::AT_EACCESS,
            ) == 0
        }
    }
}

/// 保存の一時ファイル。対象と同じフォルダに作り、書き込み用に開いて持つ。
struct TempFile {
    file: File,
    name: CString,
    path: PathBuf,
}

impl TempFile {
    /// 対象と同じフォルダに、この保存だけが使う一時ファイルを作る(既にある名前は触らずに別の名前にする)。
    fn create(target: &Target) -> Result<Self, DocumentAccessError> {
        let flags =
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC;
        for _ in 0..SAVE_ATTEMPTS {
            let n = SAVE_SEQ.fetch_add(1, Ordering::Relaxed);
            let file_name = format!("{SAVE_PREFIX}{}-{n}", std::process::id());
            let path = target.dir_path.join(&file_name);
            let name = CString::new(file_name).expect("一時ファイルの名前に NUL は無い");
            // SAFETY: fd は開いているフォルダ、名前は NUL で終わる有効な C 文字列。O_CREAT なので権限を渡す
            let fd = unsafe {
                libc::openat(target.dir.as_raw_fd(), name.as_ptr(), flags, NEW_FILE_MODE)
            };
            if fd < 0 {
                let e = io::Error::last_os_error();
                if e.raw_os_error() == Some(libc::EEXIST) {
                    continue;
                }
                return Err(DocumentAccessError::Io(format!(
                    "{}: 一時ファイルを作れなかった({e})",
                    target.dir_path.display()
                )));
            }
            return Ok(Self {
                // SAFETY: openat が返した、ほかに持ち主のいない fd
                file: File::from(unsafe { OwnedFd::from_raw_fd(fd) }),
                name,
                path,
            });
        }
        Err(DocumentAccessError::Io(format!(
            "{}: 一時ファイルの名前が空いていない",
            target.dir_path.display()
        )))
    }

    /// 中身を書き、ディスクまで書き出す。
    fn write_all(&self, bytes: &[u8]) -> io::Result<()> {
        let mut file = &self.file;
        file.write_all(bytes)?;
        file.sync_all()
    }
}

/// ふつうのファイルでなければ種類に応じた失敗にする。
fn classify(path: &Path, st: &libc::stat) -> Result<(), DocumentAccessError> {
    let message = || path.display().to_string();
    match st.st_mode & libc::S_IFMT {
        libc::S_IFREG => Ok(()),
        libc::S_IFLNK => Err(DocumentAccessError::Link(message())),
        libc::S_IFDIR => Err(DocumentAccessError::Directory(message())),
        _ => Err(DocumentAccessError::NotAFile(message())),
    }
}

fn check_size(size: u64) -> Result<(), DocumentAccessError> {
    if size > DOCUMENT_LIMIT_BYTES {
        return Err(DocumentAccessError::TooLarge { size });
    }
    Ok(())
}

/// 版に使う `stat` の 4 つ。
fn file_stat(st: &libc::stat) -> FileStat {
    FileStat {
        dev: st.st_dev as u64,
        ino: st.st_ino,
        size: st.st_size as u64,
        mtime_ns: st.st_mtime * 1_000_000_000 + st.st_mtime_nsec,
    }
}

fn same_stat(a: &libc::stat, b: &libc::stat) -> bool {
    file_stat(a) == file_stat(b)
}

/// 入出力のエラーを [`DocumentAccessError`] に振り分ける。
fn io_error(path: &Path, e: &io::Error) -> DocumentAccessError {
    let message = format!("{}: {e}", path.display());
    match e.raw_os_error() {
        Some(libc::ENOENT) => DocumentAccessError::NotFound(message),
        // openat(O_NOFOLLOW) が最後の要素のリンクを拒んだ
        Some(libc::ELOOP) => DocumentAccessError::Link(message),
        Some(libc::EEXIST) => DocumentAccessError::AlreadyExists(message),
        _ => DocumentAccessError::Io(message),
    }
}

fn c_string(bytes: &[u8], path: &Path) -> Result<CString, DocumentAccessError> {
    CString::new(bytes)
        .map_err(|_| DocumentAccessError::Io(format!("{}: パスに NUL が含まれる", path.display())))
}

/// `renameatx_np(from_fd, from_name, to_fd, to_name, flags)`。fd はどちらも押さえた親のフォルダ。
/// `flags` が 0 なら宛先を置き換え、`RENAME_EXCL` なら宛先に何かあれば何も変えずに `EEXIST` で失敗する。
fn rename_at(from: (RawFd, &CStr), to: (RawFd, &CStr), flags: libc::c_uint) -> io::Result<()> {
    // SAFETY: fd はどちらも開いているフォルダ、名前はどちらも NUL で終わる有効な C 文字列で、
    // この呼び出しの間は生きている
    let rc = unsafe { libc::renameatx_np(from.0, from.1.as_ptr(), to.0, to.1.as_ptr(), flags) };
    if rc == -1 {
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
