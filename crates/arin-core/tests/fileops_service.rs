//! ファイル操作の手順(`FileOperationService`)、衝突、切り取り(`FileClipboard`)。
//! 設計書 §5.1・§5.2・§7.1「ファイル操作」・§9.1、FR-025〜030・035・036、計画書 T39b、M3 の【仮定】12〜15。
//!
//! 偽のファイルシステムは上書きと削除を求められると panic するので、その経路が無いことも同時に確かめる。

#[path = "support/fake_fs.rs"]
mod fake_fs;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use arin_core::fileops::clipboard::{FileClipboard, PasteSource};
use arin_core::fileops::service::FileOperationService;
use arin_core::fileops::{
    ConflictResolution, FileOpOutcome, InvalidNameReason, NewEntryKind, NotAllowedReason,
};
use arin_core::paths::RelPath;
use arin_core::ports::fs::FsError;
use arin_core::ports::pasteboard::{FilePasteboardPort, PasteboardError};
use arin_core::project::ProjectId;
use fake_fs::{Call, FakeFs};

const ROOT: &str = "/x/app";
const OTHER_ROOT: &str = "/x/other";

fn rel(text: &str) -> RelPath {
    RelPath::parse(text).unwrap_or_else(|e| panic!("{text:?} を読めなかった: {e}"))
}

fn p(text: &str) -> PathBuf {
    PathBuf::from(text)
}

fn root() -> &'static Path {
    Path::new(ROOT)
}

fn done(select: &str) -> FileOpOutcome {
    FileOpOutcome::Done {
        select: Some(rel(select)),
    }
}

/// 単一の項目の衝突(数は 1)。
fn conflict(name: &str) -> FileOpOutcome {
    conflicts(name, 1)
}

/// 衝突(最初の名前と、衝突した項目の数)。
fn conflicts(name: &str, total: u32) -> FileOpOutcome {
    FileOpOutcome::Conflict {
        name: name.to_owned(),
        total,
    }
}

fn failed_message(outcome: &FileOpOutcome) -> &str {
    match outcome {
        FileOpOutcome::Failed { message } => message,
        other => panic!("失敗を返す: {other:?}"),
    }
}

fn sample() -> FakeFs {
    FakeFs::new()
        .file("/x/app/src/main.ts")
        .file("/x/app/docs/a.md")
        .dir("/x/app/notes")
        .file("/x/app/notes/a.md")
        .file("/x/outside/secret.txt")
        .file("/x/other/b.txt")
}

fn project(n: u8) -> ProjectId {
    ProjectId::from_random_bytes([n; 16])
}

/// プロジェクトの ID からルートを引く(T42c は `Workspace` から引く)。
fn roots(id: &ProjectId) -> Option<PathBuf> {
    if *id == project(1) {
        Some(p(ROOT))
    } else if *id == project(2) {
        Some(p(OTHER_ROOT))
    } else {
        None
    }
}

/// 偽のペーストボード(ファイルの URL と変更番号)。
#[derive(Default)]
struct FakePasteboard {
    inner: Mutex<(Vec<PathBuf>, i64)>,
}

impl FakePasteboard {
    /// ほかのアプリ(Finder など)がファイルの URL を書いたことにする。
    fn external_write(&self, paths: &[&str]) {
        let mut inner = self.inner.lock().expect("ロック");
        inner.0 = paths.iter().map(PathBuf::from).collect();
        inner.1 += 1;
    }

    /// ほかのアプリが文字列だけを書いたことにする(ファイルの URL は無くなる)。
    fn external_write_text(&self) {
        let mut inner = self.inner.lock().expect("ロック");
        inner.0.clear();
        inner.1 += 1;
    }

    fn urls(&self) -> Vec<PathBuf> {
        self.inner.lock().expect("ロック").0.clone()
    }
}

impl FilePasteboardPort for FakePasteboard {
    fn write_file_urls(&self, paths: &[&Path]) -> Result<i64, PasteboardError> {
        let mut inner = self.inner.lock().expect("ロック");
        inner.0 = paths.iter().map(|p| p.to_path_buf()).collect();
        inner.1 += 1;
        Ok(inner.1)
    }

    fn read_file_urls(&self) -> Result<Vec<PathBuf>, PasteboardError> {
        Ok(self.urls())
    }

    fn change_count(&self) -> i64 {
        self.inner.lock().expect("ロック").1
    }
}

/// 応答に絶対パスが載っていないこと(`/x/` で始まる文字列を含まない)。
fn assert_no_absolute(outcome: &FileOpOutcome) {
    let json = serde_json::to_string(outcome).expect("JSON にできる");
    assert!(!json.contains("/x/"), "応答に絶対パスが載った: {json}");
}

// ---- 新規作成(FR-025) ----

#[test]
fn 新規ファイルは空で作られ作った相対パスを返す() {
    let fs = sample();
    let service = FileOperationService::new(&fs);

    let outcome = service.create(root(), &rel("src"), "a.ts", NewEntryKind::File, None);

    assert_eq!(outcome, done("src/a.ts"));
    assert!(fs.exists("/x/app/src/a.ts"));
    assert_eq!(fs.mutations(), vec![Call::CreateFile(p("/x/app/src/a.ts"))]);
    assert_no_absolute(&outcome);
}

#[test]
fn 新規フォルダを作る() {
    let fs = sample();
    let service = FileOperationService::new(&fs);

    let outcome = service.create(root(), &RelPath::root(), "lib", NewEntryKind::Folder, None);

    assert_eq!(outcome, done("lib"));
    assert_eq!(fs.mutations(), vec![Call::CreateDir(p("/x/app/lib"))]);
}

#[test]
fn 同名があれば作らず衝突を返す() {
    let fs = sample();
    let service = FileOperationService::new(&fs);

    let outcome = service.create(root(), &rel("docs"), "a.md", NewEntryKind::File, None);

    assert_eq!(outcome, conflict("a.md"));
    // 失敗した作成の 1 回だけ(何も変わっていない)。
    assert_eq!(
        fs.mutations(),
        vec![Call::CreateFile(p("/x/app/docs/a.md"))]
    );
    assert!(fs.trashed().is_empty());
}

