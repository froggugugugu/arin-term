//! 範囲の検査(`PathScopeGuard`。設計書 §5.1・§7.2・§15.2 #8、FR-035、計画書 T39a)。
//!
//! ルートの外(シンボリックリンクで外へ出るものを含む)、ルート自体、自分自身や子孫への移動を、
//! 偽のファイルシステムで確かめる。

#[path = "support/fake_fs.rs"]
mod fake_fs;

use std::path::{Path, PathBuf};

use arin_core::fileops::scope::{PathScopeGuard, ScopeError};
use arin_core::fileops::{FileOpOutcome, InvalidNameReason, NotAllowedReason};
use arin_core::paths::RelPath;
use arin_core::ports::fs::{EntryKind, FsError};
use fake_fs::{Call, FakeFs};

const ROOT: &str = "/x/app";

fn rel(text: &str) -> RelPath {
    RelPath::parse(text).unwrap_or_else(|e| panic!("{text:?} を読めなかった: {e}"))
}

/// ルート `/x/app` と、その外の `/x/app2`・`/x/outside`・`/etc` がある偽のファイルシステム。
fn sample() -> FakeFs {
    FakeFs::new()
        .file("/x/app/README.md")
        .file("/x/app/src/main.rs")
        .file("/x/app/src/ui/a b.ts")
        .dir("/x/app/docs/deep/er")
        .file("/x/app2/secret.txt")
        .dir("/x/outside/inner")
        .file("/etc/passwd")
}

fn not_allowed(result: Result<impl std::fmt::Debug, ScopeError>) -> NotAllowedReason {
    match result {
        Err(ScopeError::NotAllowed(reason)) => reason,
        other => panic!("対象にできないはずが {other:?}"),
    }
}

// ---- ルートの中 ----

#[test]
fn ルートの中の相対パスは通す() {
    let fs = sample();
    let guard = PathScopeGuard::new(&fs, Path::new(ROOT));

    let entry = guard.entry(&rel("src/ui/a b.ts")).expect("通す");
    assert_eq!(entry.rel, rel("src/ui/a b.ts"));
    assert_eq!(entry.path, PathBuf::from("/x/app/src/ui/a b.ts"));
    assert_eq!(entry.kind, EntryKind::File);

    let folder = guard.folder(&rel("docs/deep")).expect("通す");
    assert_eq!(folder.rel, rel("docs/deep"));
    assert_eq!(folder.real, PathBuf::from("/x/app/docs/deep"));

    let root = guard
        .folder(&RelPath::root())
        .expect("ルートも置き先にできる");
    assert_eq!(root.real, PathBuf::from(ROOT));
}

#[test]
fn ルートのパスがシンボリックリンクでも実パスで比べる() {
    // `/Users/me/app` → `/private/real/app` のように、登録したパスがリンクを含む場合。
    let fs = FakeFs::new()
        .file("/private/real/app/a.txt")
        .link("/Users/me/app", "/private/real/app");
    let guard = PathScopeGuard::new(&fs, Path::new("/Users/me/app"));

    let entry = guard.entry(&rel("a.txt")).expect("通す");
    assert_eq!(entry.path, PathBuf::from("/private/real/app/a.txt"));
    assert_eq!(
        guard.folder(&RelPath::root()).expect("通す").real,
        PathBuf::from("/private/real/app")
    );
}

#[test]
fn ルートの中を指すシンボリックリンクを経由する経路は通す() {
    let fs = sample().link("/x/app/shortcut", "docs/deep");
    let guard = PathScopeGuard::new(&fs, Path::new(ROOT));

    let folder = guard.folder(&rel("shortcut/er")).expect("中を指すので通す");
    assert_eq!(folder.real, PathBuf::from("/x/app/docs/deep/er"));
}

// ---- シンボリックリンクでの脱出 ----

