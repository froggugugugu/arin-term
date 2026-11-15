//! ツリーの規則: 除外・並び順・監視の変更のまとめ(`arin-core/tree`。計画書 T38c)。
//!
//! FR-013・FR-014・FR-017、C-07、PRD C7、設計書 §5.1 の `ExclusionRule`・`ChangeBatch`、§7.1「外部でのファイル変更」、
//! §15.2 #1・#8、計画書 M3 の【仮定】2・3・19。

use std::path::{Path, PathBuf};

use arin_core::paths::RelPath;
use arin_core::ports::watch::WatchBatch;
use arin_core::project::ProjectId;
use arin_core::tree::changes::{ChangeBatch, MAX_CHANGED_DIRS, summarize_changes};
use arin_core::tree::exclusion::{DEFAULT_HIDDEN_NAMES, ExclusionRule};
use arin_core::tree::order::{compare_entries, is_expandable, is_folder_like, sort_entries};
use arin_core::tree::{DirEntry, EntryKind, TreeChange};

const ROOT: &str = "/Users/x/dev/app";

fn entry(name: &str, kind: EntryKind) -> DirEntry {
    DirEntry {
        name: name.to_owned(),
        kind,
    }
}

fn names(entries: &[DirEntry]) -> Vec<&str> {
    entries.iter().map(|e| e.name.as_str()).collect()
}

fn rel(text: &str) -> RelPath {
    RelPath::parse(text).unwrap_or_else(|e| panic!("{text:?} を読めなかった: {e}"))
}

fn batch(paths: &[&str]) -> WatchBatch {
    WatchBatch {
        paths: paths.iter().map(PathBuf::from).collect(),
        rescan: false,
    }
}

fn dirs(texts: &[&str]) -> Vec<RelPath> {
    texts.iter().map(|t| rel(t)).collect()
}

fn summarize(paths: &[&str], show_hidden: bool) -> ChangeBatch {
    summarize_changes(
        Path::new(ROOT),
        &batch(paths),
        &ExclusionRule::new(show_hidden),
    )
}

// ---- 除外(C-07、FR-017、§15.2 #1 の A) ----

#[test]
fn 既定では_git_node_modules_ds_store_を隠す() {
    let rule = ExclusionRule::default();

    assert!(!rule.show_hidden());
    assert_eq!(DEFAULT_HIDDEN_NAMES, [".git", "node_modules", ".DS_Store"]);
    for name in [".git", "node_modules", ".DS_Store"] {
        assert!(rule.is_hidden_name(name), "{name} は隠す");
    }

    let listed = rule.filter(vec![
        entry(".git", EntryKind::Directory),
        entry("node_modules", EntryKind::Directory),
        entry(".DS_Store", EntryKind::File),
        entry("src", EntryKind::Directory),
        entry("README.md", EntryKind::File),
    ]);
    assert_eq!(names(&listed), ["src", "README.md"]);
}

#[test]
fn ほかのドットファイルは既定でも出す() {
    let rule = ExclusionRule::default();

    for name in [".claude", ".github", ".env", ".gitignore"] {
        assert!(!rule.is_hidden_name(name), "{name} は出す");
    }
    let listed = rule.filter(vec![
        entry(".claude", EntryKind::Directory),
        entry(".github", EntryKind::Directory),
        entry(".gitignore", EntryKind::File),
    ]);
    assert_eq!(names(&listed), [".claude", ".github", ".gitignore"]);
}

#[test]
fn 隠しファイル表示をオンにすると_3_つも出す() {
    let rule = ExclusionRule::new(true);

    assert!(rule.show_hidden());
    for name in DEFAULT_HIDDEN_NAMES {
        assert!(!rule.is_hidden_name(name), "{name} も出す");
    }
    assert!(!rule.is_hidden_path(&rel("node_modules/react/index.js")));
    let listed = rule.filter(vec![
        entry(".git", EntryKind::Directory),
        entry("node_modules", EntryKind::Directory),
        entry(".DS_Store", EntryKind::File),
    ]);
    assert_eq!(names(&listed), [".git", "node_modules", ".DS_Store"]);
}