#[test]
fn 正しくない名前はポートを呼ばずに拒否する() {
    let fs = sample();
    let service = FileOperationService::new(&fs);

    assert_eq!(
        service.create(root(), &rel("src"), "a/b", NewEntryKind::File, None),
        FileOpOutcome::InvalidName {
            reason: InvalidNameReason::ContainsSlash
        }
    );
    assert_eq!(
        service.rename(root(), &rel("src/main.ts"), "", None),
        FileOpOutcome::InvalidName {
            reason: InvalidNameReason::Empty
        }
    );
    assert!(fs.calls().is_empty());
}

#[test]
fn 作成先のフォルダが実行の時点で無ければ見つからないを返す() {
    let fs = sample();
    let service = FileOperationService::new(&fs);

    let outcome = service.create(root(), &rel("gone"), "a.ts", NewEntryKind::File, None);

    assert_eq!(outcome, FileOpOutcome::NotFound);
    assert!(fs.mutations().is_empty());
}

// ---- 名称変更(FR-026・FR-036) ----

#[test]
fn 名称変更でディスクの名前が変わる() {
    let fs = sample();
    let service = FileOperationService::new(&fs);

    let outcome = service.rename(root(), &rel("src/main.ts"), "index.ts", None);

    assert_eq!(outcome, done("src/index.ts"));
    assert!(!fs.exists("/x/app/src/main.ts"));
    assert!(fs.exists("/x/app/src/index.ts"));
    assert_eq!(
        fs.mutations(),
        vec![Call::Move(
            p("/x/app/src/main.ts"),
            p("/x/app/src/index.ts")
        )]
    );
}

#[test]
fn 名称変更の対象が実行の時点で無ければ何も変えず見つからないを返す() {
    // メニューを開いたあと(画面は `src/old.ts` を見ていた)に外から消された(AC-T24)。
    let fs = sample();
    let service = FileOperationService::new(&fs);

    let outcome = service.rename(root(), &rel("src/old.ts"), "new.ts", None);

    assert_eq!(outcome, FileOpOutcome::NotFound);
    assert!(fs.mutations().is_empty());
    assert!(!fs.exists("/x/app/src/new.ts"));
}

#[test]
fn 存在を確かめたあと移す瞬間に消えていても見つからないを返す() {
    // 存在の確認と移動のあいだに消された(ポートの移動の失敗で判定する)。
    let fs = sample().fail_on(
        Call::Move(p("/x/app/src/main.ts"), p("/x/app/src/index.ts")),
        FsError::NotFound("/x/app/src/main.ts".to_owned()),
    );
    let service = FileOperationService::new(&fs);

    let outcome = service.rename(root(), &rel("src/main.ts"), "index.ts", None);

    assert_eq!(outcome, FileOpOutcome::NotFound);
    assert!(fs.exists("/x/app/src/main.ts"));
    assert!(!fs.exists("/x/app/src/index.ts"));
}

#[test]
fn メニューを開いたあとに同名ができていたら衝突を返す() {
    // 対象を確かめた直後に、外から同じ名前のファイルが作られた(FR-036 の派生)。
    let fs = sample().after_call_file(
        Call::EntryKind(p("/x/app/src/main.ts")),
        "/x/app/src/index.ts",
    );
    let service = FileOperationService::new(&fs);

    let outcome = service.rename(root(), &rel("src/main.ts"), "index.ts", None);

    assert_eq!(outcome, conflict("index.ts"));
    assert!(fs.exists("/x/app/src/main.ts"));
    assert!(fs.exists("/x/app/src/index.ts"));
    assert!(fs.trashed().is_empty());
}

#[test]
fn 大文字小文字だけを変える名称変更は衝突にしない() {
    let fs = sample();
    let service = FileOperationService::new(&fs);

    let outcome = service.rename(root(), &rel("src/main.ts"), "Main.ts", None);

    assert_eq!(outcome, done("src/Main.ts"));
    assert!(fs.exists("/x/app/src/Main.ts"));
    assert!(fs.trashed().is_empty());
}

#[test]
fn 同じ名前への名称変更は何もしない() {
    let fs = sample();
    let service = FileOperationService::new(&fs);

    let outcome = service.rename(root(), &rel("src/main.ts"), "main.ts", None);

    assert_eq!(outcome, done("src/main.ts"));
    assert!(fs.mutations().is_empty());
}

// ---- 移動・コピー(FR-029・FR-030) ----

#[test]
fn 移動でフォルダの中へ動く() {
    let fs = sample();
    let service = FileOperationService::new(&fs);

    let outcome = service.move_entry(root(), &rel("src/main.ts"), &rel("docs"), None);

    assert_eq!(outcome, done("docs/main.ts"));
    assert!(!fs.exists("/x/app/src/main.ts"));
    assert!(fs.exists("/x/app/docs/main.ts"));
}

#[test]
fn 移動は宛先に同名があれば何も変えず衝突を返す() {
    // `docs/a.md` があるところへ別の `a.md` をドロップした(AC-T18)。中止は呼び直さないこと。
    let fs = sample();
    let service = FileOperationService::new(&fs);

    let outcome = service.move_entry(root(), &rel("notes/a.md"), &rel("docs"), None);

    assert_eq!(outcome, conflict("a.md"));
    assert!(fs.exists("/x/app/notes/a.md"));
    assert!(fs.exists("/x/app/docs/a.md"));
    assert!(fs.trashed().is_empty());
    assert_no_absolute(&outcome);
}

#[test]
fn 置き換えるは既存をゴミ箱に入れてから置き上書きしない() {
    let fs = sample();
    let service = FileOperationService::new(&fs);

    let outcome = service.move_entry(
        root(),
        &rel("notes/a.md"),
        &rel("docs"),
        Some(ConflictResolution::Replace),
    );

    assert_eq!(outcome, done("docs/a.md"));
    assert_eq!(
        fs.mutations(),
        vec![
            Call::Move(p("/x/app/notes/a.md"), p("/x/app/docs/a.md")),
            Call::Trash(p("/x/app/docs/a.md")),
            Call::Move(p("/x/app/notes/a.md"), p("/x/app/docs/a.md")),
        ]
    );
    assert_eq!(fs.trashed(), vec![p("/x/app/docs/a.md")]);
    assert!(!fs.exists("/x/app/notes/a.md"));
    assert!(fs.exists("/x/app/docs/a.md"));
}

