//! 性能の計測: ツリーのフォルダの読み込み(展開)の時間(計画書 T44a、NFR-005)。記録するだけで合否にしない。
//!
//! 一時フォルダに見本を 2 つ作り、フォルダを展開したときに Rust 側でかかる時間を測る。
//! - `flat`: 1 つのフォルダに 10 万ファイル
//! - `nested`: 1,000 フォルダ × 100 ファイル。ルート(1,000 フォルダ)と、その中の 1 フォルダ(100 ファイル)を測る
//!
//! 測るのは `src-tauri/src/tree.rs` の `list_directory` と同じ手順: 範囲の検査(`PathScopeGuard::folder`)と
//! 読み込み(`LocalFileSystem::read_dir`)、除外(`ExclusionRule` とコピーの作業用フォルダ)、並べ替え(`sort_entries`)。
//! 画面側(写しの受け渡し・描画)の時間は含めない(T44b)。
//!
//! 普段の `cargo test` では動かさない(`#[ignore]`)。`npm run perf`(`scripts/perf/run.mjs`)が
//! `--release -- --ignored --nocapture` で動かし、`ARIN_PERF ` で始まる行(1 行 1 つの JSON)を読み取る。
//! ファイルの数は `ARIN_PERF_TREE_FILES`(既定 100000。`npm run perf -- --tree-files=<数>` が渡す)で変えられる。
//! 見本は一時フォルダに作り、抜けるときに消す。

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use arin_core::fileops::scope::PathScopeGuard;
use arin_core::paths::RelPath;
use arin_core::ports::fs::{DirEntry, FileSystemPort};
use arin_core::tree::exclusion::ExclusionRule;
use arin_core::tree::order::sort_entries;
use arin_platform::fs::{LocalFileSystem, STAGING_PREFIX};

/// 既定のファイルの数(NFR-005 の「10 万ファイル」)。
const DEFAULT_FILES: usize = 100_000;
/// `nested` の 1 フォルダの中のファイルの数。
const FILES_PER_DIR: usize = 100;
/// 計測の回数。中央値と最小・最大は `scripts/perf/run.mjs` が出す。
const RUNS: usize = 3;
/// ファイルの数を変える環境変数(計測の引数。製品のコードは読まない)。
const FILES_ENV: &str = "ARIN_PERF_TREE_FILES";

/// 計測だけに使う一時フォルダ。抜けるときに自分が作ったフォルダだけを消す。
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("時計が 1970 年より前")
            .as_nanos();
        let base = fs::canonicalize(std::env::temp_dir()).expect("一時領域の実パスを取れなかった");
        let dir = base.join(format!("arin-perf-{name}-{}-{nanos}", std::process::id()));
        assert!(
            dir.starts_with(&base) && dir != base,
            "一時領域の外を使おうとした"
        );
        fs::create_dir(&dir).expect("一時フォルダを作れなかった");
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let started = Instant::now();
        let _ = fs::remove_dir_all(&self.0);
        eprintln!(
            "見本を片付けた({:.1} 秒){}",
            started.elapsed().as_secs_f64(),
            if self.0.exists() {
                "。消し残りがある"
            } else {
                ""
            }
        );
    }
}

/// 計測するファイルの数。不正な値は既定に戻す。
fn file_count() -> usize {
    match std::env::var(FILES_ENV) {
        Ok(text) => match text.parse::<usize>() {
            Ok(n) if n >= FILES_PER_DIR => n,
            _ => {
                eprintln!("{FILES_ENV}={text:?} は使えないので既定の {DEFAULT_FILES} にする");
                DEFAULT_FILES
            }
        },
        Err(_) => DEFAULT_FILES,
    }
}

/// 空のファイルを作る。
fn touch(path: &Path) {
    File::create(path).unwrap_or_else(|e| panic!("{} を作れなかった: {e}", path.display()));
}

/// 見本の名前(大文字・小文字と拡張子を混ぜ、並べ替えに手間がかかるようにする)。
fn file_name(i: usize) -> String {
    const EXT: [&str; 5] = ["ts", "tsx", "md", "json", "rs"];
    let head = if i.is_multiple_of(3) { "File" } else { "file" };
    format!("{head}-{i:06}.{}", EXT[i % EXT.len()])
}

/// 既定で隠す名前を足す(除外の手順も通るように)。
fn add_hidden(dir: &Path) {
    fs::create_dir(dir.join(".git")).expect(".git を作れなかった");
    fs::create_dir(dir.join("node_modules")).expect("node_modules を作れなかった");
    touch(&dir.join(".DS_Store"));
}

