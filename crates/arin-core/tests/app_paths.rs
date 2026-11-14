//! 置き場所の決定(`AppPaths`)の振る舞い。
//!
//! 入力(ホームディレクトリと差し替えの指定)は引数で渡す。ここでは環境変数もファイルシステムも触らない。

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use arin_core::paths::{
    AppPaths, AppPathsError, SMOKE_READY_MARKER_FILE_NAME, SMOKE_TERMINAL_MARKER_FILE_NAME,
};

const HOME: &str = "/Users/someone";

fn home() -> Option<&'static Path> {
    Some(Path::new(HOME))
}

#[test]
fn 差し替えの指定が無ければホームの下の_application_support_arin_になる() {
    let paths = AppPaths::resolve(home(), None).expect("決められるはず");

    assert_eq!(
        paths.state_dir(),
        Path::new("/Users/someone/Library/Application Support/Arin")
    );
    assert!(!paths.is_overridden());
}

#[test]
fn 差し替えの指定があればそれを使う() {
    let paths =
        AppPaths::resolve(home(), Some(OsStr::new("/tmp/arin-smoke-1"))).expect("決められるはず");

    assert_eq!(paths.state_dir(), Path::new("/tmp/arin-smoke-1"));
    assert!(paths.is_overridden());
}

#[test]
fn 差し替えの指定があればホームが分からなくても決められる() {
    let paths = AppPaths::resolve(None, Some(OsStr::new("/tmp/arin-smoke-2")))
        .expect("ホームを使わないので決められるはず");

    assert_eq!(paths.state_dir(), Path::new("/tmp/arin-smoke-2"));
}

#[test]
fn 差し替えの指定が空文字列なら指定が無いものとして扱う() {
    // 環境変数の慣習(空は未設定と同じ)に合わせる。
    let paths = AppPaths::resolve(home(), Some(OsStr::new(""))).expect("決められるはず");

    assert_eq!(
        paths.state_dir(),
        Path::new("/Users/someone/Library/Application Support/Arin")
    );
    assert!(!paths.is_overridden());
}

#[test]
fn 差し替えの指定が相対パスなら失敗する() {
    // GUI のアプリは起動元によって作業ディレクトリが変わるので、相対パスは受け付けない。
    let err = AppPaths::resolve(home(), Some(OsStr::new("state/arin"))).unwrap_err();

    assert_eq!(
        err,
        AppPathsError::RelativeOverride(PathBuf::from("state/arin"))
    );
    assert!(err.to_string().contains("絶対パス"));
}

#[test]
fn ドットで始まる相対パスも失敗する() {
    let err = AppPaths::resolve(home(), Some(OsStr::new("./arin"))).unwrap_err();

    assert_eq!(
        err,
        AppPathsError::RelativeOverride(PathBuf::from("./arin"))
    );
}

#[test]
fn 差し替えが無くホームも分からなければ失敗する() {
    let err = AppPaths::resolve(None, None).unwrap_err();

    assert_eq!(err, AppPathsError::HomeUnknown);
}

#[test]
fn ホームが空なら分からないものとして失敗する() {
    let err = AppPaths::resolve(Some(Path::new("")), None).unwrap_err();

    assert_eq!(err, AppPathsError::HomeUnknown);
}

#[test]
fn ホームが相対パスなら失敗する() {
    let err = AppPaths::resolve(Some(Path::new("someone")), None).unwrap_err();

    assert_eq!(err, AppPathsError::RelativeHome(PathBuf::from("someone")));
}

#[test]
fn スモークの印は差し替えた状態フォルダの中に置く() {
    let paths = AppPaths::resolve(home(), Some(OsStr::new("/tmp/arin-smoke-3"))).unwrap();

    assert_eq!(
        paths.smoke_ready_marker(true),
        Some(PathBuf::from("/tmp/arin-smoke-3").join(SMOKE_READY_MARKER_FILE_NAME))
    );
}

#[test]
fn スモークを求めていなければ印を置かない() {
    let paths = AppPaths::resolve(home(), Some(OsStr::new("/tmp/arin-smoke-4"))).unwrap();

    assert_eq!(paths.smoke_ready_marker(false), None);
}

#[test]
fn 状態フォルダを差し替えていなければスモークを求めても印を置かない() {
    // 本物の ~/Library/Application Support/Arin/ に印を書かないため。
    let paths = AppPaths::resolve(home(), None).unwrap();

    assert_eq!(paths.smoke_ready_marker(true), None);
}

#[test]
fn 印のファイル名は_smoke_ready() {
    // scripts/smoke.mjs がこの名前を待つ。変えるときは両方を合わせる。
    assert_eq!(SMOKE_READY_MARKER_FILE_NAME, "smoke-ready");
}

#[test]
fn ターミナルの印は差し替えた状態フォルダの中に置く() {
    let paths = AppPaths::resolve(home(), Some(OsStr::new("/tmp/arin-smoke-5"))).unwrap();

    assert_eq!(
        paths.smoke_terminal_marker(true),
        Some(PathBuf::from("/tmp/arin-smoke-5").join(SMOKE_TERMINAL_MARKER_FILE_NAME))
    );
    assert_eq!(paths.smoke_terminal_marker(false), None);
}

#[test]
fn 状態フォルダを差し替えていなければターミナルの印も置かない() {
    let paths = AppPaths::resolve(home(), None).unwrap();

    assert_eq!(paths.smoke_terminal_marker(true), None);
}

#[test]
fn ターミナルの印のファイル名は_smoke_terminal_で準備完了の印と違う() {
    // scripts/smoke.mjs がこの名前を待つ。変えるときは両方を合わせる。
    assert_eq!(SMOKE_TERMINAL_MARKER_FILE_NAME, "smoke-terminal");
    assert_ne!(
        SMOKE_TERMINAL_MARKER_FILE_NAME,
        SMOKE_READY_MARKER_FILE_NAME
    );
}
