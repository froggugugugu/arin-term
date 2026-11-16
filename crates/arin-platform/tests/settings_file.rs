//! 設定ファイルの読み込みと TOML の解析(`arin_platform::storage::FsSettingsFile`・`parse_settings_toml`、T65a)。
//!
//! システムの一時領域の下に、このテスト専用のフォルダを作って(`canonicalize` した実パスで)その中だけで行う。
//! 本物の `~/Library/Application Support/Arin/` は使わない。

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use arin_core::ports::settings_file::{
    MAX_SETTINGS_FILE_BYTES, SETTINGS_FILE_NAME, SampleCreation, SettingsFileError,
    SettingsFilePort, SettingsFileRead,
};
use arin_core::settings::{
    SectionBody, SettingEntry, SettingValue, SettingsDocument, SettingsLoad, SettingsSection,
    SettingsUnreadable, TextPosition,
};
use arin_platform::storage::{FsSettingsFile, load_settings_file, parse_settings_toml};

/// このテストだけが使う一時フォルダ。抜けるときに自分が作ったフォルダだけを消す。
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("時計が 1970 年より前")
            .as_nanos();
        let base = std::env::temp_dir()
            .canonicalize()
            .expect("一時領域の実パスを得られなかった");
        let dir = base.join(format!(
            "arin-platform-test-settings-{name}-{}-{nanos}",
            std::process::id()
        ));
        assert!(dir.starts_with(&base), "一時領域の外を使おうとした");
        fs::create_dir(&dir).expect("一時フォルダを作れなかった");
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn settings_path(&self) -> PathBuf {
        self.0.join(SETTINGS_FILE_NAME)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        // 権限を落としたファイルがあっても消せるように戻してから消す。
        let _ = fs::set_permissions(
            self.0.join(SETTINGS_FILE_NAME),
            fs::Permissions::from_mode(0o600),
        );
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn mode_of(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

/// 中身と更新日時(秒とナノ秒)。
fn snapshot(path: &Path) -> (Vec<u8>, i64, i64) {
    let metadata = fs::metadata(path).unwrap();
    (
        fs::read(path).unwrap(),
        metadata.mtime(),
        metadata.mtime_nsec(),
    )
}

fn text(key: &str, value: &str) -> SettingEntry {
    SettingEntry {
        key: key.to_owned(),
        value: SettingValue::Text(value.to_owned()),
    }
}

fn not_text(key: &str) -> SettingEntry {
    SettingEntry {
        key: key.to_owned(),
        value: SettingValue::NotText,
    }
}

fn shortcuts(entries: Vec<SettingEntry>) -> SettingsDocument {
    SettingsDocument {
        sections: vec![SettingsSection {
            name: "shortcuts".to_owned(),
            body: SectionBody::Table(entries),
        }],
    }
}

// ---- 読む ----

#[test]
fn 無いときは無いと返す() {
    let dir = TempDir::new("missing");
    let file = FsSettingsFile::new(dir.path().to_path_buf());

    assert_eq!(file.read(), Ok(SettingsFileRead::Missing));
    assert_eq!(load_settings_file(&file), SettingsLoad::Missing);
    assert!(!dir.settings_path().exists(), "読むだけでファイルを作った");

    // 状態フォルダ自体が無くても「無い」で、フォルダを作らない。
    let absent = FsSettingsFile::new(dir.path().join("Arin"));
    assert_eq!(absent.read(), Ok(SettingsFileRead::Missing));
    assert!(
        !dir.path().join("Arin").exists(),
        "読むだけでフォルダを作った"
    );
}

#[test]
fn 読んでも中身と更新日時が変わらない() {
    let dir = TempDir::new("unchanged");
    let contents = b"[shortcuts]\n\"terminal.new-tab\" = \"Cmd+T\"\n";
    fs::write(dir.settings_path(), contents).unwrap();
    let before = snapshot(&dir.settings_path());
    let file = FsSettingsFile::new(dir.path().to_path_buf());

    assert_eq!(
        file.read(),
        Ok(SettingsFileRead::Contents(contents.to_vec()))
    );
    assert_eq!(
        load_settings_file(&file),
        SettingsLoad::Loaded(shortcuts(vec![text("terminal.new-tab", "Cmd+T")]))
    );
    assert_eq!(snapshot(&dir.settings_path()), before);
}

#[test]
fn シンボリックリンクの先を読む() {
    let dir = TempDir::new("symlink-read");
    let target = dir.path().join("real-settings.toml");
    fs::write(&target, b"[shortcuts]\n").unwrap();
    symlink(&target, dir.settings_path()).unwrap();
    let file = FsSettingsFile::new(dir.path().to_path_buf());

    assert_eq!(
        file.read(),
        Ok(SettingsFileRead::Contents(b"[shortcuts]\n".to_vec()))
    );
}

#[test]
fn フォルダならファイルでないと返す() {
    let dir = TempDir::new("directory");
    fs::create_dir(dir.settings_path()).unwrap();
    let file = FsSettingsFile::new(dir.path().to_path_buf());

    assert_eq!(file.read(), Err(SettingsFileError::NotAFile));
    assert_eq!(
        load_settings_file(&file),
        SettingsLoad::Unreadable(SettingsUnreadable::NotAFile)
    );
}

#[test]
fn _1_mib_を超えると中身を読まずに大きすぎると返す() {
    let dir = TempDir::new("too-large");
    // 中身を書かずに大きさだけを伸ばす(疎なファイル)。
    let big = fs::File::create(dir.settings_path()).unwrap();
    big.set_len(MAX_SETTINGS_FILE_BYTES + 1).unwrap();
    drop(big);
    let file = FsSettingsFile::new(dir.path().to_path_buf());

    assert_eq!(file.read(), Err(SettingsFileError::TooLarge));

    // ちょうど 1 MiB は読む(上限は「超える」で判定する)。
    fs::File::options()
        .write(true)
        .open(dir.settings_path())
        .unwrap()
        .set_len(MAX_SETTINGS_FILE_BYTES)
        .unwrap();
    match file.read() {
        Ok(SettingsFileRead::Contents(bytes)) => {
            assert_eq!(bytes.len() as u64, MAX_SETTINGS_FILE_BYTES);
        }
        other => panic!("1 MiB ちょうどは読めるはず: {other:?}"),
    }
}

#[test]
fn utf_8_でないと返す() {
    let dir = TempDir::new("not-utf8");
    fs::write(dir.settings_path(), b"[shortcuts]\n\xff\xfe = \"x\"\n").unwrap();
    let file = FsSettingsFile::new(dir.path().to_path_buf());

    assert_eq!(
        load_settings_file(&file),
        SettingsLoad::Unreadable(SettingsUnreadable::NotUtf8)
    );
}

#[test]
fn 読む権限が無いと権限が無いと返す() {
    let dir = TempDir::new("no-permission");
    fs::write(dir.settings_path(), b"[shortcuts]\n").unwrap();
    fs::set_permissions(dir.settings_path(), fs::Permissions::from_mode(0o000)).unwrap();
    let file = FsSettingsFile::new(dir.path().to_path_buf());

    assert_eq!(file.read(), Err(SettingsFileError::PermissionDenied));
    assert_eq!(
        load_settings_file(&file),
        SettingsLoad::Unreadable(SettingsUnreadable::PermissionDenied)
    );
}

// ---- TOML の解析 ----

#[test]
fn shortcuts_の文字列の行を書いた順に返す() {
    let toml = "[shortcuts]\n\
                zeta = \"Cmd+Z\"\n\
                alpha = \"Cmd+A\"\n\
                \"terminal.new-tab\" = \"Cmd+T\"\n\
                middle = \"Cmd+M\"\n";

    assert_eq!(
        parse_settings_toml(toml),
        Ok(shortcuts(vec![
            text("zeta", "Cmd+Z"),
            text("alpha", "Cmd+A"),
            text("terminal.new-tab", "Cmd+T"),
            text("middle", "Cmd+M"),
        ]))
    );
}

#[test]
fn 引用符の無い点つきの_id_を_1_段つないで読む() {
    let toml = "[shortcuts]\n\
                view.zoom = \"Cmd+Shift+Z\"\n\
                terminal.new-tab = \"Cmd+T\"\n\
                terminal.close-tab = \"Cmd+W\"\n\
                app.quit = \"Cmd+Q\"\n";

    assert_eq!(
        parse_settings_toml(toml),
        Ok(shortcuts(vec![
            text("view.zoom", "Cmd+Shift+Z"),
            text("terminal.new-tab", "Cmd+T"),
            text("terminal.close-tab", "Cmd+W"),
            text("app.quit", "Cmd+Q"),
        ]))
    );
}

#[test]
fn shortcuts_terminal_の節の行も同じ_id_で読む() {
    let toml = "[shortcuts]\n\
                app.quit = \"Cmd+Q\"\n\
                \n\
                [shortcuts.terminal]\n\
                new-tab = \"Cmd+T\"\n\
                close-tab = \"Cmd+W\"\n";

    assert_eq!(
        parse_settings_toml(toml),
        Ok(shortcuts(vec![
            text("app.quit", "Cmd+Q"),
            text("terminal.new-tab", "Cmd+T"),
            text("terminal.close-tab", "Cmd+W"),
        ]))
    );
}

#[test]
fn _2_段以上の入れ子と文字列でない値は文字列でないの印になる() {
    let toml = "[shortcuts]\n\
                number = 1\n\
                flag = true\n\
                list = [\"Cmd+T\"]\n\
                a.b.c = \"Cmd+C\"\n\
                terminal.count = 2\n\
                ok = \"Cmd+O\"\n";

    assert_eq!(
        parse_settings_toml(toml),
        Ok(shortcuts(vec![
            not_text("number"),
            not_text("flag"),
            not_text("list"),
            not_text("a.b"),
            not_text("terminal.count"),
            text("ok", "Cmd+O"),
        ]))
    );
}

#[test]
fn ほかの節は返さない() {
    let toml = "top = \"x\"\n\
                [appearance]\n\
                theme = \"dark\"\n\
                [shortcuts]\n\
                app.quit = \"Cmd+Q\"\n\
                [terminal]\n\
                font = \"Menlo\"\n";

    assert_eq!(
        parse_settings_toml(toml),
        Ok(shortcuts(vec![text("app.quit", "Cmd+Q")]))
    );
    assert_eq!(
        parse_settings_toml("[appearance]\ntheme = \"dark\"\n"),
        Ok(SettingsDocument::default())
    );
    assert_eq!(
        parse_settings_toml("shortcuts = 1\n"),
        Ok(SettingsDocument {
            sections: vec![SettingsSection {
                name: "shortcuts".to_owned(),
                body: SectionBody::NotTable,
            }],
        })
    );
}

#[test]
fn 構文エラーは行と列だけを返し本文を含まない() {
    let secret = "himitsu-no-mojiretsu";
    let toml = format!("[shortcuts]\nok = \"Cmd+O\"\n{secret} = = \"x\"\n");

    let result = parse_settings_toml(&toml);
    let Err(position) = result else {
        panic!("構文エラーになるはず: {result:?}");
    };
    assert_eq!(position.line, 3);
    assert!(position.column >= 1);

    let dir = TempDir::new("syntax");
    fs::write(dir.settings_path(), toml.as_bytes()).unwrap();
    let load = load_settings_file(&FsSettingsFile::new(dir.path().to_path_buf()));
    assert_eq!(load, SettingsLoad::Syntax(position));
    let debug = format!("{load:?}");
    assert!(
        !debug.contains(secret),
        "結果に該当行の文字列が入った: {debug}"
    );
    assert!(
        !debug.contains("Cmd+O"),
        "結果にファイルの中身が入った: {debug}"
    );
}

#[test]
fn 同じキーを_2_度書くと構文エラー() {
    let toml = "[shortcuts]\napp.quit = \"Cmd+Q\"\napp.quit = \"Cmd+W\"\n";

    let result = parse_settings_toml(toml);
    assert!(
        matches!(result, Err(TextPosition { line: 3, .. })),
        "3 行目の構文エラーになるはず: {result:?}"
    );
}

// ---- 見本を作る ----

#[test]
fn 無いときだけ見本を作り_0600() {
    let dir = TempDir::new("sample-create");
    let state_dir = dir.path().join("Arin");
    let file = FsSettingsFile::new(state_dir.clone());

    assert_eq!(
        file.create_sample_if_missing(b"# sample\n"),
        Ok(SampleCreation::Created)
    );
    let path = state_dir.join(SETTINGS_FILE_NAME);
    assert_eq!(fs::read(&path).unwrap(), b"# sample\n");
    assert_eq!(mode_of(&path), 0o600);
    assert_eq!(mode_of(&state_dir), 0o700);

    // 2 度目は作らない。
    assert_eq!(
        file.create_sample_if_missing(b"# other\n"),
        Ok(SampleCreation::AlreadyExists)
    );
    assert_eq!(fs::read(&path).unwrap(), b"# sample\n");
}

#[test]
fn あるときは見本を作らず中身と更新日時を変えない() {
    let dir = TempDir::new("sample-exists");
    fs::write(dir.settings_path(), b"[shortcuts]\n").unwrap();
    fs::set_permissions(dir.settings_path(), fs::Permissions::from_mode(0o644)).unwrap();
    let before = snapshot(&dir.settings_path());
    let file = FsSettingsFile::new(dir.path().to_path_buf());

    assert_eq!(
        file.create_sample_if_missing(b"# sample\n"),
        Ok(SampleCreation::AlreadyExists)
    );
    assert_eq!(snapshot(&dir.settings_path()), before);
    assert_eq!(mode_of(&dir.settings_path()), 0o644);

    // フォルダがあっても上書きしない。
    let folder = TempDir::new("sample-folder");
    fs::create_dir(folder.settings_path()).unwrap();
    assert_eq!(
        FsSettingsFile::new(folder.path().to_path_buf()).create_sample_if_missing(b"# sample\n"),
        Ok(SampleCreation::AlreadyExists)
    );
    assert!(folder.settings_path().is_dir());
}

#[test]
fn シンボリックリンクがあると見本を作らない() {
    let dir = TempDir::new("sample-symlink");
    // 先の無いリンク。リンクをたどって先にファイルを作ってはいけない。
    let target = dir.path().join("elsewhere.toml");
    symlink(&target, dir.settings_path()).unwrap();
    let file = FsSettingsFile::new(dir.path().to_path_buf());

    assert_eq!(
        file.create_sample_if_missing(b"# sample\n"),
        Ok(SampleCreation::AlreadyExists)
    );
    assert!(!target.exists(), "リンクの先にファイルを作った");
    assert!(
        fs::symlink_metadata(dir.settings_path())
            .unwrap()
            .file_type()
            .is_symlink()
    );
}
