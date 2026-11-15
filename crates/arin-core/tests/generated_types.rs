//! Rust の型から生成した TypeScript の型(`src/ipc/generated/`)が最新であることを確かめる。
//!
//! 生成物はコミットする。このテストは、いまの Rust の型から作り直した内容と、
//! コミットされているファイルが 1 文字も違わないことを見る(古い生成物のコミットを防ぐ)。
//!
//! 再生成するときは、環境変数 `ARIN_UPDATE_GENERATED=1` を付けて実行する:
//!
//! ```sh
//! ARIN_UPDATE_GENERATED=1 cargo test -p arin-core --test generated_types
//! ```
//!
//! ファイルへの書き出しはこのテストだけが行う。`arin-core` の製品コードは入出力をしない。

use std::any::TypeId;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use arin_core::fileops::{ConflictResolution, FileOpOutcome, NewEntryKind};
use arin_core::layout::tiles::{DisplayMode, TileGrid};
use arin_core::paths::{PathKind, RelPath};
use arin_core::project::ProjectColor;
use arin_core::shortcuts::table::ShortcutTable;
use arin_core::tree::{DirListing, TreeChange};
use arin_core::workspace::{
    AddProjectOutcome, AppSnapshot, ConnectOutcome, DisplaySnapshot, MenuCommand, QuitRequest,
    RemoveProjectOutcome,
};
use ts_rs::{Config, TS, TypeVisitor};

/// 画面とやり取りする型の入口。ここから参照している型は、たどって全部生成する。
fn collect_generated(cfg: &Config) -> BTreeMap<PathBuf, String> {
    let mut collector = Collector {
        cfg,
        seen: HashSet::new(),
        files: BTreeMap::new(),
    };
    collector.visit::<AppSnapshot>();
    collector.visit::<ConnectOutcome>();
    collector.visit::<AddProjectOutcome>();
    collector.visit::<RemoveProjectOutcome>();
    collector.visit::<ProjectColor>();
    collector.visit::<MenuCommand>();
    collector.visit::<QuitRequest>();
    // M3 のツリーとファイル操作(計画書 T38a)。
    collector.visit::<RelPath>();
    collector.visit::<PathKind>();
    collector.visit::<DirListing>();
    collector.visit::<TreeChange>();
    collector.visit::<FileOpOutcome>();
    collector.visit::<ConflictResolution>();
    collector.visit::<NewEntryKind>();
    // ショートカットの表(計画書 T38d)。行・コマンドの ID・受け口もたどって生成する。
    collector.visit::<ShortcutTable>();
    // タイル表示の表示モードとグリッド(計画書 T46a)。
    collector.visit::<DisplayMode>();
    collector.visit::<TileGrid>();
    // 写しの表示の状態(計画書 T46b。`AppSnapshot` からもたどれる)。
    collector.visit::<DisplaySnapshot>();
    collector.files
}

/// 型とその依存をたどり、出力先の相対パスと中身を集める。
struct Collector<'a> {
    cfg: &'a Config,
    seen: HashSet<TypeId>,
    files: BTreeMap<PathBuf, String>,
}

impl TypeVisitor for Collector<'_> {
    fn visit<T: TS + 'static + ?Sized>(&mut self) {
        // 数値や文字列などの組み込みの型は出力先を持たないので、ファイルにしない。
        let Some(path) = T::output_path() else {
            return;
        };
        if !self.seen.insert(TypeId::of::<T>()) {
            return;
        }
        let text = T::export_to_string(self.cfg).unwrap_or_else(|e| {
            panic!("{} の型を生成できなかった: {e}", std::any::type_name::<T>())
        });
        self.files.insert(path, text);
        T::visit_dependencies(self);
    }
}

/// 既定のショートカットの表の JSON の置き場所(生成先からの相対パス。計画書 T38d)。
///
/// 画面の自動テストの偽物と Vitest が、Rust と同じ既定の表を使うために置く。
const DEFAULT_SHORTCUTS_JSON: &str = "defaultShortcuts.json";

fn generated_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src/ipc/generated")
}

/// 生成先にいまある `.ts` のファイル(生成先からの相対パス)。
fn existing_ts_files(dir: &Path) -> BTreeSet<PathBuf> {
    let mut found = BTreeSet::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let entries = fs::read_dir(&current)
            .unwrap_or_else(|e| panic!("{} を読めなかった: {e}", current.display()));
        for entry in entries {
            let path = entry.expect("フォルダの項目を読めなかった").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "ts") {
                let rel = path
                    .strip_prefix(dir)
                    .expect("生成先の外のファイルが見つかった");
                found.insert(rel.to_path_buf());
            }
        }
    }
    found
}

#[test]
fn 生成した_type_script_の型が最新である() {
    let cfg = Config::new();
    let expected = collect_generated(&cfg);
    assert!(!expected.is_empty(), "生成する型が 1 つも無い");

    let dir = generated_dir();
    let update = std::env::var_os("ARIN_UPDATE_GENERATED").is_some_and(|v| v == "1");

    if update {
        for (rel, text) in &expected {
            let path = dir.join(rel);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .unwrap_or_else(|e| panic!("{} を作れなかった: {e}", parent.display()));
            }
            fs::write(&path, text)
                .unwrap_or_else(|e| panic!("{} に書けなかった: {e}", path.display()));
        }
    }

    let hint = "再生成: ARIN_UPDATE_GENERATED=1 cargo test -p arin-core --test generated_types";

    for (rel, text) in &expected {
        let path = dir.join(rel);
        let actual = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("生成物 {} が無い({e})。{hint}", path.display()));
        assert_eq!(
            &actual,
            text,
            "生成物 {} が Rust の型と合っていない。{hint}",
            path.display()
        );
    }

    // Rust の型を消したり名前を変えたりしたときに、古いファイルが残っていないこと。
    // 消すのは人の判断に任せる(このテストはファイルを消さない)。
    let expected_paths: BTreeSet<PathBuf> = expected.keys().cloned().collect();
    let stale: Vec<_> = existing_ts_files(&dir)
        .difference(&expected_paths)
        .map(|p| p.display().to_string())
        .collect();
    assert!(
        stale.is_empty(),
        "生成先に、どの Rust の型からも作られないファイルがある: {stale:?}。不要なら消す"
    );
}

#[test]
fn 既定のショートカットの表の_json_が最新である() {
    let expected = ShortcutTable::default_table().to_json();
    let path = generated_dir().join(DEFAULT_SHORTCUTS_JSON);
    let update = std::env::var_os("ARIN_UPDATE_GENERATED").is_some_and(|v| v == "1");

    if update {
        fs::write(&path, &expected)
            .unwrap_or_else(|e| panic!("{} に書けなかった: {e}", path.display()));
    }

    let hint = "再生成: ARIN_UPDATE_GENERATED=1 cargo test -p arin-core --test generated_types";
    let actual = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("生成物 {} が無い({e})。{hint}", path.display()));
    assert_eq!(
        actual,
        expected,
        "生成物 {} が Rust の既定の表と合っていない。{hint}",
        path.display()
    );
}