#[test]
fn シンボリックリンクで親がルートの外に出るものは拒否する() {
    let fs = sample()
        .link("/x/app/out", "/x/outside")
        .link("/x/app/up", "..")
        .link("/x/app/etc", "/etc");
    let guard = PathScopeGuard::new(&fs, Path::new(ROOT));

    // 外のフォルダの中の既存の項目を操作の対象にする(名称変更・移動・ゴミ箱)。
    assert_eq!(
        not_allowed(guard.entry(&rel("out/inner"))),
        NotAllowedReason::OutsideRoot
    );
    assert_eq!(
        not_allowed(guard.entry(&rel("etc/passwd"))),
        NotAllowedReason::OutsideRoot
    );
    assert_eq!(
        not_allowed(guard.entry(&rel("up/app2/secret.txt"))),
        NotAllowedReason::OutsideRoot
    );
    // 外のフォルダを置き先にする(新規作成・ペースト・移動・コピーの宛先)。
    assert_eq!(
        not_allowed(guard.folder(&rel("out"))),
        NotAllowedReason::OutsideRoot
    );
    assert_eq!(
        not_allowed(guard.folder(&rel("out/inner"))),
        NotAllowedReason::OutsideRoot
    );
    assert_eq!(
        not_allowed(guard.folder(&rel("up"))),
        NotAllowedReason::OutsideRoot
    );
    // 中のフォルダから外へ移す・写す。
    assert_eq!(
        not_allowed(guard.transfer(&rel("README.md"), &rel("out"))),
        NotAllowedReason::OutsideRoot
    );
    // 外の項目を中へ移す(元が外)。
    assert_eq!(
        not_allowed(guard.transfer(&rel("out/inner"), &rel("docs"))),
        NotAllowedReason::OutsideRoot
    );
    // どれも変える操作を呼ばない。
    assert!(fs.mutations().is_empty());
}

#[test]
fn 途中の階層のリンクで外に出るものも拒否する() {
    // `src/vendor` が外を指し、さらにその下を指定する。
    let fs = sample().link("/x/app/src/vendor", "/x/outside");
    let guard = PathScopeGuard::new(&fs, Path::new(ROOT));

    assert_eq!(
        not_allowed(guard.folder(&rel("src/vendor/inner"))),
        NotAllowedReason::OutsideRoot
    );
    assert_eq!(
        not_allowed(guard.entry(&rel("src/vendor/inner"))),
        NotAllowedReason::OutsideRoot
    );
}

#[test]
fn 外を指すリンクを経由して中へ戻る経路は中として通す() {
    // `/x/app/out` → `/x/outside`、`/x/outside/back` → `/x/app/docs`。行き着く先はルートの中。
    let fs = sample()
        .link("/x/app/out", "/x/outside")
        .link("/x/outside/back", "/x/app/docs");
    let guard = PathScopeGuard::new(&fs, Path::new(ROOT));

    let folder = guard.folder(&rel("out/back")).expect("実パスは中");
    assert_eq!(folder.real, PathBuf::from("/x/app/docs"));
}

#[test]
fn ルートの外を指すリンク自体はリンクとして操作の対象にできる() {
    // リンクはたどらずリンク自体を名称変更・ゴミ箱の対象にする(指す先は変えない)。
    let fs = sample().link("/x/app/out", "/x/outside");
    let guard = PathScopeGuard::new(&fs, Path::new(ROOT));

    let entry = guard.entry(&rel("out")).expect("リンク自体はルートの中");
    assert_eq!(entry.path, PathBuf::from("/x/app/out"));
    assert_eq!(entry.kind, EntryKind::DirectoryLink);
}

#[test]
fn ルートを指すリンクの先の操作もルートの中として扱う() {
    let fs = sample().link("/x/app/self", ".");
    let guard = PathScopeGuard::new(&fs, Path::new(ROOT));

    let entry = guard.entry(&rel("self/README.md")).expect("中");
    assert_eq!(entry.path, PathBuf::from("/x/app/README.md"));
}

#[test]
fn 名前の前方が同じだけの別のフォルダはルートの外() {
    // `/x/app2` は文字列としては `/x/app` で始まるが、要素ごとに比べると外。
    let fs = sample()
        .link("/x/app/sibling", "/x/app2")
        .link("/x/app/prefix", "/x/app2/");
    let guard = PathScopeGuard::new(&fs, Path::new(ROOT));

    assert_eq!(
        not_allowed(guard.folder(&rel("sibling"))),
        NotAllowedReason::OutsideRoot
    );
    assert_eq!(
        not_allowed(guard.entry(&rel("sibling/secret.txt"))),
        NotAllowedReason::OutsideRoot
    );
    assert_eq!(
        not_allowed(guard.folder(&rel("prefix"))),
        NotAllowedReason::OutsideRoot
    );
}

