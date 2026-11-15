//! 状態ファイル(`state.json`)の読み書きと退避(設計書 §6.2「書き込み方法」、計画書 M2 の決定 (d))。
//!
//! 状態フォルダは作るときに渡す(`arin_core::paths::AppPaths::state_dir`)。触るのはそのフォルダと、
//! その直下の `state.json`・一時ファイル・退避の名前のファイルだけ。標準ライブラリだけで書く。

use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use arin_core::ports::state_file::{STATE_FILE_NAME, StateFileError, StateFilePort, StateFileRead};

/// 状態フォルダの権限。
const DIR_MODE: u32 = 0o700;
/// 状態ファイル(一時ファイルを含む)の権限。
const FILE_MODE: u32 = 0o600;
/// 一時ファイルの名前の後ろに付ける文字列。状態ファイルと同じフォルダに置く。
const TEMP_SUFFIX: &str = ".tmp";

/// 状態フォルダの `state.json` をファイルシステムで読み書きする。
#[derive(Debug, Clone)]
pub struct FsStateFile {
    state_dir: PathBuf,
}

impl FsStateFile {
    /// 状態フォルダを渡して作る。ここではフォルダを作らない・調べない。
    #[must_use]
    pub fn new(state_dir: PathBuf) -> Self {
        Self { state_dir }
    }

    fn state_path(&self) -> PathBuf {
        self.state_dir.join(STATE_FILE_NAME)
    }

    fn temp_path(&self) -> PathBuf {
        self.state_dir
            .join(format!("{STATE_FILE_NAME}{TEMP_SUFFIX}"))
    }

    /// 状態フォルダが無ければ 0700 で作り、あれば 0700 に揃える。親のフォルダは作らない。
    fn ensure_state_dir(&self) -> io::Result<()> {
        match DirBuilder::new().mode(DIR_MODE).create(&self.state_dir) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
        let metadata = fs::metadata(&self.state_dir)?;
        if !metadata.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::NotADirectory,
                "状態フォルダの場所にフォルダでないものがある",
            ));
        }
        if metadata.permissions().mode() & 0o777 != DIR_MODE {
            fs::set_permissions(&self.state_dir, fs::Permissions::from_mode(DIR_MODE))?;
        }
        Ok(())
    }

    /// 一時ファイルに書いて `fsync` する。前回の一時ファイルが残っていれば先に消す
    /// (シンボリックリンクならリンクだけを消すので、リンクの先には書かない)。
    fn write_temp(temp: &Path, contents: &[u8]) -> io::Result<()> {
        match fs::remove_file(temp) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let mut file: File = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(FILE_MODE)
            .open(temp)?;
        let written = (|| {
            // umask で狭まっていても広がっていても 0600 に揃える。
            file.set_permissions(fs::Permissions::from_mode(FILE_MODE))?;
            file.write_all(contents)?;
            file.sync_all()
        })();
        if written.is_err() {
            drop(file);
            let _ = fs::remove_file(temp);
        }
        written
    }
}

/// 退避の名前が状態フォルダの直下の 1 つの名前か。
fn is_plain_file_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\0'])
}

fn io_error(what: &str, path: &Path, error: &io::Error) -> StateFileError {
    StateFileError::Io(format!("{what}({}): {error}", path.display()))
}

impl StateFilePort for FsStateFile {
    fn read(&self) -> Result<StateFileRead, StateFileError> {
        let path = self.state_path();
        match fs::read(&path) {
            Ok(bytes) => Ok(StateFileRead::Contents(bytes)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(StateFileRead::Missing),
            Err(e) => Err(io_error("読めなかった", &path, &e)),
        }
    }

    fn write(&self, contents: &[u8]) -> Result<(), StateFileError> {
        self.ensure_state_dir()
            .map_err(|e| io_error("状態フォルダを用意できなかった", &self.state_dir, &e))?;
        let temp = self.temp_path();
        Self::write_temp(&temp, contents)
            .map_err(|e| io_error("一時ファイルに書けなかった", &temp, &e))?;
        let path = self.state_path();
        fs::rename(&temp, &path).map_err(|e| {
            let _ = fs::remove_file(&temp);
            io_error("置き換えられなかった", &path, &e)
        })
    }

    fn move_aside(&self, backup_name: &str) -> Result<(), StateFileError> {
        if !is_plain_file_name(backup_name) {
            return Err(StateFileError::InvalidBackupName(backup_name.to_owned()));
        }
        let path = self.state_path();
        let backup = self.state_dir.join(backup_name);
        // `rename` は同じ名前があると黙って置き換えるので、同じ名前があれば失敗する `link` で
        // 退避の名前を付けてから元の名前を外す(中身も inode も変わらない)。
        fs::hard_link(&path, &backup).map_err(|e| {
            if e.kind() == io::ErrorKind::AlreadyExists {
                StateFileError::BackupExists(backup.display().to_string())
            } else {
                io_error("退避の名前を付けられなかった", &path, &e)
            }
        })?;
        fs::remove_file(&path).map_err(|e| io_error("元の名前を外せなかった", &path, &e))
    }
}