#[test]
fn 置き換えの途中でまた同名ができたら衝突を返す() {
    // 既存をゴミ箱に入れた直後に、外から同じ名前ができた。元は動かさない。
    let fs = sample().after_call_file(Call::Trash(p("/x/app/docs/a.md")), "/x/app/docs/a.md");
    let service = FileOperationService::new(&fs);

    let outcome = service.move_entry(
        root(),
        &rel("notes/a.md"),
        &rel("docs"),
        Some(ConflictResolution::Replace),
    );

    assert_eq!(outcome, conflict("a.md"));
    assert!(fs.exists("/x/app/notes/a.md"), "元はそのまま");
    assert!(
        fs.exists("/x/app/docs/a.md"),
        "外から作られたものはそのまま"
    );
    assert_eq!(
        fs.trashed(),
        vec![p("/x/app/docs/a.md")],
        "前の既存はゴミ箱にある"
    );
}

#[test]
fn ゴミ箱に入れられなければ置き換えず失敗を返す() {
    let fs = sample().fail_on(
        Call::Trash(p("/x/app/docs/a.md")),
        FsError::Io("権限が無い /x/app/docs/a.md".to_owned()),
    );
    let service = FileOperationService::new(&fs);

    let outcome = service.move_entry(
        root(),
        &rel("notes/a.md"),
        &rel("docs"),
        Some(ConflictResolution::Replace),
    );

    assert!(
        matches!(outcome, FileOpOutcome::Failed { .. }),
        "{outcome:?}"
    );
    assert_no_absolute(&outcome);
    assert!(fs.exists("/x/app/notes/a.md"));
    assert!(fs.exists("/x/app/docs/a.md"));
    assert!(fs.trashed().is_empty());
    // ゴミ箱の失敗のあとに、移動も削除も呼ばない。
    assert_eq!(
        fs.mutations().last(),
        Some(&Call::Trash(p("/x/app/docs/a.md")))
    );
}

#[test]
fn 既存をゴミ箱に入れたあとに移動が失敗したら既存はゴミ箱に残り元はそのまま() {
    // 2 回目の移動だけが失敗する(権限など)。消えたものは無い: 既存はゴミ箱(戻せる)、元は元の場所。
    let fs = sample()
        .fail_on(
            Call::Move(p("/x/app/notes/a.md"), p("/x/app/docs/a.md")),
            FsError::AlreadyExists("/x/app/docs/a.md".to_owned()),
        )
        .fail_on(
            Call::Move(p("/x/app/notes/a.md"), p("/x/app/docs/a.md")),
            FsError::Io("権限が無い".to_owned()),
        );
    let service = FileOperationService::new(&fs);

    let outcome = service.move_entry(
        root(),
        &rel("notes/a.md"),
        &rel("docs"),
        Some(ConflictResolution::Replace),
    );

    let FileOpOutcome::Failed { message } = &outcome else {
        panic!("失敗を返す: {outcome:?}");
    };
    assert!(
        message.contains("ゴミ箱"),
        "既存がゴミ箱にあることを伝える: {message}"
    );
    assert_no_absolute(&outcome);
    assert!(fs.exists("/x/app/notes/a.md"));
    assert_eq!(fs.trashed(), vec![p("/x/app/docs/a.md")]);
}

#[test]
fn 元の項目を含むものは置き換えない() {
    // `a/a` をルートへ移すときの既存 `a` は元の親。ゴミ箱に入れると元も消えるので拒否する。
    let fs = FakeFs::new().file("/x/app/a/a");
    let service = FileOperationService::new(&fs);

    let outcome = service.move_entry(
        root(),
        &rel("a/a"),
        &RelPath::root(),
        Some(ConflictResolution::Replace),
    );

    assert!(
        matches!(outcome, FileOpOutcome::Failed { .. }),
        "{outcome:?}"
    );
    assert!(fs.trashed().is_empty());
    assert!(fs.exists("/x/app/a/a"));
}

#[test]
fn 両方残すは番号付きの名前で置く() {
    let fs = sample();
    let service = FileOperationService::new(&fs);

    let outcome = service.move_entry(
        root(),
        &rel("notes/a.md"),
        &rel("docs"),
        Some(ConflictResolution::KeepBoth),
    );

    assert_eq!(outcome, done("docs/a 2.md"));
    assert!(fs.exists("/x/app/docs/a.md"));
    assert!(fs.exists("/x/app/docs/a 2.md"));
    assert!(!fs.exists("/x/app/notes/a.md"));
    assert!(fs.trashed().is_empty());
}

#[test]
fn 衝突が無ければ対処を付けても普通に置く() {
    let fs = sample();
    let service = FileOperationService::new(&fs);

    let outcome = service.move_entry(
        root(),
        &rel("src/main.ts"),
        &rel("docs"),
        Some(ConflictResolution::Replace),
    );

    assert_eq!(outcome, done("docs/main.ts"));
    assert!(fs.trashed().is_empty());
}

#[test]
fn 同じフォルダへの移動は何もしない() {
    let fs = sample();
    let service = FileOperationService::new(&fs);

    let outcome = service.move_entry(
        root(),
        &rel("docs/a.md"),
        &rel("docs"),
        Some(ConflictResolution::Replace),
    );

    assert_eq!(outcome, done("docs/a.md"));
    assert!(fs.mutations().is_empty());
}

#[test]
fn コピーは元を残す() {
    let fs = sample();
    let service = FileOperationService::new(&fs);

    let outcome = service.copy_entry(root(), &rel("src/main.ts"), &rel("docs"), None);

    assert_eq!(outcome, done("docs/main.ts"));
    assert!(fs.exists("/x/app/src/main.ts"));
    assert!(fs.exists("/x/app/docs/main.ts"));
    assert_eq!(
        fs.mutations(),
        vec![Call::Copy(
            p("/x/app/src/main.ts"),
            p("/x/app/docs/main.ts")
        )]
    );
}

