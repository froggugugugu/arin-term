//! 起動時の設定を環境変数から読む(設計書 §11 の 1)。
//!
//! 環境変数を読むのは `from_env` だけにして薄くする。読んだ値の解釈は `from_inputs` に引数で渡し、
//! 置き場所の規則そのものは `arin_core::paths::AppPaths` が持つ。テストは `from_inputs` を直接呼び、環境変数を書き換えない。
//! ターミナルの起動に使う値(ホームフォルダ、`USER`、親の環境変数)もここで読み、`TerminalEnv` にして渡す。
//! 画面の描画方式と全角幅の方式の指定(`ARIN_RENDERER`・`ARIN_UNICODE`。M1 の検証用、M4 の T50b で外す)もここで読み、
//! 解釈は `arin_core::launch_options::LaunchOptions::resolve` に任せる。
//! 9 枚の検証画面の指定(`ARIN_VERIFY_GRID`)は M4 の T49c・T49d で外した。指定されていても読まない(通常の起動になる)。

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use arin_core::launch_options::{
    ENV_RENDERER, ENV_UNICODE, InvalidLaunchOption, LaunchOptionInputs, LaunchOptions,
};
use arin_core::paths::{AppPaths, AppPathsError};

/// 状態フォルダの差し替え。絶対パスを指定する。空文字列は指定なしと同じ。
///
/// テストと起動スモークが、本物の `~/Library/Application Support/Arin/` を読まない・書かないために使う(設計書 §10.2)。
pub const ENV_STATE_DIR: &str = "ARIN_STATE_DIR";

/// 起動スモークの印を有効にする。値が `1` のときだけ有効。
///
/// `ENV_STATE_DIR` で状態フォルダを差し替えているときだけ効く(本物の状態フォルダには印を書かない)。
pub const ENV_SMOKE: &str = "ARIN_SMOKE";

/// 起動時の指定が使えない値だったときに、標準エラーに書く 1 行の頭。
pub const LAUNCH_WARNING_PREFIX: &str = "[arin:launch]";

/// ターミナルの起動に使う、アプリが受け取った値(設計書 §7.1「ターミナルの起動」)。
///
/// シェルへ渡す環境変数は `arin_core::terminal::ShellLaunchSpec` が許可リストで絞る。ここでは読んだまま持つ。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TerminalEnv {
    /// ホームフォルダ。【仮定】M1 のターミナルの cwd(計画書 M1 の冒頭)。
    pub home: Option<PathBuf>,
    /// `USER` の値。既定のシェルをユーザー情報から引くのに使う。
    pub user: Option<String>,
    /// アプリが受け取った環境変数の全部。
    pub vars: Vec<(OsString, OsString)>,
}

/// 組み立てに渡す起動時の設定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchConfig {
    paths: AppPaths,
    ready_marker: Option<PathBuf>,
    terminal_marker: Option<PathBuf>,
    terminal: TerminalEnv,
    options: LaunchOptions,
}

impl LaunchConfig {
    /// 読んだ値から設定を作る。ターミナルの値はホームフォルダだけを持つ(`with_terminal_env` で差し替える)。
    ///
    /// # Errors
    ///
    /// 状態フォルダを決められないとき(`AppPaths::resolve` の失敗)。
    pub fn from_inputs(
        home: Option<&Path>,
        state_dir_override: Option<&OsStr>,
        smoke: Option<&OsStr>,
    ) -> Result<Self, AppPathsError> {
        let paths = AppPaths::resolve(home, state_dir_override)?;
        let smoke_requested = smoke.is_some_and(|v| v == "1");
        let ready_marker = paths.smoke_ready_marker(smoke_requested);
        let terminal_marker = paths.smoke_terminal_marker(smoke_requested);
        Ok(Self {
            paths,
            ready_marker,
            terminal_marker,
            terminal: TerminalEnv {
                home: home.map(Path::to_path_buf),
                ..TerminalEnv::default()
            },
            options: LaunchOptions::default(),
        })
    }

    /// ターミナルの起動に使う値を差し替える。
    #[must_use]
    pub fn with_terminal_env(mut self, terminal: TerminalEnv) -> Self {
        self.terminal = terminal;
        self
    }

