//! ログインシェルの起動の仕様(`ShellLaunchSpec`)の振る舞い(設計書 §7.1「ターミナルの起動」)。
//!
//! 親の環境変数とシステムの言語設定は引数で渡す。ここでは環境変数もファイルシステムも触らない。

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use arin_core::terminal::{ShellLaunchError, ShellLaunchSpec};

const SHELL: &str = "/bin/zsh";
const CWD: &str = "/Users/someone/projects/arin";

/// 許可リストの 6 個と、引き継いではいけない値を混ぜた親の環境変数。
fn parent_env() -> Vec<(OsString, OsString)> {
    [
        ("HOME", "/Users/someone"),
        ("USER", "someone"),
        ("LOGNAME", "someone"),
        ("TMPDIR", "/var/folders/xx/T/"),
        (
            "SSH_AUTH_SOCK",
            "/private/tmp/com.apple.launchd.x/Listeners",
        ),
        ("__CF_USER_TEXT_ENCODING", "0x1F5:0x1:0xE"),
        // 以下は引き継がない
        ("PATH", "/opt/homebrew/bin:/usr/bin:/bin"),
        ("CARGO_TARGET_DIR", "/tmp/target"),
        ("NODE_ENV", "development"),
        ("TERM", "dumb"),
        ("SHELL", "/bin/bash"),
        ("LANG", "C"),
        ("TERM_PROGRAM", "vscode"),
    ]
    .into_iter()
    .map(|(k, v)| (OsString::from(k), OsString::from(v)))
    .collect()
}

fn spec() -> ShellLaunchSpec {
    ShellLaunchSpec::new(
        Path::new(SHELL),
        Path::new(CWD),
        parent_env(),
        Some("ja_JP"),
    )
    .expect("組み立てられるはず")
}

fn env_of<'a>(spec: &'a ShellLaunchSpec, name: &str) -> Option<&'a str> {
    spec.env()
        .get(OsStr::new(name))
        .map(|v| v.to_str().expect("UTF-8 のはず"))
}

fn lang_for(locale: Option<&str>) -> String {
    let spec =
        ShellLaunchSpec::new(Path::new(SHELL), Path::new(CWD), parent_env(), locale).unwrap();
    env_of(&spec, "LANG").expect("LANG は必ず入る").to_owned()
}

// --- argv[0] とプログラム ---

#[test]
fn argv0_はハイフンとシェルのファイル名になる() {
    assert_eq!(spec().argv0(), OsStr::new("-zsh"));
}

#[test]
fn argv0_は別のシェルでもファイル名から作る() {
    let spec = ShellLaunchSpec::new(
        Path::new("/opt/homebrew/bin/fish"),
        Path::new(CWD),
        parent_env(),
        None,
    )
    .unwrap();

    assert_eq!(spec.argv0(), OsStr::new("-fish"));
    assert_eq!(spec.program(), Path::new("/opt/homebrew/bin/fish"));
}

#[test]
fn 起動するプログラムはシェルのパスそのもの() {
    assert_eq!(spec().program(), Path::new(SHELL));
}

// --- cwd ---

#[test]
fn cwd_は指定どおり() {
    assert_eq!(spec().cwd(), Path::new(CWD));
}

// --- 環境変数 ---

#[test]
fn 環境変数は許可リストの名前だけになる() {
    let spec = spec();
    let names: Vec<&str> = spec.env().keys().map(|k| k.to_str().unwrap()).collect();

    assert_eq!(
        names,
        vec![
            "COLORTERM",
            "HOME",
            "LANG",
            "LOGNAME",
            "PATH",
            "SHELL",
            "SSH_AUTH_SOCK",
            "TERM",
            "TERM_PROGRAM",
            "TMPDIR",
            "USER",
            "__CF_USER_TEXT_ENCODING",
        ]
    );
}