#[test]
fn 同じフォルダへのコピーは衝突になり両方残すで複製になる() {
    let fs = sample();
    let service = FileOperationService::new(&fs);

    assert_eq!(
        service.copy_entry(root(), &rel("docs/a.md"), &rel("docs"), None),
        conflict("a.md")
    );
    assert_eq!(
        service.copy_entry(
            root(),
            &rel("docs/a.md"),
            &rel("docs"),
            Some(ConflictResolution::KeepBoth)
        ),
        done("docs/a 2.md")
    );
}

#[test]
fn 同じフォルダへのコピーを置き換えにすると元をゴミ箱に入れずに失敗する() {
    let fs = sample();
    let service = FileOperationService::new(&fs);

    let outcome = service.copy_entry(
        root(),
        &rel("docs/a.md"),
        &rel("docs"),
        Some(ConflictResolution::Replace),
    );

    assert!(
        matches!(outcome, FileOpOutcome::Failed { .. }),
        "{outcome:?}"
    );
    assert!(fs.trashed().is_empty());
    assert!(fs.exists("/x/app/docs/a.md"));
}

#[test]
fn 別のボリュームへの移動は失敗を返し何も変えない() {
    let fs = sample().fail_on(
        Call::Move(p("/x/app/src/main.ts"), p("/x/app/docs/main.ts")),
        FsError::CrossDevice("/x/app/src/main.ts".to_owned()),
    );
    let service = FileOperationService::new(&fs);

    let outcome = service.move_entry(root(), &rel("src/main.ts"), &rel("docs"), None);

    assert_eq!(
        outcome,
        FileOpOutcome::Failed {
            message: "別のディスクへは移動できません".to_owned()
        }
    );
    assert!(fs.exists("/x/app/src/main.ts"));
    assert!(!fs.exists("/x/app/docs/main.ts"));
    // コピーしてから消すことはしない(【仮定】14)。
    assert_eq!(
        fs.mutations(),
        vec![Call::Move(
            p("/x/app/src/main.ts"),
            p("/x/app/docs/main.ts")
        )]
    );
}

// ---- ゴミ箱(FR-028) ----

#[test]
fn ゴミ箱に入れると対象が消えゴミ箱の操作だけが呼ばれる() {
    let fs = sample();
    let service = FileOperationService::new(&fs);

    let outcome = service.trash(root(), &rel("docs/a.md"));

    assert_eq!(outcome, FileOpOutcome::Done { select: None });
    assert!(!fs.exists("/x/app/docs/a.md"));
    assert_eq!(fs.mutations(), vec![Call::Trash(p("/x/app/docs/a.md"))]);
}

#[test]
fn 無いものをゴミ箱に入れると見つからないを返す() {
    let fs = sample();
    let service = FileOperationService::new(&fs);

    assert_eq!(
        service.trash(root(), &rel("docs/gone.md")),
        FileOpOutcome::NotFound
    );
    assert!(fs.mutations().is_empty());
}

// ---- 範囲(FR-035) ----

#[test]
fn ルートの外とルート自体への操作はポートを呼ばずに拒否する() {
    let fs = sample();
    let service = FileOperationService::new(&fs);
    let root_itself = FileOpOutcome::NotAllowed {
        reason: NotAllowedReason::RootItself,
    };

    // ルート自体: ポートを 1 回も呼ばない。
    assert_eq!(
        service.rename(root(), &RelPath::root(), "b", None),
        root_itself
    );
    assert_eq!(service.trash(root(), &RelPath::root()), root_itself);
    assert_eq!(
        service.move_entry(root(), &RelPath::root(), &rel("docs"), None),
        root_itself
    );
    assert_eq!(
        service.copy_entry(root(), &RelPath::root(), &rel("docs"), None),
        root_itself
    );
    assert!(fs.calls().is_empty(), "{:?}", fs.calls());

    // 自分の子孫への移動: 相対パスで決まるのでポートを呼ばない。
    assert_eq!(
        service.move_entry(root(), &rel("docs"), &rel("docs/deep"), None),
        FileOpOutcome::NotAllowed {
            reason: NotAllowedReason::IntoDescendant
        }
    );
    assert!(fs.calls().is_empty(), "{:?}", fs.calls());
}

#[test]
fn シンボリックリンクでルートの外へ出る操作は何も変えずに拒否する() {
    let fs = sample().link("/x/app/out", "/x/outside");
    let service = FileOperationService::new(&fs);
    let outside = FileOpOutcome::NotAllowed {
        reason: NotAllowedReason::OutsideRoot,
    };

    assert_eq!(
        service.rename(root(), &rel("out/secret.txt"), "b.txt", None),
        outside
    );
    assert_eq!(service.trash(root(), &rel("out/secret.txt")), outside);
    assert_eq!(
        service.create(root(), &rel("out"), "new.txt", NewEntryKind::File, None),
        outside
    );
    assert_eq!(
        service.move_entry(root(), &rel("src/main.ts"), &rel("out"), None),
        outside
    );
    assert_eq!(
        service.copy_entry(root(), &rel("out/secret.txt"), &rel("docs"), None),
        outside
    );
    // 調べる操作は呼んでよいが、変える操作は 1 回も呼ばない。
    assert!(fs.mutations().is_empty(), "{:?}", fs.mutations());
    assert!(fs.exists("/x/outside/secret.txt"));
}

// ---- コピー・切り取り・ペースト(FR-027) ----

#[test]
fn ツリーでコピーするとペーストボードにファイルの_url_が載る() {
    let fs = sample();
    let service = FileOperationService::new(&fs);
    let board = FakePasteboard::default();
    let mut clipboard = FileClipboard::new();

    let outcome = service.copy_to_pasteboard(&board, &mut clipboard, root(), &[rel("docs/a.md")]);

    assert_eq!(outcome, FileOpOutcome::Done { select: None });
    assert_eq!(board.urls(), vec![p("/x/app/docs/a.md")]);
    assert!(clipboard.cut_items().is_none());
    assert!(fs.mutations().is_empty());
}

#[test]
fn ルート自体は切り取れない() {
    let fs = sample();
    let service = FileOperationService::new(&fs);
    let board = FakePasteboard::default();
    let mut clipboard = FileClipboard::new();

    let outcome = service.cut(
        &board,
        &mut clipboard,
        &project(1),
        root(),
        &[RelPath::root()],
    );

    assert_eq!(
        outcome,
        FileOpOutcome::NotAllowed {
            reason: NotAllowedReason::RootItself
        }
    );
    assert!(board.urls().is_empty(), "ペーストボードに書かない");
    assert!(clipboard.cut_items().is_none());
}