    /// 画面へ渡す起動時の指定を差し替える。
    #[must_use]
    pub fn with_launch_options(mut self, options: LaunchOptions) -> Self {
        self.options = options;
        self
    }

    /// 環境変数とホームディレクトリを読んで設定を作る。
    ///
    /// 起動時の指定(`ARIN_RENDERER`・`ARIN_UNICODE`)が使えない値なら既定に戻し、標準エラーに 1 行ずつ書く。
    /// この指定は M1 の検証用で、配布用の `.app` でも比べられるようリリースビルドでも読む(M4 の T50b で外す)。
    ///
    /// # Errors
    ///
    /// 状態フォルダを決められないとき。
    pub fn from_env() -> Result<Self, AppPathsError> {
        let home = std::env::home_dir();
        let state_dir = std::env::var_os(ENV_STATE_DIR);
        let smoke = std::env::var_os(ENV_SMOKE);
        let (options, invalid) = launch_options_from(|name| std::env::var_os(name));
        for warning in invalid {
            eprintln!("{LAUNCH_WARNING_PREFIX} {warning}");
        }
        let terminal = TerminalEnv {
            home: home.clone(),
            user: std::env::var("USER").ok(),
            vars: std::env::vars_os().collect(),
        };
        Ok(
            Self::from_inputs(home.as_deref(), state_dir.as_deref(), smoke.as_deref())?
                .with_terminal_env(terminal)
                .with_launch_options(options),
        )
    }

    /// 状態の置き場所。
    #[must_use]
    pub fn paths(&self) -> &AppPaths {
        &self.paths
    }

    /// 起動スモークの印の場所。スモークが有効でなければ `None`。
    #[must_use]
    pub fn ready_marker(&self) -> Option<&Path> {
        self.ready_marker.as_deref()
    }

    /// 起動スモークのターミナルの印の場所(最初の処理済みの通知で書く)。スモークが有効でなければ `None`。
    #[must_use]
    pub fn terminal_marker(&self) -> Option<&Path> {
        self.terminal_marker.as_deref()
    }

    /// ターミナルの起動に使う値。
    #[must_use]
    pub fn terminal(&self) -> &TerminalEnv {
        &self.terminal
    }

    /// 画面へ渡す起動時の指定(`get_launch_options` が返す)。
    #[must_use]
    pub fn options(&self) -> LaunchOptions {
        self.options
    }
}

