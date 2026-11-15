//! 相対パス・絶対パス・挿入の文字列の整形(設計書 §5.1 の `PathFormatter`、§7.1「パスをターミナルへ」、
//! §15.2 #6、計画書 T38b、【仮定】16・17)。

use std::path::{Path, PathBuf};

use arin_core::paths::format::{
    PathFormatError, absolute_path, absolute_text, insertion_path_kind, insertion_text, path_text,
    relative_text,
};
use arin_core::paths::{PathKind, RelPath};
use arin_core::project::ProjectId;

const ROOT: &str = "/Users/x/dev/app";

fn rel(text: &str) -> RelPath {
    RelPath::parse(text).unwrap_or_else(|e| panic!("{text:?} を読めなかった: {e}"))
}

fn root() -> &'static Path {
    Path::new(ROOT)
}

fn project(text: &str) -> ProjectId {
    ProjectId::parse(text).unwrap_or_else(|e| panic!("{text:?} を ID にできなかった: {e}"))
}

const PROJECT_A: &str = "00000000-0000-4000-8000-00000000000a";
const PROJECT_B: &str = "00000000-0000-4000-8000-00000000000b";

/// 挿入の文字列に、PTY に送ると行の確定や制御になるバイト(C0 と DEL)が無いこと。
fn assert_no_control_bytes(text: &str) {
    assert!(
        text.bytes().all(|b| b >= 0x20 && b != 0x7f),
        "制御文字のバイトが残っている: {text:?}"
    );
}

// --- 相対パス ---

#[test]
fn 相対パスはルート基準でスラッシュ区切り() {
    assert_eq!(relative_text(&rel("src/a b.ts")), "src/a b.ts");
    assert_eq!(relative_text(&rel("README.md")), "README.md");
    // フォルダでも末尾に `/` を付けず、先頭に `./` を付けない。
    assert_eq!(relative_text(&rel("src/ui")), "src/ui");
    // ルート自体は空の文字列。
    assert_eq!(relative_text(&RelPath::root()), "");
}

#[test]
fn パスのコピーの相対パスはエスケープしない() {
    // AC-T03: クリップボードは `src/a b.ts`(エスケープも末尾の空白も無い)。
    assert_eq!(
        path_text(root(), &rel("src/a b.ts"), PathKind::Relative),
        Ok("src/a b.ts".to_owned())
    );
}

// --- 絶対パス ---

#[test]
fn 絶対パスはルートと相対パスをつなぐ() {
    assert_eq!(
        absolute_path(root(), &rel("src/a b.ts")),
        Ok(PathBuf::from("/Users/x/dev/app/src/a b.ts"))
    );
    assert_eq!(
        absolute_text(root(), &rel("src/a b.ts")),
        Ok("/Users/x/dev/app/src/a b.ts".to_owned())
    );
    assert_eq!(
        path_text(root(), &rel("src/a b.ts"), PathKind::Absolute),
        Ok("/Users/x/dev/app/src/a b.ts".to_owned())
    );
}

#[test]
fn ルート自体の絶対パスはルートのまま() {
    assert_eq!(absolute_text(root(), &RelPath::root()), Ok(ROOT.to_owned()));
    // 末尾の `/` や重なった `/` は付けない形にそろえる。
    assert_eq!(
        absolute_text(Path::new("/Users/x/dev/app/"), &RelPath::root()),
        Ok(ROOT.to_owned())
    );
    assert_eq!(
        absolute_text(Path::new("/Users/x//dev/app/"), &rel("src")),
        Ok("/Users/x/dev/app/src".to_owned())
    );
    // ルートが `/` そのものでも二重にならない。
    assert_eq!(
        absolute_text(Path::new("/"), &rel("tmp/a")),
        Ok("/tmp/a".to_owned())
    );
    assert_eq!(
        absolute_text(Path::new("/"), &RelPath::root()),
        Ok("/".to_owned())
    );
}

#[test]
fn ルートが絶対パスでなければ失敗する() {
    for bad in ["", "Users/x/dev/app", "./app"] {
        assert_eq!(
            absolute_text(Path::new(bad), &rel("a")),
            Err(PathFormatError::RootNotAbsolute(PathBuf::from(bad))),
            "{bad:?}"
        );
    }
}

#[test]
fn ルートに_dot_dot_の要素があれば失敗する() {
    assert_eq!(
        absolute_text(Path::new("/Users/x/../y"), &rel("a")),
        Err(PathFormatError::RootNotNormal(PathBuf::from(
            "/Users/x/../y"
        )))
    );
}

#[test]
fn ルートが_utf_8_でなければ文字列にできない() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let root = Path::new(OsStr::from_bytes(b"/Users/x/\xff"));
    assert!(absolute_path(root, &rel("a")).is_ok());
    assert_eq!(
        absolute_text(root, &rel("a")),
        Err(PathFormatError::NotUtf8(PathBuf::from(OsStr::from_bytes(
            b"/Users/x/\xff/a"
        ))))
    );
}

// --- 挿入の文字列 ---