#[test]
fn 切り取ってペーストボードが変わっていなければ移動になる() {
    let fs = sample();
    let service = FileOperationService::new(&fs);
    let board = FakePasteboard::default();
    let mut clipboard = FileClipboard::new();

    assert_eq!(
        service.cut(
            &board,
            &mut clipboard,
            &project(1),
            root(),
            &[rel("src/main.ts")]
        ),
        FileOpOutcome::Done { select: None }
    );
    assert_eq!(board.urls(), vec![p("/x/app/src/main.ts")]);
    assert!(
        fs.exists("/x/app/src/main.ts"),
        "切り取っただけでは動かない"
    );

    let outcome = service.paste(&board, &mut clipboard, &roots, root(), &rel("docs"), None);

    assert_eq!(outcome, done("docs/main.ts"));
    assert!(!fs.exists("/x/app/src/main.ts"));
    assert!(fs.exists("/x/app/docs/main.ts"));
    // 移動 1 回で行う(コピーしてから消さない)。
    assert_eq!(
        fs.mutations(),
        vec![Call::Move(
            p("/x/app/src/main.ts"),
            p("/x/app/docs/main.ts")
        )]
    );
    assert!(
        clipboard.cut_items().is_none(),
        "移したら切り取りの状態を消す"
    );
}

#[test]
fn 切り取ったあとにペーストボードが変わっていたらコピーになる() {
    let fs = sample();
    let service = FileOperationService::new(&fs);
    let board = FakePasteboard::default();
    let mut clipboard = FileClipboard::new();

    let _ = service.cut(
        &board,
        &mut clipboard,
        &project(1),
        root(),
        &[rel("src/main.ts")],
    );
    // Finder で別のファイルをコピーした。
    board.external_write(&["/x/app/notes/a.md"]);

    let outcome = service.paste(&board, &mut clipboard, &roots, root(), &rel("src"), None);

    assert_eq!(outcome, done("src/a.md"));
    assert!(fs.exists("/x/app/src/main.ts"), "切り取ったものは動かない");
    assert!(fs.exists("/x/app/notes/a.md"), "コピーの元は残る");
    assert!(fs.exists("/x/app/src/a.md"));
    assert!(clipboard.cut_items().is_none(), "古い切り取りは捨てる");
}

#[test]
fn 切り取りの衝突を対処付きで呼び直しても移動になる() {
    let fs = sample();
    let service = FileOperationService::new(&fs);
    let board = FakePasteboard::default();
    let mut clipboard = FileClipboard::new();

    let _ = service.cut(
        &board,
        &mut clipboard,
        &project(1),
        root(),
        &[rel("notes/a.md")],
    );
    assert_eq!(
        service.paste(&board, &mut clipboard, &roots, root(), &rel("docs"), None),
        conflict("a.md")
    );
    assert!(
        clipboard.cut_items().is_some(),
        "衝突のあいだは切り取りを保つ"
    );

    let outcome = service.paste(
        &board,
        &mut clipboard,
        &roots,
        root(),
        &rel("docs"),
        Some(ConflictResolution::KeepBoth),
    );

    assert_eq!(outcome, done("docs/a 2.md"));
    assert!(!fs.exists("/x/app/notes/a.md"));
    assert!(fs.exists("/x/app/docs/a.md"));
}

#[test]
fn 別のプロジェクトへ切り取ってペーストできる() {
    let fs = sample();
    let service = FileOperationService::new(&fs);
    let board = FakePasteboard::default();
    let mut clipboard = FileClipboard::new();

    let _ = service.cut(
        &board,
        &mut clipboard,
        &project(1),
        root(),
        &[rel("src/main.ts")],
    );
    let outcome = service.paste(
        &board,
        &mut clipboard,
        &roots,
        Path::new(OTHER_ROOT),
        &RelPath::root(),
        None,
    );

    assert_eq!(outcome, done("main.ts"));
    assert!(!fs.exists("/x/app/src/main.ts"));
    assert!(fs.exists("/x/other/main.ts"));
}

#[test]
fn 切り取ったフォルダを自分の中へペーストすることは拒否する() {
    let fs = sample();
    let service = FileOperationService::new(&fs);
    let board = FakePasteboard::default();
    let mut clipboard = FileClipboard::new();

    let _ = service.cut(&board, &mut clipboard, &project(1), root(), &[rel("notes")]);
    let outcome = service.paste(&board, &mut clipboard, &roots, root(), &rel("notes"), None);

    assert_eq!(
        outcome,
        FileOpOutcome::NotAllowed {
            reason: NotAllowedReason::IntoDescendant
        }
    );
    assert!(fs.mutations().is_empty());
}

#[test]
fn フォルダの上でペーストするとその中に置く() {
    let fs = sample();
    let service = FileOperationService::new(&fs);
    let board = FakePasteboard::default();
    let mut clipboard = FileClipboard::new();

    let _ = service.copy_to_pasteboard(&board, &mut clipboard, root(), &[rel("src/main.ts")]);
    let outcome = service.paste(&board, &mut clipboard, &roots, root(), &rel("notes"), None);

    assert_eq!(outcome, done("notes/main.ts"));
    assert!(fs.exists("/x/app/notes/main.ts"));
    assert!(fs.exists("/x/app/src/main.ts"));
}

#[test]
fn ルートの外のファイルをペーストでコピーできる() {
    // Finder でコピーしたもの(ルートの外)。元は読むだけ。
    let fs = sample();
    let service = FileOperationService::new(&fs);
    let board = FakePasteboard::default();
    let mut clipboard = FileClipboard::new();
    board.external_write(&["/x/outside/secret.txt"]);

    let outcome = service.paste(&board, &mut clipboard, &roots, root(), &rel("docs"), None);

    assert_eq!(outcome, done("docs/secret.txt"));
    assert!(fs.exists("/x/outside/secret.txt"));
    assert!(fs.exists("/x/app/docs/secret.txt"));
    assert_eq!(
        fs.mutations(),
        vec![Call::Copy(
            p("/x/outside/secret.txt"),
            p("/x/app/docs/secret.txt")
        )]
    );
}