/// 1 つのフォルダに `files` 個のファイル。
fn build_flat(dir: &Path, files: usize) {
    fs::create_dir(dir).expect("見本のフォルダを作れなかった");
    add_hidden(dir);
    for i in 0..files {
        touch(&dir.join(file_name(i)));
    }
}

/// `files / FILES_PER_DIR` 個のフォルダに、それぞれ `FILES_PER_DIR` 個のファイル。
fn build_nested(dir: &Path, files: usize) -> usize {
    fs::create_dir(dir).expect("見本のフォルダを作れなかった");
    add_hidden(dir);
    let dirs = files / FILES_PER_DIR;
    for d in 0..dirs {
        let sub = dir.join(format!("Dir-{d:04}"));
        fs::create_dir(&sub).expect("見本のフォルダを作れなかった");
        for i in 0..FILES_PER_DIR {
            touch(&sub.join(file_name(i)));
        }
    }
    dirs
}

/// 1 回分の計測の結果。
struct Timing {
    read_ms: f64,
    filter_ms: f64,
    sort_ms: f64,
    raw_entries: usize,
    listed: usize,
}

/// `list_directory` と同じ手順で 1 回読む。
fn list_once(fs_port: &dyn FileSystemPort, root: &Path, rel: &RelPath) -> Timing {
    let rule = ExclusionRule::new(false);
    let t0 = Instant::now();
    let folder = PathScopeGuard::new(fs_port, root)
        .folder(rel)
        .expect("範囲の検査を通らなかった");
    let entries = fs_port
        .read_dir(&folder.real)
        .expect("フォルダを読めなかった");
    let t1 = Instant::now();
    let raw_entries = entries.len();
    let mut entries: Vec<DirEntry> = rule.filter(entries);
    entries.retain(|entry| !entry.name.starts_with(STAGING_PREFIX));
    let t2 = Instant::now();
    sort_entries(&mut entries);
    let t3 = Instant::now();
    let ms = |a: Instant, b: Instant| b.duration_since(a).as_secs_f64() * 1000.0;
    Timing {
        read_ms: ms(t0, t1),
        filter_ms: ms(t1, t2),
        sort_ms: ms(t2, t3),
        raw_entries,
        listed: entries.len(),
    }
}

/// `fixture` の `rel` を `RUNS` 回読み、1 回ごとに `ARIN_PERF` の行を出す。
fn measure(root: &Path, fixture: &str, rel: &str, fixture_files: usize, create_s: f64) {
    let fs_port = LocalFileSystem::new();
    let rel = RelPath::parse(rel).expect("正しい相対パス");
    for run in 1..=RUNS {
        let t = list_once(&fs_port, root, &rel);
        println!(
            "ARIN_PERF {{\"kind\":\"tree_list\",\"fixture\":\"{fixture}\",\"run\":{run},\
             \"fixture_files\":{fixture_files},\"create_s\":{create_s:.3},\"raw_entries\":{},\"listed\":{},\
             \"read_ms\":{:.3},\"filter_ms\":{:.3},\"sort_ms\":{:.3},\"total_ms\":{:.3}}}",
            t.raw_entries,
            t.listed,
            t.read_ms,
            t.filter_ms,
            t.sort_ms,
            t.read_ms + t.filter_ms + t.sort_ms,
        );
    }
}

#[test]
#[ignore = "性能の計測。npm run perf が --ignored で動かす"]
fn 十万ファイルの見本でフォルダの読み込みの時間を記録する() {
    let files = file_count();
    let dir = TempDir::new("tree");
    let root = dir.path();

    let started = Instant::now();
    build_flat(&root.join("flat"), files);
    let flat_s = started.elapsed().as_secs_f64();
    eprintln!("見本 flat({files} ファイル)を {flat_s:.1} 秒で作った");
    measure(root, "flat", "flat", files, flat_s);

    let started = Instant::now();
    let dirs = build_nested(&root.join("nested"), files);
    let nested_s = started.elapsed().as_secs_f64();
    eprintln!("見本 nested({dirs} フォルダ × {FILES_PER_DIR} ファイル)を {nested_s:.1} 秒で作った");
    measure(root, "nested_root", "nested", files, nested_s);
    let middle = format!("nested/Dir-{:04}", dirs / 2);
    measure(root, "nested_child", &middle, files, nested_s);
}