#[test]
fn ルートの親を指すリンクの先は外() {
    let fs = sample().link("/x/app/parent", "/x");
    let guard = PathScopeGuard::new(&fs, Path::new(ROOT));

    assert_eq!(
        not_allowed(guard.folder(&rel("parent"))),
        NotAllowedReason::OutsideRoot
    );
    // 親を経由してルートに戻る経路(`parent/app`)は中。
    assert_eq!(
        guard.folder(&rel("parent/app")).expect("中").real,
        PathBuf::from(ROOT)
    );
}

#[test]
fn 選んだ子のパスは検査済みの実パスの下に名前を足したもの() {
    let fs = sample().link("/x/app/shortcut", "docs");
    let guard = PathScopeGuard::new(&fs, Path::new(ROOT));

    let folder = guard.folder(&rel("shortcut")).expect("中");
    let child = folder.child("new.md").expect("正しい名前");
    assert_eq!(child.rel, rel("shortcut/new.md"));
    assert_eq!(child.path, PathBuf::from("/x/app/docs/new.md"));

    let root_child = guard
        .folder(&RelPath::root())
        .expect("ルート")
        .child("top.md")
        .expect("正しい名前");
    assert_eq!(root_child.rel, rel("top.md"));
    assert_eq!(root_child.path, PathBuf::from("/x/app/top.md"));
}

#[test]
fn 子の名前が正しくなければパスを作らない() {
    let fs = sample();
    let guard = PathScopeGuard::new(&fs, Path::new(ROOT));
    let folder = guard.folder(&rel("docs")).expect("中");

    assert_eq!(folder.child("..").err(), Some(InvalidNameReason::DotName));
    assert_eq!(
        folder.child("a/b").err(),
        Some(InvalidNameReason::ContainsSlash)
    );
    assert_eq!(folder.child("").err(), Some(InvalidNameReason::Empty));
}

// ---- ルート自体 ----

#[test]
fn ルート自体の名称変更_移動_切り取り_ゴミ箱は拒否する() {
    let fs = sample();
    let guard = PathScopeGuard::new(&fs, Path::new(ROOT));

    // 名称変更・切り取り・ゴミ箱は「既存の項目を対象にする」検査を通る。
    assert_eq!(
        not_allowed(guard.entry(&RelPath::root())),
        NotAllowedReason::RootItself
    );
    // 移動とコピーの元。
    assert_eq!(
        not_allowed(guard.transfer(&RelPath::root(), &rel("docs"))),
        NotAllowedReason::RootItself
    );
    // ルート自体の拒否はファイルシステムに問い合わせずに決まる。
    assert!(fs.calls().is_empty(), "呼ばれた: {:?}", fs.calls());
}

#[test]
fn ルートを指すリンク自体はルートではない() {
    let fs = sample().link("/x/app/self", ".");
    let guard = PathScopeGuard::new(&fs, Path::new(ROOT));

    let entry = guard.entry(&rel("self")).expect("リンク自体は対象にできる");
    assert_eq!(entry.path, PathBuf::from("/x/app/self"));
    assert_eq!(entry.kind, EntryKind::DirectoryLink);
}

#[test]
fn ルートへの新規作成とペーストは通す() {
    let fs = sample();
    let guard = PathScopeGuard::new(&fs, Path::new(ROOT));

    // 新規作成・ペーストは置き先のフォルダの検査を通る。ルートはそこに入る。
    let root = guard.folder(&RelPath::root()).expect("通す");
    assert_eq!(root.rel, RelPath::root());
    assert_eq!(
        root.child("new.txt").expect("名前").path,
        PathBuf::from("/x/app/new.txt")
    );
    // ルートの直下へ移す・写す。
    let (entry, dest) = guard
        .transfer(&rel("src/main.rs"), &RelPath::root())
        .expect("通す");
    assert_eq!(entry.path, PathBuf::from("/x/app/src/main.rs"));
    assert_eq!(dest.real, PathBuf::from(ROOT));
    assert!(fs.mutations().is_empty());
}

// ---- 自分自身と子孫 ----

