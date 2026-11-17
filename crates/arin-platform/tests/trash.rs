//! ゴミ箱の結合テスト(`LocalFileSystem::trash`、計画書 T40c)。FR-028、ADR-007 の (a)、計画書 M3 の【仮定】23。
//!
//! - 毎回動くテストは、OS に頼む前に決まる場合(相対パス・`..` を含むパス・無いパス)だけ。ゴミ箱には何も入らない
//! - 本物のゴミ箱を使うテストは `#[ignore]`。`cargo test -p arin-platform --test trash -- --ignored --nocapture`
//!   と明示したときだけ動く。一時フォルダに作った自分の項目(`arin-test-trash-<pid>-<連番>…`)だけを入れ、
//!   行き先を標準エラーに `[arin:test-trash]` で出す
//! - ゴミ箱の中のものは消さない(`~/.Trash` はシステムが保護していて、ほかの項目を巻き込む危険もある)。
//!   入れた項目は、出した名前を見て人が片付ける
//! - 一時フォルダは `std::env::temp_dir()` の下に作り、抜けるときに自分が作ったフォルダだけを消す

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use arin_core::ports::fs::{FileSystemPort, FsError};
use arin_platform::fs::LocalFileSystem;

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// システムの一時領域の実パス。
fn temp_base() -> PathBuf {
    fs::canonicalize(std::env::temp_dir()).expect("一時領域の実パスを取れなかった")
}