#[test]
fn 入れ子の階層でも名前が一致すれば隠す() {
    let rule = ExclusionRule::default();

    assert!(rule.is_hidden_path(&rel("packages/web/node_modules")));
    assert!(rule.is_hidden_path(&rel("packages/web/node_modules/react/index.js")));
    assert!(rule.is_hidden_path(&rel("vendor/lib/.git/HEAD")));
    assert!(rule.is_hidden_path(&rel("docs/img/.DS_Store")));
    assert!(!rule.is_hidden_path(&rel("packages/web/src/index.ts")));
    assert!(!rule.is_hidden_path(&RelPath::root()));
}

#[test]
fn 名前の一部が一致するだけなら隠さない() {
    let rule = ExclusionRule::default();

    for name in [
        "node_modules_backup",
        "my_node_modules",
        ".git2",
        ".gitkeep",
        ".DS_Store.bak",
        "Node_Modules",
        ".ds_store",
    ] {
        assert!(!rule.is_hidden_name(name), "{name} は出す");
    }
    assert!(!rule.is_hidden_path(&rel("node_modules_backup/a.js")));
}

// ---- 並び順(【仮定】2) ----

#[test]
fn フォルダが先でその中は名前の順() {
    let mut entries = vec![
        entry("zeta.md", EntryKind::File),
        entry("Beta", EntryKind::Directory),
        entry("alpha.ts", EntryKind::File),
        entry("src", EntryKind::Directory),
        entry("Cargo.toml", EntryKind::File),
        entry("app", EntryKind::Directory),
        entry("broken", EntryKind::Other),
    ];

    sort_entries(&mut entries);

    assert_eq!(
        names(&entries),
        [
            "app",
            "Beta",
            "src",
            "alpha.ts",
            "broken",
            "Cargo.toml",
            "zeta.md"
        ]
    );
}

#[test]
fn 数字は自然順ではなく文字の順() {
    let mut entries = vec![
        entry("file10.txt", EntryKind::File),
        entry("file2.txt", EntryKind::File),
        entry("file1.txt", EntryKind::File),
    ];

    sort_entries(&mut entries);

    assert_eq!(names(&entries), ["file1.txt", "file10.txt", "file2.txt"]);
}

#[test]
fn 大文字小文字が違うだけの名前はバイトの順で決まる() {
    let mut entries = vec![
        entry("readme.md", EntryKind::File),
        entry("README.md", EntryKind::File),
        entry("ReadMe.md", EntryKind::File),
    ];

    sort_entries(&mut entries);

    // `R`(0x52)は `r`(0x72)より前、`E`(0x45)は `e`(0x65)より前。
    assert_eq!(names(&entries), ["README.md", "ReadMe.md", "readme.md"]);

    // 入力の順に関係なく同じ結果になる(決定的)。
    let mut reversed = vec![
        entry("ReadMe.md", EntryKind::File),
        entry("README.md", EntryKind::File),
        entry("readme.md", EntryKind::File),
    ];
    sort_entries(&mut reversed);
    assert_eq!(reversed, entries);
}

#[test]
fn 日本語の名前も決まった順に並ぶ() {
    let mut entries = vec![
        entry("資料.md", EntryKind::File),
        entry("b.md", EntryKind::File),
        entry("あ.md", EntryKind::File),
        entry("A.md", EntryKind::File),
    ];

    sort_entries(&mut entries);

    assert_eq!(names(&entries), ["A.md", "b.md", "あ.md", "資料.md"]);
}