#[test]
fn フォルダを自分自身や子孫へ移すことは拒否する() {
    let fs = sample();
    let guard = PathScopeGuard::new(&fs, Path::new(ROOT));

    for dest in ["docs", "docs/deep", "docs/deep/er"] {
        assert_eq!(
            not_allowed(guard.transfer(&rel("docs"), &rel(dest))),
            NotAllowedReason::IntoDescendant,
            "宛先 {dest}"
        );
    }
    // 名前の前方が同じだけの兄弟は子孫ではない。
    let fs = sample().dir("/x/app/docs2");
    let guard = PathScopeGuard::new(&fs, Path::new(ROOT));
    guard
        .transfer(&rel("docs"), &rel("docs2"))
        .expect("兄弟へは移せる");
    // 親へは移せる(同じ名前があれば手順の側で衝突になる)。
    guard
        .transfer(&rel("docs/deep/er"), &rel("docs"))
        .expect("親へは移せる");
}

#[test]
fn リンクを経由して子孫へ移すことも拒否する() {
    // `/x/app/into` → `docs/deep`。`docs` を `into` へ移すと実際には自分の子孫へ移る。
    let fs = sample().link("/x/app/into", "docs/deep");
    let guard = PathScopeGuard::new(&fs, Path::new(ROOT));

    assert_eq!(
        not_allowed(guard.transfer(&rel("docs"), &rel("into"))),
        NotAllowedReason::IntoDescendant
    );
    assert_eq!(
        not_allowed(guard.transfer(&rel("docs"), &rel("into/er"))),
        NotAllowedReason::IntoDescendant
    );
    // 元をリンクで指しても、実パスが同じフォルダなら同じ。
    assert_eq!(
        not_allowed(guard.transfer(&rel("into"), &rel("into"))),
        NotAllowedReason::IntoDescendant
    );
    assert!(fs.mutations().is_empty());
}

#[test]
fn ファイルは子孫の検査に当たらない() {
    let fs = sample();
    let guard = PathScopeGuard::new(&fs, Path::new(ROOT));

    let (entry, dest) = guard
        .transfer(&rel("README.md"), &rel("docs/deep"))
        .expect("通す");
    assert_eq!(entry.kind, EntryKind::File);
    assert_eq!(dest.real, PathBuf::from("/x/app/docs/deep"));
}

#[test]
fn フォルダへのリンクはリンク自体を移すので指す先の中へも移せる() {
    // `link` → `docs`。リンクを `docs` の中へ移しても、フォルダの中身は動かない。
    let fs = sample().link("/x/app/link", "docs");
    let guard = PathScopeGuard::new(&fs, Path::new(ROOT));

    let (entry, _) = guard
        .transfer(&rel("link"), &rel("docs"))
        .expect("リンク自体を移す");
    assert_eq!(entry.kind, EntryKind::DirectoryLink);
}

// ---- 見つからない・フォルダでない ----

#[test]
fn 無い項目は見つからないを返す() {
    let fs = sample();
    let guard = PathScopeGuard::new(&fs, Path::new(ROOT));

    assert!(matches!(
        guard.entry(&rel("nothing.md")),
        Err(ScopeError::NotFound)
    ));
    assert!(matches!(
        guard.entry(&rel("no/such/thing")),
        Err(ScopeError::NotFound)
    ));
    assert!(matches!(
        guard.entry(&rel("README.md/child")),
        Err(ScopeError::NotFound)
    ));
    assert!(matches!(
        guard.folder(&rel("nothing")),
        Err(ScopeError::NotFound)
    ));
}

#[test]
fn ルートが無ければ見つからないを返す() {
    let fs = FakeFs::new();
    let guard = PathScopeGuard::new(&fs, Path::new(ROOT));

    assert!(matches!(
        guard.folder(&RelPath::root()),
        Err(ScopeError::NotFound)
    ));
    assert!(matches!(
        guard.entry(&rel("a.txt")),
        Err(ScopeError::NotFound)
    ));
}

#[test]
fn ファイルを置き先にするとフォルダでないを返す() {
    let fs = sample();
    let guard = PathScopeGuard::new(&fs, Path::new(ROOT));

    assert!(matches!(
        guard.folder(&rel("README.md")),
        Err(ScopeError::Fs(FsError::NotADirectory(_)))
    ));
}

#[test]
fn 壊れたリンクや外の項目のリンクも項目として扱えるが置き先にはならない() {
    let fs = sample()
        .link("/x/app/broken", "/nowhere")
        .link("/x/app/pw", "/etc/passwd");
    let guard = PathScopeGuard::new(&fs, Path::new(ROOT));

    assert_eq!(
        guard.entry(&rel("broken")).expect("リンク自体").kind,
        EntryKind::Other
    );
    assert_eq!(
        guard.entry(&rel("pw")).expect("リンク自体").kind,
        EntryKind::FileLink
    );
    assert!(matches!(
        guard.folder(&rel("broken")),
        Err(ScopeError::NotFound)
    ));
}