/// このテストだけが使う一時フォルダ。抜けるときに自分が作ったフォルダだけを消す(ゴミ箱には触れない)。
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = temp_base().join(format!(
            "arin-platform-trash-{name}-{}-{n}",
            std::process::id()
        ));
        assert!(
            dir.starts_with(temp_base()) && dir != temp_base(),
            "一時領域の外を使おうとした"
        );
        fs::create_dir(&dir).expect("一時フォルダを作れなかった");
        Self(dir)
    }

    /// 一時フォルダの中のパス。名前は 1 つの要素だけ(外に出ない)。
    fn at(&self, name: &str) -> PathBuf {
        let p = self.0.join(name);
        assert!(
            p.parent() == Some(self.0.as_path())
                && p.components().all(|c| !matches!(c, Component::ParentDir)),
            "テストのフォルダの外を使おうとした"
        );
        p
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// ゴミ箱に入れる項目の名前。ゴミ箱の中で見分けられるよう、プロセス ID と連番を含める。
fn unique_name(suffix: &str) -> String {
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    format!("arin-test-trash-{}-{n}{suffix}", std::process::id())
}

/// 本物のゴミ箱に入れ、入れた項目と行き先を標準エラーに出す(人が片付けるため)。
fn trash_and_report(path: &Path) -> Result<Option<PathBuf>, FsError> {
    let result = LocalFileSystem::new().trash(path);
    match &result {
        Ok(Some(dest)) => eprintln!(
            "[arin:test-trash] ゴミ箱に入れた: {} → {}",
            path.display(),
            dest.display()
        ),
        Ok(None) => eprintln!(
            "[arin:test-trash] ゴミ箱に入れた(行き先は不明): {}",
            path.display()
        ),
        Err(e) => eprintln!(
            "[arin:test-trash] 入れられなかった: {}: {e}",
            path.display()
        ),
    }
    result
}

/// 行き先がホームのゴミ箱(`…/.Trash/…`)の下で、名前が `name` であることを確かめる。
/// 名前はバイト列で比べる(分解した形=NFD に変わっていないこと)。
fn assert_in_trash(dest: &Path, name: &str) {
    assert!(
        dest.components()
            .any(|c| c == Component::Normal(".Trash".as_ref())),
        "行き先がゴミ箱の下にない: {}",
        dest.display()
    );
    let got = dest
        .file_name()
        .and_then(|n| n.to_str())
        .expect("行き先に名前が無い");
    assert_eq!(
        got.as_bytes(),
        name.as_bytes(),
        "ゴミ箱の中の名前が元と違う(NFD に変わった?): {got:?}"
    );
    // ゴミ箱の中を調べられるかは環境による(`~/.Trash` はプライバシーの保護の対象)。調べられたときだけ見る
    match fs::symlink_metadata(dest) {
        Ok(_) => eprintln!("[arin:test-trash] 行き先に項目がある: {}", dest.display()),
        Err(e) => eprintln!(
            "[arin:test-trash] 行き先を調べられなかった(保護のため。確かめは人が行う): {}: {e}",
            dest.display()
        ),
    }
}

// ---- 毎回動く(OS に頼む前に決まる) ----

#[test]
fn 相対パスはゴミ箱に入れず失敗する() {
    // 在っても入れないことを示すため、実在しない名前にする(作業ディレクトリのファイルを巻き込まない)
    let path = Path::new("arin-test-trash-relative-does-not-exist.txt");
    assert!(matches!(
        LocalFileSystem::new().trash(path),
        Err(FsError::Io(_))
    ));
}

#[test]
fn 親へ戻る要素を含むパスはゴミ箱に入れず失敗する() {
    let tmp = TempDir::new("parent");
    let path = tmp.at("sub").join("..").join("does-not-exist.txt");
    assert!(matches!(
        LocalFileSystem::new().trash(&path),
        Err(FsError::Io(_))
    ));
}

#[test]
fn 無いものをゴミ箱に入れると見つからないを返す() {
    let tmp = TempDir::new("missing");
    let path = tmp.at("does-not-exist.txt");
    assert!(matches!(
        LocalFileSystem::new().trash(&path),
        Err(FsError::NotFound(_))
    ));
    assert!(fs::symlink_metadata(&path).is_err());
}

// ---- 本物のゴミ箱(`#[ignore]`。明示したときだけ) ----

#[test]
#[ignore = "本物のゴミ箱に項目が入る。cargo test -p arin-platform --test trash -- --ignored --nocapture で明示して動かす"]
fn ゴミ箱に入れると元の場所から消えゴミ箱の中の場所が元の名前のまま返る() {
    // 濁点を含む日本語の名前(NFC)。`fileURLWithPath` を使うと NFD に変わる
    let tmp = TempDir::new("file");
    let name = unique_name("-ダミー.txt");
    let path = tmp.at(&name);
    fs::write(&path, "a").expect("ファイルを書けなかった");

    let dest = trash_and_report(&path)
        .expect("ゴミ箱に入れられなかった")
        .expect("ゴミ箱の中の場所が返らなかった");

    assert!(fs::symlink_metadata(&path).is_err(), "元の場所に残っている");
    assert_in_trash(&dest, &name);
}

#[test]
#[ignore = "本物のゴミ箱に項目が入る。cargo test -p arin-platform --test trash -- --ignored --nocapture で明示して動かす"]
fn フォルダも中身ごとゴミ箱に入る() {
    let tmp = TempDir::new("dir");
    let name = unique_name("-dir");
    let dir = tmp.at(&name);
    fs::create_dir(&dir).expect("フォルダを作れなかった");
    fs::write(dir.join("inner.txt"), "i").expect("ファイルを書けなかった");

    let dest = trash_and_report(&dir)
        .expect("ゴミ箱に入れられなかった")
        .expect("ゴミ箱の中の場所が返らなかった");

    assert!(fs::symlink_metadata(&dir).is_err(), "元の場所に残っている");
    assert_in_trash(&dest, &name);
}

#[test]
#[ignore = "本物のゴミ箱に項目が入る。cargo test -p arin-platform --test trash -- --ignored --nocapture で明示して動かす"]
fn シンボリックリンクはリンク自体がゴミ箱に入り先は残る() {
    // 先はフォルダ(末尾の `/` で先をたどる誤りを見つけるため)。末尾に `/` を付けたパスで渡す
    let tmp = TempDir::new("link");
    let target = tmp.at("target-dir");
    fs::create_dir(&target).expect("フォルダを作れなかった");
    fs::write(target.join("keep.txt"), "k").expect("ファイルを書けなかった");
    let name = unique_name("-link");
    let link = tmp.at(&name);
    symlink(&target, &link).expect("リンクを作れなかった");
    let with_slash = PathBuf::from(format!("{}/", link.display()));

    let dest = trash_and_report(&with_slash)
        .expect("ゴミ箱に入れられなかった")
        .expect("ゴミ箱の中の場所が返らなかった");

    assert!(
        fs::symlink_metadata(&link).is_err(),
        "リンクが元の場所に残っている"
    );
    assert_eq!(
        fs::read_to_string(target.join("keep.txt")).expect("先の中身が無くなった"),
        "k"
    );
    assert_in_trash(&dest, &name);
}