#[test]
fn 空白はバックスラッシュでエスケープし末尾に半角スペースを付ける() {
    // AC-T04: `src/a\ b.ts `(末尾に半角スペース)。
    assert_eq!(
        insertion_text(root(), &[rel("src/a b.ts")], PathKind::Relative),
        Ok(r"src/a\ b.ts ".to_owned())
    );
}

#[test]
fn 挿入の文字列は_enter_を含まない() {
    for name in [
        "src/a b.ts",
        "a\nb",
        "a\r\nb",
        "a\rb",
        "x\n\nrm -rf ~\n",
        "$(reboot)\n",
        "\u{1b}[200~evil\u{1b}[201~",
    ] {
        for kind in [PathKind::Relative, PathKind::Absolute] {
            let text = insertion_text(root(), &[rel(name)], kind)
                .unwrap_or_else(|e| panic!("{name:?} を整形できなかった: {e}"));
            assert!(
                !text.contains('\n') && !text.contains('\r'),
                "改行のバイトが入っている: {text:?}"
            );
            assert_no_control_bytes(&text);
            assert!(text.ends_with(' '), "末尾が半角スペースでない: {text:?}");
            assert!(!text.ends_with("  "), "末尾の空白が 2 つ以上: {text:?}");
        }
    }
}

#[test]
fn 改行を含む名前の挿入はドル引用符で包む() {
    assert_eq!(
        insertion_text(root(), &[rel("src/a\nb.ts")], PathKind::Relative),
        Ok(r"$'src/a\x0ab.ts' ".to_owned())
    );
    assert_eq!(
        insertion_text(root(), &[rel("src/a\nb.ts")], PathKind::Absolute),
        Ok(r"$'/Users/x/dev/app/src/a\x0ab.ts' ".to_owned())
    );
}

#[test]
fn 絶対パスの挿入はルートの特殊文字もエスケープする() {
    let root = Path::new("/Users/x/my $dev (old)");
    assert_eq!(
        insertion_text(root, &[rel("a b")], PathKind::Absolute),
        Ok(r"/Users/x/my\ \$dev\ \(old\)/a\ b ".to_owned())
    );
}

#[test]
fn ハイフンで始まる相対パスの挿入は_dot_slash_を付ける() {
    // コマンドのオプションとして読まれないように(【仮定】)。
    assert_eq!(
        insertion_text(root(), &[rel("-rf")], PathKind::Relative),
        Ok("./-rf ".to_owned())
    );
    assert_eq!(
        insertion_text(root(), &[rel("--help me/a")], PathKind::Relative),
        Ok(r"./--help\ me/a ".to_owned())
    );
    // 途中のハイフンは何もしない。
    assert_eq!(
        insertion_text(root(), &[rel("src/-a")], PathKind::Relative),
        Ok("src/-a ".to_owned())
    );
    // コピーには付けない。
    assert_eq!(
        path_text(root(), &rel("-rf"), PathKind::Relative),
        Ok("-rf".to_owned())
    );
}

#[test]
fn ルート自体の相対パスの挿入はドットにする() {
    assert_eq!(
        insertion_text(root(), &[RelPath::root()], PathKind::Relative),
        Ok(". ".to_owned())
    );
    assert_eq!(
        insertion_text(root(), &[RelPath::root()], PathKind::Absolute),
        Ok("/Users/x/dev/app ".to_owned())
    );
}

#[test]
fn 複数のパスは空白区切りでつなぎ末尾に_1_つだけ空白を付ける() {
    assert_eq!(
        insertion_text(
            root(),
            &[rel("a b"), rel("c"), rel("d\ne")],
            PathKind::Relative
        ),
        Ok(r"a\ b c $'d\x0ae' ".to_owned())
    );
}

#[test]
fn 挿入するパスが無ければ失敗する() {
    assert_eq!(
        insertion_text(root(), &[], PathKind::Relative),
        Err(PathFormatError::NoPaths)
    );
}

#[test]
fn 日本語と絵文字はそのまま() {
    assert_eq!(
        insertion_text(root(), &[rel("資料/📝メモ.md")], PathKind::Relative),
        Ok("資料/📝メモ.md ".to_owned())
    );
    assert_eq!(
        path_text(root(), &rel("資料/📝 メモ.md"), PathKind::Absolute),
        Ok("/Users/x/dev/app/資料/📝 メモ.md".to_owned())
    );
}

// --- 挿入先(§15.2 #6 の A) ---

#[test]
fn 同じプロジェクトのターミナルへは相対パスにする() {
    assert_eq!(
        insertion_path_kind(&project(PROJECT_A), &project(PROJECT_A)),
        PathKind::Relative
    );
}

#[test]
fn 別のプロジェクトのターミナルへは絶対パスにする() {
    let kind = insertion_path_kind(&project(PROJECT_A), &project(PROJECT_B));
    assert_eq!(kind, PathKind::Absolute);
    assert_eq!(
        insertion_text(root(), &[rel("src/a b.ts")], kind),
        Ok(r"/Users/x/dev/app/src/a\ b.ts ".to_owned())
    );
}