/// 環境変数の値を返す関数(`var`)から起動時の指定を決める。読む名前は `ARIN_RENDERER`・`ARIN_UNICODE` だけ。
/// `from_env` は `std::env::var_os` を渡す。テストは環境変数を書き換えずに、値を返す関数を渡す。
fn launch_options_from(
    var: impl Fn(&str) -> Option<OsString>,
) -> (LaunchOptions, Vec<InvalidLaunchOption>) {
    let renderer = var(ENV_RENDERER);
    let unicode = var(ENV_UNICODE);
    LaunchOptions::resolve(LaunchOptionInputs {
        renderer: renderer.as_deref(),
        unicode: unicode.as_deref(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: &str = "/Users/someone";

    fn config(state_dir: Option<&str>, smoke: Option<&str>) -> LaunchConfig {
        LaunchConfig::from_inputs(
            Some(Path::new(HOME)),
            state_dir.map(OsStr::new),
            smoke.map(OsStr::new),
        )
        .expect("設定を作れるはず")
    }

    #[test]
    fn 環境変数が何も無ければ既定の置き場所で印を書かない() {
        let c = config(None, None);

        assert_eq!(
            c.paths().state_dir(),
            Path::new("/Users/someone/Library/Application Support/Arin")
        );
        assert_eq!(c.ready_marker(), None);
        assert_eq!(c.terminal_marker(), None);
    }

    #[test]
    fn 差し替えとスモークの指定があれば印を差し替えた状態フォルダに置く() {
        let c = config(Some("/tmp/arin-x"), Some("1"));

        assert_eq!(c.ready_marker(), Some(Path::new("/tmp/arin-x/smoke-ready")));
        assert_eq!(
            c.terminal_marker(),
            Some(Path::new("/tmp/arin-x/smoke-terminal"))
        );
    }

    #[test]
    fn スモークの値が_1_以外なら印を書かない() {
        for value in ["", "0", "true", "yes", " 1"] {
            assert_eq!(
                config(Some("/tmp/arin-x"), Some(value)).ready_marker(),
                None,
                "ARIN_SMOKE={value:?} で印が有効になった"
            );
            assert_eq!(
                config(Some("/tmp/arin-x"), Some(value)).terminal_marker(),
                None,
                "ARIN_SMOKE={value:?} でターミナルの印が有効になった"
            );
        }
    }

    #[test]
    fn 差し替えが無ければスモークの指定があっても印を書かない() {
        assert_eq!(config(None, Some("1")).ready_marker(), None);
        assert_eq!(config(None, Some("1")).terminal_marker(), None);
    }

    #[test]
    fn ターミナルの値は既定でホームフォルダだけを持ち_差し替えられる() {
        let c = config(None, None);
        assert_eq!(
            c.terminal(),
            &TerminalEnv {
                home: Some(PathBuf::from(HOME)),
                user: None,
                vars: Vec::new(),
            }
        );

        let env = TerminalEnv {
            home: Some(PathBuf::from("/Users/other")),
            user: Some("other".to_owned()),
            vars: vec![("HOME".into(), "/Users/other".into())],
        };
        assert_eq!(c.with_terminal_env(env.clone()).terminal(), &env);
    }

    #[test]
    fn 起動時の指定は既定で_dom_と_unicode11_で_差し替えられる() {
        use arin_core::launch_options::{RendererKind, UnicodeWidth};

        let c = config(None, None);
        assert_eq!(c.options(), LaunchOptions::default());

        let options = LaunchOptions {
            renderer: RendererKind::Webgl,
            unicode: UnicodeWidth::Graphemes,
        };
        assert_eq!(c.with_launch_options(options).options(), options);
    }

    /// 名前と値の組から、環境変数の値を返す関数を作る。
    fn vars(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let pairs: Vec<(String, OsString)> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), OsString::from(v)))
            .collect();
        move |name| {
            pairs
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.clone())
        }
    }

    #[test]
    fn 起動時の指定に_arin_verify_grid_があっても通常の起動になる() {
        // 撤去した 9 枚の検証画面の指定(T49d)。読まないので、既定の指定のままで、使えない値の報告も出さない。
        for value in ["1", "0", "yes"] {
            let (options, invalid) = launch_options_from(vars(&[("ARIN_VERIFY_GRID", value)]));
            assert_eq!(
                options,
                LaunchOptions::default(),
                "ARIN_VERIFY_GRID={value:?}"
            );
            assert_eq!(invalid, Vec::new(), "ARIN_VERIFY_GRID={value:?}");
        }
        // 画面へ渡す起動時の指定にも項目が無い。
        let json = serde_json::to_value(LaunchOptions::default()).expect("JSON にできるはず");
        assert_eq!(
            json,
            serde_json::json!({ "renderer": "dom", "unicode": "unicode11" })
        );
    }

    #[test]
    fn 起動時の指定は描画方式と全角幅の方式の環境変数から決まる() {
        use arin_core::launch_options::{RendererKind, UnicodeWidth};

        let (options, invalid) = launch_options_from(vars(&[
            ("ARIN_RENDERER", "webgl"),
            ("ARIN_UNICODE", "graphemes"),
        ]));
        assert_eq!(
            options,
            LaunchOptions {
                renderer: RendererKind::Webgl,
                unicode: UnicodeWidth::Graphemes,
            }
        );
        assert!(invalid.is_empty());

        let (options, invalid) = launch_options_from(vars(&[("ARIN_RENDERER", "canvas")]));
        assert_eq!(options, LaunchOptions::default());
        assert_eq!(invalid.len(), 1);
        assert_eq!(invalid[0].name, "ARIN_RENDERER");
    }

    #[test]
    fn 差し替えが相対パスなら失敗する() {
        let err = LaunchConfig::from_inputs(
            Some(Path::new(HOME)),
            Some(OsStr::new("rel")),
            Some(OsStr::new("1")),
        )
        .unwrap_err();

        assert_eq!(err, AppPathsError::RelativeOverride(PathBuf::from("rel")));
    }
}