#[test]
fn 相対のルートは受け付けない() {
    let fs = sample();
    let guard = PathScopeGuard::new(&fs, Path::new("x/app"));

    assert!(matches!(
        guard.folder(&RelPath::root()),
        Err(ScopeError::Fs(FsError::Io(_)))
    ));
    assert!(fs.calls().is_empty());
}

// ---- 実パスの正規化を信用しない ----

/// `real_dir` が `..` を含む形を返す壊れた実装(範囲の検査は要素ごとに比べるので外とみなすこと)。
struct DotDotRealDir(FakeFs);

impl arin_core::ports::fs::FileSystemPort for DotDotRealDir {
    fn read_dir(&self, dir: &Path) -> Result<Vec<arin_core::ports::fs::DirEntry>, FsError> {
        self.0.read_dir(dir)
    }
    fn entry_kind(&self, path: &Path) -> Result<EntryKind, FsError> {
        self.0.entry_kind(path)
    }
    fn real_dir(&self, dir: &Path) -> Result<PathBuf, FsError> {
        let real = self.0.real_dir(dir)?;
        if real == Path::new(ROOT) {
            Ok(real)
        } else {
            // `/x/app/../app2` のように、前方は中に見えるが実際は外。
            Ok(PathBuf::from("/x/app/../app2"))
        }
    }
    fn create_file(&self, path: &Path) -> Result<(), FsError> {
        self.0.create_file(path)
    }
    fn create_dir(&self, path: &Path) -> Result<(), FsError> {
        self.0.create_dir(path)
    }
    fn move_entry(&self, from: &Path, to: &Path) -> Result<(), FsError> {
        self.0.move_entry(from, to)
    }
    fn copy_entry(&self, from: &Path, to: &Path) -> Result<(), FsError> {
        self.0.copy_entry(from, to)
    }
    fn same_volume(&self, path: &Path, dir: &Path) -> Result<bool, FsError> {
        self.0.same_volume(path, dir)
    }
    fn trash(&self, path: &Path) -> Result<Option<PathBuf>, FsError> {
        self.0.trash(path)
    }
}

#[test]
fn 実パスにドットドットが残っていれば外とみなす() {
    let fs = DotDotRealDir(sample());
    let guard = PathScopeGuard::new(&fs, Path::new(ROOT));

    assert_eq!(
        not_allowed(guard.folder(&rel("docs"))),
        NotAllowedReason::OutsideRoot
    );
    assert_eq!(
        not_allowed(guard.entry(&rel("docs/deep"))),
        NotAllowedReason::OutsideRoot
    );
}

// ---- 応答への変換 ----

#[test]
fn 検査の失敗はファイル操作の応答になり絶対パスを載せない() {
    assert_eq!(
        FileOpOutcome::from(ScopeError::NotAllowed(NotAllowedReason::OutsideRoot)),
        FileOpOutcome::NotAllowed {
            reason: NotAllowedReason::OutsideRoot
        }
    );
    assert_eq!(
        FileOpOutcome::from(ScopeError::NotFound),
        FileOpOutcome::NotFound
    );
    for error in [
        FsError::NotADirectory("/x/app/README.md".to_owned()),
        FsError::Io("/x/app/secret".to_owned()),
    ] {
        match FileOpOutcome::from(ScopeError::Fs(error)) {
            FileOpOutcome::Failed { message } => {
                assert!(!message.is_empty());
                assert!(!message.contains('/'), "絶対パスを載せた: {message}");
            }
            other => panic!("失敗のはずが {other:?}"),
        }
    }
}

#[test]
fn 検査は読むだけでファイルシステムを変えない() {
    let fs = sample().link("/x/app/out", "/x/outside");
    let guard = PathScopeGuard::new(&fs, Path::new(ROOT));

    let _ = guard.entry(&rel("src/main.rs"));
    let _ = guard.folder(&rel("docs"));
    let _ = guard.transfer(&rel("docs"), &rel("src"));
    let _ = guard.entry(&rel("out/inner"));

    assert!(fs.mutations().is_empty());
    assert!(
        fs.calls()
            .iter()
            .all(|c| matches!(c, Call::RealDir(_) | Call::EntryKind(_)))
    );
}
