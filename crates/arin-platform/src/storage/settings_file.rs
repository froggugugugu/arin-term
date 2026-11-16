//! 設定ファイル(`settings.toml`)を読む・無いときだけ見本を作る・TOML を解析する
//! (設計書 §5.1 `SettingsFilePort`、§1.3 #10、ADR-012 の 1・2・4、計画書 T65a)。
//!
//! 設定ファイルは人が書くもので、アプリは読むだけ(見本は無いときだけ作る)。上書き・改名・削除の口は持たない。
//! UTF-8 と大きさの確かめは `arin_core::settings::SettingsLoad::from_file` が行い、ここは TOML の解析を渡す。
//! 失敗の理由は種類だけを返し、パス・OS の文言・TOML のクレートの文言は持たない(知らせに出さないため)。

use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;

use arin_core::ports::settings_file::{
    MAX_SETTINGS_FILE_BYTES, SETTINGS_FILE_NAME, SampleCreation, SettingsFileError,
    SettingsFilePort, SettingsFileRead,
};
use arin_core::settings::{
    SHORTCUTS_SECTION, SectionBody, SettingEntry, SettingValue, SettingsDocument, SettingsLoad,
    SettingsSection, TextPosition,
};
use toml::de::{DeTable, DeValue};

/// 状態フォルダの権限(作るときだけ使う)。
const DIR_MODE: u32 = 0o700;
/// 見本のファイルの権限。
const FILE_MODE: u32 = 0o600;

/// 状態フォルダの `settings.toml` をファイルシステムで読む(見本は無いときだけ作る)。
#[derive(Debug, Clone)]
pub struct FsSettingsFile {
    state_dir: PathBuf,
}

impl FsSettingsFile {
    /// 状態フォルダを渡して作る。ここではフォルダを作らない・調べない。
    #[must_use]
    pub fn new(state_dir: PathBuf) -> Self {
        Self { state_dir }
    }

    fn settings_path(&self) -> PathBuf {
        self.state_dir.join(SETTINGS_FILE_NAME)
    }

    /// 状態フォルダが無ければ 0700 で作る。あれば権限を変えない。親のフォルダは作らない。
    fn ensure_state_dir(&self) -> io::Result<()> {
        match DirBuilder::new().mode(DIR_MODE).create(&self.state_dir) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
            Err(e) => Err(e),
        }
    }
}

fn kind_of(error: &io::Error) -> SettingsFileError {
    if error.kind() == io::ErrorKind::PermissionDenied {
        SettingsFileError::PermissionDenied
    } else {
        SettingsFileError::Io
    }
}

impl SettingsFilePort for FsSettingsFile {
    fn read(&self) -> Result<SettingsFileRead, SettingsFileError> {
        // シンボリックリンクはたどる(ADR-012 の 1)。名前付きパイプなどで止まらないよう O_NONBLOCK で開く
        // (ふつうのファイルの読み取りには影響しない)。
        let mut file = match OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(self.settings_path())
        {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(SettingsFileRead::Missing),
            Err(e) => return Err(kind_of(&e)),
        };
        let metadata = file.metadata().map_err(|e| kind_of(&e))?;
        if !metadata.is_file() {
            return Err(SettingsFileError::NotAFile);
        }
        if metadata.len() > MAX_SETTINGS_FILE_BYTES {
            return Err(SettingsFileError::TooLarge);
        }
        // 調べたあとに伸びても上限 + 1 バイトまでしか読まない。
        let mut bytes = Vec::new();
        (&mut file)
            .take(MAX_SETTINGS_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| kind_of(&e))?;
        if u64::try_from(bytes.len()).map_or(true, |len| len > MAX_SETTINGS_FILE_BYTES) {
            return Err(SettingsFileError::TooLarge);
        }
        Ok(SettingsFileRead::Contents(bytes))
    }