#[test]
fn 貼り付け先がルートの外ならペーストしない() {
    let fs = sample().link("/x/app/out", "/x/outside");
    let service = FileOperationService::new(&fs);
    let board = FakePasteboard::default();
    let mut clipboard = FileClipboard::new();
    board.external_write(&["/x/app/docs/a.md"]);

    let outcome = service.paste(&board, &mut clipboard, &roots, root(), &rel("out"), None);

    assert_eq!(
        outcome,
        FileOpOutcome::NotAllowed {
            reason: NotAllowedReason::OutsideRoot
        }
    );
    assert!(fs.mutations().is_empty());
}

#[test]
fn ペーストボードにファイルが無ければ何もしない() {
    let fs = sample();
    let service = FileOperationService::new(&fs);
    let board = FakePasteboard::default();
    let mut clipboard = FileClipboard::new();
    board.external_write_text();

    let outcome = service.paste(&board, &mut clipboard, &roots, root(), &rel("docs"), None);

    assert_eq!(outcome, FileOpOutcome::Done { select: None });
    assert!(fs.calls().is_empty());
}

#[test]
fn 複数のファイルのペーストは衝突が1つでもあれば何も置かずに衝突を返す() {
    let fs = sample();
    let service = FileOperationService::new(&fs);
    let board = FakePasteboard::default();
    let mut clipboard = FileClipboard::new();
    board.external_write(&["/x/outside/secret.txt", "/x/app/notes/a.md"]);

    let outcome = service.paste(&board, &mut clipboard, &roots, root(), &rel("docs"), None);

    assert_eq!(outcome, conflict("a.md"));
    assert!(fs.mutations().is_empty());

    let outcome = service.paste(
        &board,
        &mut clipboard,
        &roots,
        root(),
        &rel("docs"),
        Some(ConflictResolution::KeepBoth),
    );
    assert_eq!(outcome, done("docs/secret.txt"));
    assert!(fs.exists("/x/app/docs/secret.txt"));
    assert!(fs.exists("/x/app/docs/a 2.md"));
}

#[test]
fn ペーストのコピー元が実行の時点で無ければ見つからないを返す() {
    let fs = sample();
    let service = FileOperationService::new(&fs);
    let board = FakePasteboard::default();
    let mut clipboard = FileClipboard::new();
    board.external_write(&["/x/outside/gone.txt"]);

    let outcome = service.paste(&board, &mut clipboard, &roots, root(), &rel("docs"), None);

    assert_eq!(outcome, FileOpOutcome::NotFound);
    assert!(fs.mutations().is_empty());
}

#[test]
fn 切り取りの状態はペーストボードの変更番号で移動かコピーかを決める() {
    let board = FakePasteboard::default();
    let mut clipboard = FileClipboard::new();
    let count = board
        .write_file_urls(&[Path::new("/x/app/a")])
        .expect("書ける");
    clipboard.record_cut(project(1), vec![rel("a")], count);

    assert_eq!(
        clipboard.source(&board).expect("読める"),
        PasteSource::Move {
            project: project(1),
            items: vec![rel("a")]
        }
    );

    board.external_write(&["/x/app/b"]);
    assert_eq!(
        clipboard.source(&board).expect("読める"),
        PasteSource::Copy {
            paths: vec![p("/x/app/b")]
        }
    );
    assert!(clipboard.cut_items().is_none());

    board.external_write_text();
    assert_eq!(
        clipboard.source(&board).expect("読める"),
        PasteSource::Nothing
    );
}

// ---- 複数の項目のペーストの衝突(R1・R2) ----

#[test]
fn 複数のペーストは衝突したすべての項目を数えて返す() {
    // docs には a.md と main.ts がある。3 つのうち 2 つが衝突する。
    let fs = sample().file("/x/app/docs/main.ts");
    let service = FileOperationService::new(&fs);
    let board = FakePasteboard::default();
    let mut clipboard = FileClipboard::new();
    board.external_write(&[
        "/x/app/notes/a.md",
        "/x/outside/secret.txt",
        "/x/app/src/main.ts",
    ]);

    let outcome = service.paste(&board, &mut clipboard, &roots, root(), &rel("docs"), None);

    assert_eq!(outcome, conflicts("a.md", 2));
    assert!(fs.mutations().is_empty(), "衝突したら何も置かない");
}

#[test]
fn 単一の操作の衝突は数が1() {
    let fs = sample();
    let service = FileOperationService::new(&fs);
    let board = FakePasteboard::default();
    let mut clipboard = FileClipboard::new();
    board.external_write(&["/x/app/notes/a.md"]);

    assert_eq!(
        service.paste(&board, &mut clipboard, &roots, root(), &rel("docs"), None),
        conflicts("a.md", 1)
    );
    assert_eq!(
        service.move_entry(root(), &rel("notes/a.md"), &rel("docs"), None),
        conflicts("a.md", 1)
    );
    assert_eq!(
        service.create(root(), &rel("docs"), "a.md", NewEntryKind::File, None),
        conflicts("a.md", 1)
    );
}

#[test]
fn ペーストする項目どうしの同じ名前も事前の検査で衝突として数える() {
    // 別のフォルダの同じ名前。置く先にはどちらも無い。
    let fs = FakeFs::new()
        .file("/x/app/a/x.txt")
        .file("/x/app/b/x.txt")
        .file("/x/app/c/y.txt")
        .dir("/x/app/docs");
    let service = FileOperationService::new(&fs);
    let board = FakePasteboard::default();
    let mut clipboard = FileClipboard::new();
    board.external_write(&["/x/app/a/x.txt", "/x/app/b/x.txt", "/x/app/c/y.txt"]);

    let outcome = service.paste(&board, &mut clipboard, &roots, root(), &rel("docs"), None);

    assert_eq!(outcome, conflicts("x.txt", 1));
    assert!(
        fs.mutations().is_empty(),
        "1 件目だけ置いて 2 件目で止まらない"
    );

    // 両方残すなら、2 件目は番号付きの名前になる。
    let outcome = service.paste(
        &board,
        &mut clipboard,
        &roots,
        root(),
        &rel("docs"),
        Some(ConflictResolution::KeepBoth),
    );
    assert_eq!(outcome, done("docs/x.txt"));
    assert!(fs.exists("/x/app/docs/x.txt"));
    assert!(fs.exists("/x/app/docs/x 2.txt"));
    assert!(fs.exists("/x/app/docs/y.txt"));
    assert!(fs.trashed().is_empty());
}