#[test]
fn フォルダへのシンボリックリンクはフォルダと同じ並びで展開しない種類() {
    let mut entries = vec![
        entry("b.txt", EntryKind::File),
        entry("linked", EntryKind::DirectoryLink),
        entry("zdir", EntryKind::Directory),
        entry("adir", EntryKind::Directory),
        entry("alias.txt", EntryKind::FileLink),
    ];

    sort_entries(&mut entries);

    assert_eq!(
        names(&entries),
        ["adir", "linked", "zdir", "alias.txt", "b.txt"]
    );
    assert!(is_folder_like(EntryKind::Directory));
    assert!(is_folder_like(EntryKind::DirectoryLink));
    assert!(!is_folder_like(EntryKind::File));
    assert!(!is_folder_like(EntryKind::FileLink));
    assert!(!is_folder_like(EntryKind::Other));

    // 展開できるのはふつうのフォルダだけ(§15.2 #8)。
    assert!(is_expandable(EntryKind::Directory));
    assert!(!is_expandable(EntryKind::DirectoryLink));
    assert!(!is_expandable(EntryKind::File));
    assert!(!is_expandable(EntryKind::FileLink));
    assert!(!is_expandable(EntryKind::Other));
}

#[test]
fn 同じ名前で種類だけが違うときも順が決まる() {
    let dir = entry("x", EntryKind::Directory);
    let link = entry("x", EntryKind::DirectoryLink);

    assert_eq!(compare_entries(&dir, &link), std::cmp::Ordering::Less);
    assert_eq!(compare_entries(&link, &dir), std::cmp::Ordering::Greater);
    assert_eq!(compare_entries(&dir, &dir), std::cmp::Ordering::Equal);
}

// ---- 監視の変更のまとめ(FR-014、PRD C7、§7.1「外部でのファイル変更」、【仮定】19) ----

#[test]
fn ファイルの変更はその親のフォルダの変更にまとまる() {
    let result = summarize(
        &[
            "/Users/x/dev/app/src/new.txt",
            "/Users/x/dev/app/README.md",
            "/Users/x/dev/app/src/ui/a b.ts",
        ],
        false,
    );

    assert!(!result.full_reload);
    assert_eq!(result.dirs, dirs(&["", "src", "src/ui"]));
}

#[test]
fn 同じフォルダの変更は_1_つにまとまる() {
    let result = summarize(
        &[
            "/Users/x/dev/app/src/b.ts",
            "/Users/x/dev/app/src/a.ts",
            "/Users/x/dev/app/src/a.ts",
            "/Users/x/dev/app/src/c.ts",
        ],
        false,
    );

    assert_eq!(result.dirs, dirs(&["src"]));
}

#[test]
fn 変わったフォルダは入力の順に関係なく同じ順で返る() {
    let a = summarize(
        &[
            "/Users/x/dev/app/z/1",
            "/Users/x/dev/app/a/1",
            "/Users/x/dev/app/m/1",
        ],
        false,
    );
    let b = summarize(
        &[
            "/Users/x/dev/app/m/1",
            "/Users/x/dev/app/z/1",
            "/Users/x/dev/app/a/1",
        ],
        false,
    );

    assert_eq!(a, b);
    assert_eq!(a.dirs, dirs(&["a", "m", "z"]));
}

#[test]
fn ルート自体の変更はルートの読み直しになる() {
    let result = summarize(&["/Users/x/dev/app"], false);

    assert_eq!(result.dirs, vec![RelPath::root()]);
}

#[test]
fn 除外の中の変更は捨てる() {
    let result = summarize(
        &[
            "/Users/x/dev/app/.git/index",
            "/Users/x/dev/app/.git",
            "/Users/x/dev/app/node_modules/react/index.js",
            "/Users/x/dev/app/packages/web/node_modules/.bin/vite",
            "/Users/x/dev/app/docs/.DS_Store",
            "/Users/x/dev/app/src/main.ts",
        ],
        false,
    );

    assert_eq!(result.dirs, dirs(&["src"]));

    let only_excluded = summarize(&["/Users/x/dev/app/.git/HEAD"], false);
    assert_eq!(only_excluded, ChangeBatch::default());
}

#[test]
fn 隠しファイル表示がオンなら除外の中の変更も渡す() {
    let result = summarize(
        &[
            "/Users/x/dev/app/.git/index",
            "/Users/x/dev/app/node_modules/react/index.js",
            "/Users/x/dev/app/docs/.DS_Store",
        ],
        true,
    );

    assert_eq!(result.dirs, dirs(&[".git", "docs", "node_modules/react"]));
}

