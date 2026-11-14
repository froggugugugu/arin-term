//! ユーザー情報とシステムの設定から、ターミナルの起動に要る値を引く(設計書 §7.1「ターミナルの起動」の 1)。
//!
//! どちらも macOS 標準のコマンドを絶対パスで起動して読む(`unsafe` と新しい依存を増やさないため)。
//! 環境変数は読まない(読むのは `src-tauri/src/launch.rs` だけ)。ユーザー名は引数で受け取る。

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const DSCL: &str = "/usr/bin/dscl";
const DEFAULTS: &str = "/usr/bin/defaults";

/// ユーザー情報から既定のシェルを引けないときに使うシェル(macOS の既定)。
pub const FALLBACK_SHELL: &str = "/bin/zsh";

/// ユーザー情報(ローカルのディレクトリサービスの `UserShell`)に登録された既定のシェルを返す。
///
/// 引けないとき(ユーザーが無い、名前に `/` を含む、登録されたシェルが絶対パスでないかファイルが無い)は
/// [`FALLBACK_SHELL`] を返す。Finder から起動したアプリには `SHELL` が渡らないことがあるので、環境変数は見ない。
#[must_use]
pub fn default_shell(user_name: &str) -> PathBuf {
    lookup_user_shell(user_name).unwrap_or_else(|| PathBuf::from(FALLBACK_SHELL))
}

/// システムの言語設定の識別子(例 `ja_JP`)を返す。`ShellLaunchSpec::new` の `system_locale` に渡す。
///
/// 設定が無いとき、読めないときは `None`(`LANG` は `ShellLaunchSpec` が代わりの値にする)。
#[must_use]
pub fn system_locale() -> Option<String> {
    let stdout = run(DEFAULTS, &["read", "-g", "AppleLocale"])?;
    parse_locale(&stdout)
}

fn lookup_user_shell(user_name: &str) -> Option<PathBuf> {
    if !is_plain_user_name(user_name) {
        return None;
    }
    let record = format!("/Users/{user_name}");
    let stdout = run(DSCL, &[".", "-read", &record, "UserShell"])?;
    parse_user_shell(&stdout).filter(|shell| shell.is_file())
}

/// ユーザー名として受け付けるか。レコードのパスを別の場所へ向けられないよう、`/` と `.`・`..` を断る。
fn is_plain_user_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\0'])
}

/// コマンドを起動し、成功したら標準出力を返す。
fn run(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

/// `dscl` の出力(`UserShell: /bin/zsh`。値に空白があると次の行に字下げして出る)から値を取り出す。
fn parse_user_shell(stdout: &str) -> Option<PathBuf> {
    let value = stdout.trim().strip_prefix("UserShell:")?.trim();
    if value.is_empty() || value.contains('\n') {
        return None;
    }
    let shell = Path::new(value);
    shell.is_absolute().then(|| shell.to_path_buf())
}

/// `defaults` の出力(`ja_JP` と改行)から、空白を含まない 1 語だけを受け付ける。
fn parse_locale(stdout: &str) -> Option<String> {
    let value = stdout.trim();
    (!value.is_empty() && !value.contains(char::is_whitespace)).then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dscl_の出力からシェルを取り出す() {
        assert_eq!(
            parse_user_shell("UserShell: /bin/zsh\n"),
            Some(PathBuf::from("/bin/zsh"))
        );
        assert_eq!(
            parse_user_shell("UserShell:\n /opt/my shell/bin/fish\n"),
            Some(PathBuf::from("/opt/my shell/bin/fish"))
        );
    }

    #[test]
    fn 想定外の_dscl_の出力は受け付けない() {
        assert_eq!(parse_user_shell(""), None);
        assert_eq!(parse_user_shell("UserShell:\n"), None);
        assert_eq!(parse_user_shell("UserShell: zsh\n"), None);
        assert_eq!(parse_user_shell("RealName: zsh\n"), None);
        assert_eq!(parse_user_shell("UserShell: /bin/zsh\n /bin/bash\n"), None);
    }

    #[test]
    fn ユーザー名は_スラッシュ_や_ドット_を含まないものだけ受け付ける() {
        assert!(is_plain_user_name("alice"));
        assert!(is_plain_user_name("_mbsetupuser"));
        assert!(!is_plain_user_name(""));
        assert!(!is_plain_user_name("."));
        assert!(!is_plain_user_name(".."));
        assert!(!is_plain_user_name("../root"));
        assert!(!is_plain_user_name("a\0b"));
    }

    #[test]
    fn defaults_の出力から言語設定の識別子を取り出す() {
        assert_eq!(parse_locale("ja_JP\n"), Some("ja_JP".to_owned()));
        assert_eq!(
            parse_locale("en_US@rg=jpzzzz\n"),
            Some("en_US@rg=jpzzzz".to_owned())
        );
        assert_eq!(parse_locale("\n"), None);
        assert_eq!(parse_locale("ja JP\n"), None);
    }
}