#[test]
fn 親の余計な変数は入らない() {
    let spec = spec();

    assert_eq!(env_of(&spec, "CARGO_TARGET_DIR"), None);
    assert_eq!(env_of(&spec, "NODE_ENV"), None);
}

#[test]
fn 許可リストの変数は親の値を引き継ぐ() {
    let spec = spec();

    assert_eq!(env_of(&spec, "HOME"), Some("/Users/someone"));
    assert_eq!(env_of(&spec, "USER"), Some("someone"));
    assert_eq!(env_of(&spec, "LOGNAME"), Some("someone"));
    assert_eq!(env_of(&spec, "TMPDIR"), Some("/var/folders/xx/T/"));
    assert_eq!(
        env_of(&spec, "SSH_AUTH_SOCK"),
        Some("/private/tmp/com.apple.launchd.x/Listeners")
    );
    assert_eq!(
        env_of(&spec, "__CF_USER_TEXT_ENCODING"),
        Some("0x1F5:0x1:0xE")
    );
}

#[test]
fn 親に無い許可リストの変数は入れない() {
    let parent = vec![(OsString::from("HOME"), OsString::from("/Users/someone"))];
    let spec = ShellLaunchSpec::new(Path::new(SHELL), Path::new(CWD), parent, None).unwrap();

    assert_eq!(env_of(&spec, "HOME"), Some("/Users/someone"));
    assert_eq!(env_of(&spec, "SSH_AUTH_SOCK"), None);
    assert_eq!(env_of(&spec, "USER"), None);
}

#[test]
fn 親の値が空の変数は入れない() {
    // 環境変数の慣習(空は未設定と同じ)に合わせる。
    let parent = vec![(OsString::from("SSH_AUTH_SOCK"), OsString::new())];
    let spec = ShellLaunchSpec::new(Path::new(SHELL), Path::new(CWD), parent, None).unwrap();

    assert_eq!(env_of(&spec, "SSH_AUTH_SOCK"), None);
}

#[test]
fn 親の環境変数が無くても組み立てられる() {
    let spec = ShellLaunchSpec::new(
        Path::new(SHELL),
        Path::new(CWD),
        Vec::<(OsString, OsString)>::new(),
        None,
    )
    .unwrap();

    assert_eq!(env_of(&spec, "HOME"), None);
    assert_eq!(env_of(&spec, "TERM"), Some("xterm-256color"));
}

#[test]
fn term_は_xterm_256color() {
    assert_eq!(env_of(&spec(), "TERM"), Some("xterm-256color"));
}

#[test]
fn colorterm_は_truecolor() {
    assert_eq!(env_of(&spec(), "COLORTERM"), Some("truecolor"));
}

#[test]
fn path_は_finder_から起動したアプリと同じ最小の値() {
    // 親の PATH(開発時の tauri dev など)は引き継がない。残りはログインシェルが組み立てる。
    assert_eq!(
        env_of(&spec(), "PATH"),
        Some("/usr/bin:/bin:/usr/sbin:/sbin")
    );
}

#[test]
fn shell_は起動するシェルのパス() {
    // 親の SHELL(/bin/bash)ではなく、起動するシェル。
    assert_eq!(env_of(&spec(), "SHELL"), Some(SHELL));
}

#[test]
fn term_program_は_arin() {
    assert_eq!(env_of(&spec(), "TERM_PROGRAM"), Some("Arin"));
}

// --- LANG の組み立て ---

#[test]
fn lang_はシステムの言語設定から作り_utf8_を付ける() {
    assert_eq!(lang_for(Some("ja_JP")), "ja_JP.UTF-8");
    assert_eq!(lang_for(Some("en_US")), "en_US.UTF-8");
}

#[test]
fn lang_は親の_lang_を引き継がない() {
    // 親には LANG=C がある。
    assert_eq!(lang_for(Some("ja_JP")), "ja_JP.UTF-8");
}

#[test]
fn lang_はハイフン区切りの識別子も受け付ける() {
    assert_eq!(lang_for(Some("ja-JP")), "ja_JP.UTF-8");
}

