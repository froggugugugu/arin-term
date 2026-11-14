//! 入出力の禁止を破る見本。

pub mod sub;

use std::{fmt, fs};

pub fn run() {
    let _ = std::process::Command::new("ls");
    let _ = fs::read("x");
    let _ = fmt::Error;
}
