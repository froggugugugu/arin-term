//! `src-tauri/tests/` の共通の補助。
//!
//! テストは状態フォルダをシステムの一時領域の下に差し替え、本物の `~/Library/Application Support/Arin/` に触れない。
//! 環境変数は読まず、組み立て(`LaunchConfig::from_inputs`)に値を引数で渡す。

#![allow(dead_code)] // テストごとに使う補助が違う。

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use arin_core::launch_options::LaunchOptions;
use arin_lib::launch::LaunchConfig;
use tauri::AppHandle;
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::MockRuntime;
use tauri::webview::InvokeRequest;

/// 画面(自アプリの画面)からコマンドを呼んだときの要求を作る。
pub fn request(handle: &AppHandle<MockRuntime>, cmd: &str) -> InvokeRequest {
    request_with(handle, cmd, InvokeBody::default())
}

/// 引数 `body`(JSON。キーは camelCase)を付けてコマンドを呼んだときの要求を作る。
pub fn request_with(handle: &AppHandle<MockRuntime>, cmd: &str, body: InvokeBody) -> InvokeRequest {
    InvokeRequest {
        cmd: cmd.into(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        url: "tauri://localhost".parse().expect("URL が不正"),
        body,
        headers: Default::default(),
        invoke_key: handle.invoke_key().to_string(),
    }
}

/// このテストだけが使う一時フォルダ。抜けるときに自分が作ったフォルダだけを消す。
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(name: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("時計が 1970 年より前")
            .as_nanos();
        let base = std::env::temp_dir();
        let dir = base.join(format!(
            "arin-tauri-test-{name}-{}-{nanos}",
            std::process::id()
        ));
        assert!(dir.starts_with(&base), "一時領域の外を使おうとした");
        fs::create_dir(&dir).expect("一時フォルダを作れなかった");
        Self(dir)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// フォルダの中の名前の一覧(並べ替え済み)。
    pub fn entries(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(&self.0)
            .expect("一時フォルダを読めなかった")
            .map(|e| {
                e.expect("項目を読めなかった")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// 状態フォルダを `dir` に差し替えた組み立ての設定。`smoke` が真ならスモークの印を有効にする。
pub fn launch_in(dir: &TempDir, smoke: bool) -> LaunchConfig {
    // ホームは渡さない。差し替えがあるのでホームは使われない(本物のホームを指す余地を残さない)。
    LaunchConfig::from_inputs(
        None,
        Some(dir.path().as_os_str()),
        smoke.then(|| OsStr::new("1")),
    )
    .unwrap_or_else(|e| panic!("組み立ての設定を作れなかった: {e}"))
}

/// `launch_in` に 9 枚の検証画面の指定(`ARIN_VERIFY_GRID=1` と同じ)を足した設定。
///
/// 通常の起動はプロジェクト 0 件・ターミナル 0 枚で始まる(状態ファイルの読み込みは T33a)ので、
/// 起動直後からターミナルが要るテストはこちらを使い、1 枚目(`terminal-1`)を操作する。
pub fn launch_verify_grid_in(dir: &TempDir, smoke: bool) -> LaunchConfig {
    launch_in(dir, smoke).with_launch_options(LaunchOptions {
        verify_grid: true,
        ..LaunchOptions::default()
    })
}