#[test]
fn lang_は_at_以降の付加情報を捨てる() {
    // macOS は地域を言語と別に選ぶと `en_US@rg=jpzzzz` の形になる。
    assert_eq!(lang_for(Some("en_US@rg=jpzzzz")), "en_US.UTF-8");
}

#[test]
fn lang_は文字体系の部分を捨てる() {
    assert_eq!(lang_for(Some("zh-Hans_CN")), "zh_CN.UTF-8");
}

#[test]
fn lang_は3文字の言語コードも受け付ける() {
    assert_eq!(lang_for(Some("haw_US")), "haw_US.UTF-8");
}

#[test]
fn lang_は言語設定が分からなければ_en_us_utf8_にする() {
    assert_eq!(lang_for(None), "en_US.UTF-8");
    assert_eq!(lang_for(Some("")), "en_US.UTF-8");
}

#[test]
fn lang_は地域が無い識別子なら_en_us_utf8_にする() {
    // `ja.UTF-8` という名前のロケールは無いので作らない。
    assert_eq!(lang_for(Some("ja")), "en_US.UTF-8");
}

#[test]
fn lang_は形の崩れた識別子なら_en_us_utf8_にする() {
    assert_eq!(lang_for(Some("JA_jp")), "en_US.UTF-8");
    assert_eq!(lang_for(Some("ja_JP; rm -rf ~")), "en_US.UTF-8");
    assert_eq!(lang_for(Some("_JP")), "en_US.UTF-8");
    assert_eq!(lang_for(Some("japanese_JP")), "en_US.UTF-8");
    assert_eq!(lang_for(Some("ja_419")), "en_US.UTF-8");
}

// --- 境界値(エラー) ---

#[test]
fn シェルのパスが空なら失敗する() {
    let err = ShellLaunchSpec::new(Path::new(""), Path::new(CWD), parent_env(), None).unwrap_err();

    assert_eq!(err, ShellLaunchError::EmptyShell);
}

#[test]
fn シェルのパスが相対なら失敗する() {
    // 相対パスは cwd や PATH で解決先が変わるので受け付けない。
    let err =
        ShellLaunchSpec::new(Path::new("zsh"), Path::new(CWD), parent_env(), None).unwrap_err();

    assert_eq!(err, ShellLaunchError::RelativeShell(PathBuf::from("zsh")));
    assert!(err.to_string().contains("絶対パス"));
}

#[test]
fn シェルのパスにファイル名が無ければ失敗する() {
    for shell in ["/", "/bin/.."] {
        let err =
            ShellLaunchSpec::new(Path::new(shell), Path::new(CWD), parent_env(), None).unwrap_err();

        assert_eq!(
            err,
            ShellLaunchError::ShellWithoutFileName(PathBuf::from(shell))
        );
    }
}

#[test]
fn cwd_が空なら失敗する() {
    let err =
        ShellLaunchSpec::new(Path::new(SHELL), Path::new(""), parent_env(), None).unwrap_err();

    assert_eq!(err, ShellLaunchError::EmptyCwd);
}

#[test]
fn cwd_が相対なら失敗する() {
    let err = ShellLaunchSpec::new(
        Path::new(SHELL),
        Path::new("./projects"),
        parent_env(),
        None,
    )
    .unwrap_err();

    assert_eq!(
        err,
        ShellLaunchError::RelativeCwd(PathBuf::from("./projects"))
    );
    assert!(err.to_string().contains("絶対パス"));
}

#[test]
fn エラーの文言に原因の値が載る() {
    let empty = ShellLaunchError::EmptyShell.to_string();
    let no_name = ShellLaunchError::ShellWithoutFileName(PathBuf::from("/")).to_string();
    let empty_cwd = ShellLaunchError::EmptyCwd.to_string();

    assert!(empty.contains("シェル"));
    assert!(no_name.contains('/'));
    assert!(empty_cwd.contains("作業フォルダ"));
}