#[test]
fn ルートの外のパスは捨てる() {
    let result = summarize(
        &[
            // 名前の前方が同じだけの別のフォルダ。
            "/Users/x/dev/app2/src/a.ts",
            "/Users/x/dev/other/a.ts",
            // ルートの親(ルートの外)。
            "/Users/x/dev",
            // `..` で外へ出るもの・`..` を含むもの。
            "/Users/x/dev/app/../secret/a.ts",
            "/Users/x/dev/app/src/../a.ts",
            // 相対パス。
            "src/a.ts",
            "",
            "/Users/x/dev/app/src/ok.ts",
        ],
        false,
    );

    assert_eq!(result.dirs, dirs(&["src"]));
}

#[test]
fn ルートの外のパスだけなら何も渡さない() {
    let result = summarize(&["/etc/passwd", "/Users/x/dev/app/../x"], false);

    assert_eq!(result, ChangeBatch::default());
    assert!(result.is_empty());
}

#[test]
fn ルートの末尾にスラッシュがあっても同じにまとまる() {
    let result = summarize_changes(
        Path::new("/Users/x/dev/app/"),
        &batch(&["/Users/x/dev/app/src/a.ts", "/Users/x/dev/app2/a.ts"]),
        &ExclusionRule::default(),
    );

    assert_eq!(result.dirs, dirs(&["src"]));
}

#[test]
fn _1000_件を超えたら全部読み直しの印だけにする() {
    assert_eq!(MAX_CHANGED_DIRS, 1000);

    let at_limit: Vec<String> = (0..MAX_CHANGED_DIRS)
        .map(|i| format!("{ROOT}/d{i:04}/f.txt"))
        .collect();
    let at_limit_refs: Vec<&str> = at_limit.iter().map(String::as_str).collect();
    let listed = summarize(&at_limit_refs, false);
    assert!(!listed.full_reload);
    assert_eq!(listed.dirs.len(), MAX_CHANGED_DIRS);

    let over: Vec<String> = (0..=MAX_CHANGED_DIRS)
        .map(|i| format!("{ROOT}/d{i:04}/f.txt"))
        .collect();
    let over_refs: Vec<&str> = over.iter().map(String::as_str).collect();
    let reload = summarize(&over_refs, false);
    assert!(reload.full_reload);
    assert!(reload.dirs.is_empty());
}

#[test]
fn 同じフォルダの大量の変更は_1_つのフォルダのまま() {
    let many: Vec<String> = (0..5000).map(|i| format!("{ROOT}/src/f{i}.txt")).collect();
    let refs: Vec<&str> = many.iter().map(String::as_str).collect();

    let result = summarize(&refs, false);

    assert!(!result.full_reload);
    assert_eq!(result.dirs, dirs(&["src"]));
}

#[test]
fn 除外の中の大量の変更は上限に数えない() {
    let many: Vec<String> = (0..5000)
        .map(|i| format!("{ROOT}/node_modules/p{i}/index.js"))
        .collect();
    let refs: Vec<&str> = many.iter().map(String::as_str).collect();

    let result = summarize(&refs, false);

    assert_eq!(result, ChangeBatch::default());
}

#[test]
fn os_が伝えきれなかった印は全部読み直しにする() {
    let result = summarize_changes(
        Path::new(ROOT),
        &WatchBatch {
            paths: vec![PathBuf::from("/Users/x/dev/app/src/a.ts")],
            rescan: true,
        },
        &ExclusionRule::default(),
    );

    assert!(result.full_reload);
    assert!(result.dirs.is_empty());
    assert!(!result.is_empty());
}

#[test]
fn まとめた変更はプロジェクト_id_を付けてイベントの中身にできる() {
    let project_id = ProjectId::from_random_bytes([1; 16]);
    let change =
        summarize(&["/Users/x/dev/app/src/a.ts"], false).into_tree_change(project_id.clone());

    assert_eq!(
        change,
        TreeChange {
            project_id,
            dirs: dirs(&["src"]),
            full_reload: false,
        }
    );
}