#[test]
fn ペーストする項目どうしで大文字小文字だけ違う名前も衝突として数える() {
    // APFS(既定)は大文字・小文字を区別しないので、2 件目は 1 件目と衝突する。
    let fs = FakeFs::new()
        .file("/x/app/a/x.txt")
        .file("/x/app/b/X.txt")
        .dir("/x/app/docs");
    let service = FileOperationService::new(&fs);
    let board = FakePasteboard::default();
    let mut clipboard = FileClipboard::new();
    board.external_write(&["/x/app/a/x.txt", "/x/app/b/X.txt"]);

    let outcome = service.paste(&board, &mut clipboard, &roots, root(), &rel("docs"), None);

    assert_eq!(outcome, conflicts("X.txt", 1));
    assert!(fs.mutations().is_empty());
}

#[test]
fn ペーストする項目どうしの同じ名前を置き換えにすると何も変えずに失敗する() {
    // 置き換えると、同じペーストで置いた 1 件目をゴミ箱に入れることになる。
    let fs = FakeFs::new()
        .file("/x/app/a/x.txt")
        .file("/x/app/b/x.txt")
        .dir("/x/app/docs");
    let service = FileOperationService::new(&fs);
    let board = FakePasteboard::default();
    let mut clipboard = FileClipboard::new();
    board.external_write(&["/x/app/a/x.txt", "/x/app/b/x.txt"]);

    let outcome = service.paste(
        &board,
        &mut clipboard,
        &roots,
        root(),
        &rel("docs"),
        Some(ConflictResolution::Replace),
    );

    assert!(failed_message(&outcome).contains("同じ名前"), "{outcome:?}");
    assert_no_absolute(&outcome);
    assert!(fs.mutations().is_empty());
}

#[test]
fn 同じフォルダへの切り取りのペーストは衝突に数えない() {
    // docs/a.md を docs に戻すのは何もしない操作。src/main.ts だけが動く。
    let fs = sample();
    let service = FileOperationService::new(&fs);
    let board = FakePasteboard::default();
    let mut clipboard = FileClipboard::new();
    let _ = service.cut(
        &board,
        &mut clipboard,
        &project(1),
        root(),
        &[rel("docs/a.md"), rel("src/main.ts")],
    );

    let outcome = service.paste(&board, &mut clipboard, &roots, root(), &rel("docs"), None);

    assert_eq!(outcome, done("docs/a.md"));
    assert!(fs.exists("/x/app/docs/main.ts"));
    assert!(fs.trashed().is_empty());
}

#[test]
fn 事前の検査をすり抜けた衝突のあと切り取りの呼び直しは移し終えた項目を飛ばす() {
    // 1 件目を移した直後に、外から docs/y.txt が作られる(事前の検査のあとの衝突)。
    let fs = FakeFs::new()
        .file("/x/app/a/x.txt")
        .file("/x/app/c/y.txt")
        .dir("/x/app/docs")
        .after_call_file(
            Call::Move(p("/x/app/a/x.txt"), p("/x/app/docs/x.txt")),
            "/x/app/docs/y.txt",
        );
    let service = FileOperationService::new(&fs);
    let board = FakePasteboard::default();
    let mut clipboard = FileClipboard::new();
    let _ = service.cut(
        &board,
        &mut clipboard,
        &project(1),
        root(),
        &[rel("a/x.txt"), rel("c/y.txt")],
    );

    let outcome = service.paste(&board, &mut clipboard, &roots, root(), &rel("docs"), None);

    assert_eq!(outcome, conflicts("y.txt", 1));
    assert!(fs.exists("/x/app/docs/x.txt"), "1 件目は移したまま");
    assert_eq!(
        clipboard.cut_items().map(|c| c.items.clone()),
        Some(vec![rel("c/y.txt")]),
        "移し終えた項目は切り取りの状態から外す"
    );

    let outcome = service.paste(
        &board,
        &mut clipboard,
        &roots,
        root(),
        &rel("docs"),
        Some(ConflictResolution::KeepBoth),
    );

    assert_eq!(outcome, done("docs/y 2.txt"));
    assert!(!fs.exists("/x/app/c/y.txt"));
    assert!(
        fs.exists("/x/app/docs/y.txt"),
        "外から作られた y.txt は残る"
    );
    assert!(
        clipboard.cut_items().is_none(),
        "移し終えたら切り取りを消す"
    );
}

#[test]
fn 事前の検査をすり抜けた衝突のあとコピーの呼び直しは写し終えた項目を写し直さない() {
    let fs = sample()
        .file("/x/outside/p.txt")
        .file("/x/outside/q.txt")
        .after_call_file(
            Call::Copy(p("/x/outside/p.txt"), p("/x/app/docs/p.txt")),
            "/x/app/docs/q.txt",
        );
    let service = FileOperationService::new(&fs);
    let board = FakePasteboard::default();
    let mut clipboard = FileClipboard::new();
    board.external_write(&["/x/outside/p.txt", "/x/outside/q.txt"]);

    let outcome = service.paste(&board, &mut clipboard, &roots, root(), &rel("docs"), None);
    assert_eq!(outcome, conflicts("q.txt", 1));
    assert!(fs.exists("/x/app/docs/p.txt"));

    let outcome = service.paste(
        &board,
        &mut clipboard,
        &roots,
        root(),
        &rel("docs"),
        Some(ConflictResolution::KeepBoth),
    );

    assert_eq!(outcome, done("docs/q 2.txt"));
    assert!(
        !fs.exists("/x/app/docs/p 2.txt"),
        "写し終えた項目を写し直した"
    );
    let copies: Vec<Call> = fs
        .mutations()
        .into_iter()
        .filter(|c| matches!(c, Call::Copy(from, _) if from == &p("/x/outside/p.txt")))
        .collect();
    assert_eq!(copies.len(), 1, "p.txt を 2 回写した: {copies:?}");
}

