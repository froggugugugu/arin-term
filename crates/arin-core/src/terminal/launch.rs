//! ログインシェルの起動の仕様(設計書 §7.1「ターミナルの起動」、§1.3 #12、FR-039・FR-040)。
//!
//! 起動するシェル、`argv[0]`、cwd、環境変数は Rust 側で決め、画面からは指定させない(§12)。
//! 入力(シェルのパス、cwd、親の環境変数、システムの言語設定)は引数で受け取る。
//! 環境変数を読むのも、ユーザー情報から既定のシェルを引くのも組み立て側の仕事で、ここでは何も読まない。

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use thiserror::Error;

/// 親(アプリが受け取った環境変数)から値を引き継ぐ名前。これ以外は引き継がない。
const INHERITED_ENV_NAMES: [&str; 6] = [
    "HOME",
    "USER",
    "LOGNAME",
    "TMPDIR",
    "SSH_AUTH_SOCK",
    "__CF_USER_TEXT_ENCODING",
];

/// Finder から起動したアプリが受け取る値と同じ。残りはログインシェルが設定ファイルから組み立てる。
const LOGIN_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";
const TERM: &str = "xterm-256color";
const COLORTERM: &str = "truecolor";
/// 入れる値は設計書 §15.3 #12 で確かめる。
const TERM_PROGRAM: &str = "Arin";
/// システムの言語設定から `LANG` を作れないときの値。UTF-8 にして全角の入出力を崩さない。
const FALLBACK_LANG: &str = "en_US.UTF-8";

/// 起動の仕様を組み立てられなかった理由。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ShellLaunchError {
    /// シェルのパスが空。
    #[error("起動するシェルのパスが空になっている")]
    EmptyShell,
    /// シェルのパスが相対パス。解決先が cwd や PATH で変わるので受け付けない。
    #[error("起動するシェルのパス {} が相対パスになっている(絶対パスで指定する)", .0.display())]
    RelativeShell(PathBuf),
    /// シェルのパスにファイル名が無い(`/` や `..` で終わる)。`argv[0]` を作れない。
    #[error("起動するシェルのパス {} にファイル名が無い", .0.display())]
    ShellWithoutFileName(PathBuf),
    /// cwd が空。
    #[error("ターミナルの作業フォルダが空になっている")]
    EmptyCwd,
    /// cwd が相対パス。GUI のアプリは起動元によって作業ディレクトリが変わるので受け付けない。
    #[error("ターミナルの作業フォルダ {} が相対パスになっている(絶対パスで指定する)", .0.display())]
    RelativeCwd(PathBuf),
}

/// ログインシェルの起動の仕様。PTY のポート(`PtyPort::spawn`)に渡す。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellLaunchSpec {
    program: PathBuf,
    argv0: OsString,
    cwd: PathBuf,
    env: BTreeMap<OsString, OsString>,
}

impl ShellLaunchSpec {
    /// 起動の仕様を組み立てる。
    ///
    /// - `shell`: 起動するシェルの絶対パス(ユーザー情報に登録された既定のシェル)。`argv[0]` は `-` とそのファイル名にする
    /// - `cwd`: 作業フォルダの絶対パス(プロジェクトのルートなど)。存在するかはここでは見ない
    /// - `parent_env`: アプリが受け取った環境変数。許可リストの名前で、空でない値だけを引き継ぐ
    /// - `system_locale`: システムの言語設定の識別子(例 `ja_JP`)。`LANG` を作る
    ///
    /// 環境変数は親から丸ごと引き継がず、許可リストと固定の値(`SHELL`、`PATH`、`TERM`、`COLORTERM`、`LANG`、
    /// `TERM_PROGRAM`)だけで組み立てる。
    ///
    /// # Errors
    ///
    /// シェルのパスが空・相対パス・ファイル名なしのとき、cwd が空・相対パスのとき。
    pub fn new<I, K, V>(
        shell: &Path,
        cwd: &Path,
        parent_env: I,
        system_locale: Option<&str>,
    ) -> Result<Self, ShellLaunchError>
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        if shell.as_os_str().is_empty() {
            return Err(ShellLaunchError::EmptyShell);
        }
        if !shell.is_absolute() {
            return Err(ShellLaunchError::RelativeShell(shell.to_path_buf()));
        }
        let file_name = shell
            .file_name()
            .ok_or_else(|| ShellLaunchError::ShellWithoutFileName(shell.to_path_buf()))?;
        if cwd.as_os_str().is_empty() {
            return Err(ShellLaunchError::EmptyCwd);
        }
        if !cwd.is_absolute() {
            return Err(ShellLaunchError::RelativeCwd(cwd.to_path_buf()));
        }

        let mut argv0 = OsString::from("-");
        argv0.push(file_name);

        let mut env: BTreeMap<OsString, OsString> = parent_env
            .into_iter()
            .filter(|(name, value)| {
                !value.as_ref().is_empty()
                    && INHERITED_ENV_NAMES
                        .iter()
                        .any(|allowed| name.as_ref() == OsStr::new(allowed))
            })
            .map(|(name, value)| (name.as_ref().to_owned(), value.as_ref().to_owned()))
            .collect();
        env.insert("SHELL".into(), shell.as_os_str().to_owned());
        env.insert("PATH".into(), LOGIN_PATH.into());
        env.insert("TERM".into(), TERM.into());
        env.insert("COLORTERM".into(), COLORTERM.into());
        env.insert("TERM_PROGRAM".into(), TERM_PROGRAM.into());
        env.insert("LANG".into(), lang_from_locale(system_locale).into());

        Ok(Self {
            program: shell.to_path_buf(),
            argv0,
            cwd: cwd.to_path_buf(),
            env,
        })
    }

    /// 起動するプログラム(シェルの絶対パス)。
    #[must_use]
    pub fn program(&self) -> &Path {
        &self.program
    }

    /// `argv[0]`(例 `-zsh`)。先頭の `-` でシェルはログインシェルとして動く。
    #[must_use]
    pub fn argv0(&self) -> &OsStr {
        &self.argv0
    }

    /// 作業フォルダ。
    #[must_use]
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    /// シェルに渡す環境変数の全部。これ以外の変数は渡さない。名前の順に並ぶ。
    #[must_use]
    pub fn env(&self) -> &BTreeMap<OsString, OsString> {
        &self.env
    }
}

/// システムの言語設定の識別子から `LANG` を作る。
///
/// 識別子は `言語[_-文字体系][_-地域][@付加情報]` の形(例 `ja_JP`、`zh-Hans_CN`、`en_US@rg=jpzzzz`)。
/// 言語(小文字 2〜3 字)と地域(大文字 2 字)を取り出して `言語_地域.UTF-8` にする。
/// 取り出せなければ `en_US.UTF-8` にする(`ja.UTF-8` のような存在しないロケール名を作らない)。
fn lang_from_locale(locale: Option<&str>) -> String {
    let parse = |id: &str| -> Option<String> {
        let id = id.split('@').next().unwrap_or_default();
        let mut parts = id.split(['_', '-']);
        let language = parts.next()?;
        let region = parts.next_back()?;
        let is_language =
            (2..=3).contains(&language.len()) && language.bytes().all(|b| b.is_ascii_lowercase());
        let is_region = region.len() == 2 && region.bytes().all(|b| b.is_ascii_uppercase());
        (is_language && is_region).then(|| format!("{language}_{region}.UTF-8"))
    };
    locale
        .and_then(parse)
        .unwrap_or_else(|| FALLBACK_LANG.to_owned())
}
