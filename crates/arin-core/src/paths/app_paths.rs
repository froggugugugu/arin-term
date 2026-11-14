//! 状態の置き場所の決定(設計書 §11 の 1、§10.2)。
//!
//! 既定は `~/Library/Application Support/Arin/`(C-01b)。テストと起動スモークは差し替えの指定で
//! 一時フォルダを指し、本物の置き場所を読まない・書かない。
//!
//! 入力(ホームディレクトリ、差し替えの指定)は引数で受け取る。環境変数を読むのは組み立て側(`src-tauri`)で、
//! ここでは環境変数もファイルシステムも触らない(パスの組み立てと判定だけ)。

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use thiserror::Error;

/// 起動スモークの印のファイル名。状態フォルダの直下に置く。
///
/// `scripts/smoke.mjs` がこの名前のファイルが現れるのを待つ。変えるときは両方を合わせる。
pub const SMOKE_READY_MARKER_FILE_NAME: &str = "smoke-ready";

/// ホームから見た既定の状態フォルダ。
const DEFAULT_STATE_DIR_UNDER_HOME: [&str; 3] = ["Library", "Application Support", "Arin"];

/// 置き場所を決められなかった理由。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AppPathsError {
    /// 差し替えの指定が相対パス。GUI のアプリは起動元によって作業ディレクトリが変わるので受け付けない。
    #[error("状態フォルダの差し替えの指定 {} が相対パスになっている(絶対パスで指定する)", .0.display())]
    RelativeOverride(PathBuf),
    /// 差し替えの指定が無く、ホームディレクトリも分からない。
    #[error("ホームディレクトリが分からないため、状態フォルダを決められない")]
    HomeUnknown,
    /// ホームディレクトリが相対パス。
    #[error("ホームディレクトリ {} が相対パスになっている", .0.display())]
    RelativeHome(PathBuf),
}

/// 状態の置き場所。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    state_dir: PathBuf,
    overridden: bool,
}

impl AppPaths {
    /// 置き場所を決める。
    ///
    /// - 差し替えの指定があれば、それを状態フォルダにする(ホームは見ない)。絶対パスでなければ失敗する
    /// - 差し替えの指定が無い、または空文字列なら、`<home>/Library/Application Support/Arin` にする
    ///
    /// # Errors
    ///
    /// 差し替えの指定が相対パスのとき、または差し替えが無くホームが分からない(無い・空)か相対パスのとき。
    pub fn resolve(
        home: Option<&Path>,
        state_dir_override: Option<&OsStr>,
    ) -> Result<Self, AppPathsError> {
        if let Some(value) = state_dir_override.filter(|v| !v.is_empty()) {
            let dir = PathBuf::from(value);
            if !dir.is_absolute() {
                return Err(AppPathsError::RelativeOverride(dir));
            }
            return Ok(Self {
                state_dir: dir,
                overridden: true,
            });
        }

        let home = home
            .filter(|h| !h.as_os_str().is_empty())
            .ok_or(AppPathsError::HomeUnknown)?;
        if !home.is_absolute() {
            return Err(AppPathsError::RelativeHome(home.to_path_buf()));
        }
        let state_dir = DEFAULT_STATE_DIR_UNDER_HOME
            .iter()
            .fold(home.to_path_buf(), |dir, part| dir.join(part));
        Ok(Self {
            state_dir,
            overridden: false,
        })
    }

    /// 状態フォルダ。
    #[must_use]
    pub fn state_dir(&self) -> &Path {
        &self.state_dir
    }

    /// 差し替えの指定で決まったかどうか。
    #[must_use]
    pub fn is_overridden(&self) -> bool {
        self.overridden
    }

    /// 起動スモークの印のファイルの場所。
    ///
    /// スモークが求められていて、かつ状態フォルダが差し替えられているときだけ返す。
    /// 差し替えていなければ、スモークを求められても本物の状態フォルダに印を書かないよう `None` を返す。
    #[must_use]
    pub fn smoke_ready_marker(&self, smoke_requested: bool) -> Option<PathBuf> {
        (smoke_requested && self.overridden)
            .then(|| self.state_dir.join(SMOKE_READY_MARKER_FILE_NAME))
    }
}