    fn create_sample_if_missing(
        &self,
        contents: &[u8],
    ) -> Result<SampleCreation, SettingsFileError> {
        self.ensure_state_dir().map_err(|e| kind_of(&e))?;
        // create_new(O_CREAT | O_EXCL)は同じ名前があれば失敗し、シンボリックリンクもたどらない
        // (先の無いリンクでも先に作らない)。
        let mut file: File = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(FILE_MODE)
            .open(self.settings_path())
        {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                return Ok(SampleCreation::AlreadyExists);
            }
            Err(e) => return Err(kind_of(&e)),
        };
        // 書きかけで失敗しても消さない(削除の口を作らない。【仮定】次からは「もうある」として扱い、本人が直す)。
        // umask で狭まっていても広がっていても 0600 に揃える。
        file.set_permissions(fs::Permissions::from_mode(FILE_MODE))
            .and_then(|()| file.write_all(contents))
            .and_then(|()| file.sync_all())
            .map_err(|e| kind_of(&e))?;
        // 名前の追加をフォルダにも書き込む。
        File::open(&self.state_dir)
            .and_then(|dir| dir.sync_all())
            .map_err(|e| kind_of(&e))?;
        Ok(SampleCreation::Created)
    }
}

/// 設定ファイルを読んで、TOML として解析した結果にする。
pub fn load_settings_file(port: &impl SettingsFilePort) -> SettingsLoad {
    SettingsLoad::from_file(port.read(), parse_settings_toml)
}

/// TOML の文字列を、`arin-core` に渡す素朴な形にする(ADR-012 の 2)。
///
/// - 返す節は `[shortcuts]` だけ(ほかの節は持たない)。表でなければ [`SectionBody::NotTable`]
/// - 文字列の値はそのまま。入れ子の表は 1 段だけキーを `.` でつなぐ(`terminal.new-tab = …` も
///   `[shortcuts.terminal]` の `new-tab = …` も `terminal.new-tab`)。2 段以上と文字列でない値は
///   [`SettingValue::NotText`](2 段以上はつないだ 1 段目までのキー)
/// - 行は書いた順(キーの位置で並べる)
///
/// # Errors
///
/// TOML の構文エラー(同じキーを 2 度書いたときを含む)。クレートが返す位置から行と列だけを返し、
/// クレートの文言は持たない。位置が無ければ 1 行 1 列(【仮定】)。
pub fn parse_settings_toml(text: &str) -> Result<SettingsDocument, TextPosition> {
    let root = DeTable::parse(text).map_err(|error| {
        TextPosition::from_byte_offset(text, error.span().map_or(0, |span| span.start))
    })?;
    let mut sections = Vec::new();
    if let Some(value) = root.get_ref().get(SHORTCUTS_SECTION) {
        let body = match value.get_ref() {
            DeValue::Table(table) => SectionBody::Table(entries_of(table)),
            _ => SectionBody::NotTable,
        };
        sections.push(SettingsSection {
            name: SHORTCUTS_SECTION.to_owned(),
            body,
        });
    }
    Ok(SettingsDocument { sections })
}

/// 節の表の行を、キーの位置の順に並べて返す(表はキーの名前の順で持たれるため)。
fn entries_of(table: &DeTable<'_>) -> Vec<SettingEntry> {
    let mut entries: Vec<(usize, SettingEntry)> = Vec::new();
    for (key, value) in table.iter() {
        match value.get_ref() {
            DeValue::Table(inner) => {
                for (inner_key, inner_value) in inner.iter() {
                    entries.push((
                        inner_key.span().start,
                        SettingEntry {
                            key: format!("{}.{}", key.get_ref(), inner_key.get_ref()),
                            value: value_of(inner_value.get_ref()),
                        },
                    ));
                }
            }
            other => entries.push((
                key.span().start,
                SettingEntry {
                    key: key.get_ref().to_string(),
                    value: value_of(other),
                },
            )),
        }
    }
    entries.sort_by_key(|(position, _)| *position);
    entries.into_iter().map(|(_, entry)| entry).collect()
}

/// 1 段つないだ先の値。文字列だけを取り出し、表(2 段目以上)を含むほかは印にする。
fn value_of(value: &DeValue<'_>) -> SettingValue {
    match value {
        DeValue::String(text) => SettingValue::Text(text.to_string()),
        _ => SettingValue::NotText,
    }
}
