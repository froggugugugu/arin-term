//! 起動スモークの印のファイルを書く(設計書 §10.1「実アプリのスモーク」)。
//!
//! 印をどこに置くか、そもそも置くかどうかは `arin_core::paths::AppPaths::smoke_ready_marker` が決める。
//! ここは渡された場所に書くだけ。

use std::fs;
use std::io;
use std::path::Path;

/// 印のファイルを書く。
///
/// 待つ側(`scripts/smoke.mjs`)が書きかけの中身を読まないよう、隣に一時の名前で書いてから名前を変える。
/// 置き場所のフォルダは作らない(無ければ失敗する)。差し替えた状態フォルダはスモークの側が用意する。
///
/// # Errors
///
/// フォルダが無い、書き込めないなど、ファイルの作成・名前の変更に失敗したとき。
pub fn write_ready_marker(path: &Path, contents: &str) -> io::Result<()> {
    let mut temp_name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "印のファイル名が無い"))?
        .to_os_string();
    temp_name.push(".tmp");
    let temp = path.with_file_name(temp_name);

    fs::write(&temp, contents)?;
    fs::rename(&temp, path).inspect_err(|_| {
        let _ = fs::remove_file(&temp);
    })
}