#[test]
fn 途中の衝突のあと対処なしで新しくペーストすると最初から数え直す() {
    // 確認を中止して(呼び直さずに)、もう一度ペーストした。写し終えた項目も衝突として数える。
    let fs = sample()
        .file("/x/outside/p.txt")
        .file("/x/outside/q.txt")
        .after_call_file(
            Call::Copy(p("/x/outside/p.txt"), p("/x/app/docs/p.txt")),
            "/x/app/docs/q.txt",
        );
    let service = FileOperationService::new(&fs);
    let board = FakePasteboard::default();
    let mut clipboard = FileClipboard::new();
    board.external_write(&["/x/outside/p.txt", "/x/outside/q.txt"]);

    let _ = service.paste(&board, &mut clipboard, &roots, root(), &rel("docs"), None);
    let outcome = service.paste(&board, &mut clipboard, &roots, root(), &rel("docs"), None);

    assert_eq!(outcome, conflicts("p.txt", 2));
}

#[test]
fn ペーストのあいだに新しく切り取ったものは移し終えても消さない() {
    // ロックの外で移している間に、別の切り取りが記録された(R6)。
    let fs = sample();
    let service = FileOperationService::new(&fs);
    let board = FakePasteboard::default();
    let mut clipboard = FileClipboard::new();
    let _ = service.cut(
        &board,
        &mut clipboard,
        &project(1),
        root(),
        &[rel("src/main.ts")],
    );

    let plan = match service.plan_paste(&board, &mut clipboard, None) {
        Ok(plan) => plan,
        Err(outcome) => panic!("計画を立てられなかった: {outcome:?}"),
    };
    // 移している間に、別の項目を切り取った。
    let _ = service.cut(
        &board,
        &mut clipboard,
        &project(1),
        root(),
        &[rel("notes/a.md")],
    );
    let report = service.run_paste(&plan, &roots, root(), &rel("docs"), None);
    clipboard.settle(&plan, &report);

    assert_eq!(report.outcome, done("docs/main.ts"));
    assert_eq!(
        clipboard.cut_items().map(|c| c.items.clone()),
        Some(vec![rel("notes/a.md")]),
        "あとから切り取ったものを消した"
    );
}

// ---- 置き換えで、置けないと分かっている移動の前に既存をゴミ箱に入れない(R3) ----

#[test]
fn 別のボリュームへの切り取りのペーストは置き換えでも既存をゴミ箱に入れずに失敗する() {
    let fs = sample().volume("/x/other").file("/x/other/main.ts");
    let service = FileOperationService::new(&fs);
    let board = FakePasteboard::default();
    let mut clipboard = FileClipboard::new();
    let _ = service.cut(
        &board,
        &mut clipboard,
        &project(1),
        root(),
        &[rel("src/main.ts")],
    );

    for resolution in [None, Some(ConflictResolution::Replace)] {
        let outcome = service.paste(
            &board,
            &mut clipboard,
            &roots,
            Path::new(OTHER_ROOT),
            &RelPath::root(),
            resolution,
        );
        assert_eq!(
            outcome,
            FileOpOutcome::Failed {
                message: "別のディスクへは移動できません".to_owned()
            },
            "{resolution:?}"
        );
    }
    assert!(fs.trashed().is_empty(), "置けない移動の前にゴミ箱に入れた");
    assert!(fs.exists("/x/app/src/main.ts"));
    assert!(fs.exists("/x/other/main.ts"));
}

#[test]
fn ルートの中の別のボリュームへの移動は置き換えでも既存をゴミ箱に入れない() {
    let fs = sample().volume("/x/app/mnt").file("/x/app/mnt/main.ts");
    let service = FileOperationService::new(&fs);

    let outcome = service.move_entry(
        root(),
        &rel("src/main.ts"),
        &rel("mnt"),
        Some(ConflictResolution::Replace),
    );

    assert!(
        failed_message(&outcome).contains("別のディスク"),
        "{outcome:?}"
    );
    assert!(fs.trashed().is_empty());
    assert!(fs.exists("/x/app/src/main.ts"));
}

#[test]
fn 別のボリュームでもコピーの置き換えはできる() {
    let fs = sample().volume("/x/other").file("/x/other/main.ts");
    let service = FileOperationService::new(&fs);
    let board = FakePasteboard::default();
    let mut clipboard = FileClipboard::new();
    let _ = service.copy_to_pasteboard(&board, &mut clipboard, root(), &[rel("src/main.ts")]);

    let outcome = service.paste(
        &board,
        &mut clipboard,
        &roots,
        Path::new(OTHER_ROOT),
        &RelPath::root(),
        Some(ConflictResolution::Replace),
    );

    assert_eq!(outcome, done("main.ts"));
    assert_eq!(fs.trashed(), vec![p("/x/other/main.ts")]);
}

// ---- その他の種類はコピーしない(R6) ----

#[test]
fn ペーストの元にその他の種類があれば何も写さずに失敗する() {
    // 壊れたリンクは偽のファイルシステムでの「その他」(FIFO・ソケット・デバイスと同じ扱い)。
    let fs = sample().link("/x/outside/pipe", "/x/nowhere");
    let service = FileOperationService::new(&fs);
    let board = FakePasteboard::default();
    let mut clipboard = FileClipboard::new();
    board.external_write(&["/x/outside/secret.txt", "/x/outside/pipe"]);

    let outcome = service.paste(&board, &mut clipboard, &roots, root(), &rel("docs"), None);

    let message = failed_message(&outcome);
    assert!(message.contains("pipe"), "どの項目か分かる: {message}");
    assert!(message.contains("コピーできない"), "{message}");
    assert_no_absolute(&outcome);
    assert!(fs.mutations().is_empty());
}

#[test]
fn ツリーの中のその他の種類はコピーしないが移動はできる() {
    let fs = sample().link("/x/app/src/pipe", "/x/nowhere");
    let service = FileOperationService::new(&fs);

    let outcome = service.copy_entry(root(), &rel("src/pipe"), &rel("docs"), None);
    assert!(
        failed_message(&outcome).contains("コピーできない"),
        "{outcome:?}"
    );
    assert!(fs.mutations().is_empty());

    assert_eq!(
        service.move_entry(root(), &rel("src/pipe"), &rel("docs"), None),
        done("docs/pipe")
    );
}
