//! ルートフォルダの正規化と、開いたままにしたフォルダの現在位置(設計書 §4 の `watch/`、§1.4 #13、
//! §7.1「ルートフォルダへの追従」、FR-007・FR-010)。
//!
//! - [`FsRootLocator::canonical_dir`] は、シンボリックリンクを解決し、ディスク上の大文字・小文字の表記にそろえた絶対パスを返す
//! - [`FsRootLocator::open`] は、フォルダを `O_EVTONLY` で開いて持つ。改名・移動のあとも `fcntl(F_GETPATH)` で
//!   現在のパスが取れ、削除後は取れない
//!
//! ファイル監視(FSEvents。M3)もこのモジュールに足す。

use std::ffi::{CStr, OsStr};
use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use arin_core::ports::{OpenRoot, RootError, RootLocatorPort};

/// ファイルシステムでルートフォルダを調べる。状態を持たない。
#[derive(Debug, Clone, Copy, Default)]
pub struct FsRootLocator;

impl FsRootLocator {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl RootLocatorPort for FsRootLocator {
    fn canonical_dir(&self, path: &Path) -> Result<PathBuf, RootError> {
        // macOS の realpath(3)(`fs::canonicalize`)は、シンボリックリンクの解決に加えて、
        // 各要素をディスク上の大文字・小文字の表記にそろえる(tests/root.rs で確認)
        let real = fs::canonicalize(path).map_err(|e| root_error(path, &e))?;
        let metadata = fs::metadata(&real).map_err(|e| root_error(&real, &e))?;
        if !metadata.is_dir() {
            return Err(RootError::NotADirectory(real.display().to_string()));
        }
        Ok(real)
    }

    fn open(&self, path: &Path) -> Result<Box<dyn OpenRoot>, RootError> {
        // O_EVTONLY: 通知のためだけに開く。外付けのボリュームの取り外しを妨げない。
        // O_DIRECTORY: フォルダでなければ ENOTDIR で失敗する。std は O_CLOEXEC を付ける
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_EVTONLY | libc::O_DIRECTORY)
            .open(path)
            .map_err(|e| root_error(path, &e))?;
        Ok(Box::new(FdOpenRoot { file }))
    }
}

/// 開いたままにしたフォルダ。持っている間、フォルダの記述子を 1 つ使う。
#[derive(Debug)]
pub struct FdOpenRoot {
    file: File,
}

/// 記述子のパスと、そのパスにいまあるものが食い違ったときに取り直す回数(改名と同時に調べたとき用)。
const PATH_ATTEMPTS: usize = 3;

impl OpenRoot for FdOpenRoot {
    fn current_path(&self) -> Result<PathBuf, RootError> {
        // F_GETPATH は、フォルダが削除されたあとも消える前のパスを返す(macOS 26 の APFS。tests/root.rs で確認)。
        // 削除後は記述子の fstat が ENOENT になるので、それで見分ける。
        // 念のため、返ったパスにいまあるものが開いたフォルダと同じ(装置番号と inode 番号が一致)かも確かめる
        let opened = self.file.metadata().map_err(|e| fd_error(&e))?;
        let mut last = PathBuf::new();
        for _ in 0..PATH_ATTEMPTS {
            let path = fd_path(&self.file).map_err(|e| fd_error(&e))?;
            match fs::symlink_metadata(&path) {
                Ok(m) if m.dev() == opened.dev() && m.ino() == opened.ino() => return Ok(path),
                // 別のものがある、または何も無い。改名の途中かもしれないので取り直す
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(root_error(&path, &e)),
            }
            last = path;
        }
        Err(RootError::NotFound(format!(
            "{}: 開いたフォルダはもうそこに無い",
            last.display()
        )))
    }
}

/// 開いた記述子への操作のエラー。ENOENT はフォルダが削除されたことを表す。
fn fd_error(e: &io::Error) -> RootError {
    if e.raw_os_error() == Some(libc::ENOENT) {
        RootError::NotFound(format!("フォルダは削除された: {e}"))
    } else {
        RootError::Io(e.to_string())
    }
}

/// `fcntl(F_GETPATH)` で、開いている記述子のいまのパスを取る。
fn fd_path(file: &File) -> io::Result<PathBuf> {
    // F_GETPATH は MAXPATHLEN(1024)バイトの領域を求める
    let mut buf = [0u8; libc::MAXPATHLEN as usize];
    // SAFETY: 記述子は `file` が持っていて、この呼び出しの間は閉じられない。
    // `buf` は MAXPATHLEN バイトの書き込める領域で、OS は終端の NUL を含めてその中にだけ書く
    let rc = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETPATH, buf.as_mut_ptr()) };
    if rc == -1 {
        return Err(io::Error::last_os_error());
    }
    let path = CStr::from_bytes_until_nul(&buf)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "F_GETPATH の結果に終端が無い"))?;
    Ok(PathBuf::from(OsStr::from_bytes(path.to_bytes())))
}

/// 入出力のエラーを [`RootError`] に振り分ける。
fn root_error(path: &Path, e: &io::Error) -> RootError {
    let message = format!("{}: {e}", path.display());
    match e.kind() {
        io::ErrorKind::NotFound => RootError::NotFound(message),
        io::ErrorKind::NotADirectory => RootError::NotADirectory(message),
        _ => RootError::Io(message),
    }
}
