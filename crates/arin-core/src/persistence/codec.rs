//! `state.json` 版 1 の読み書き(計画書 T30a)。
//!
//! 設計書 §6.2 のスキーマのうち M2 で使う分(`projects[]` の `id`・`bookmark`・`lastKnownPath`・`customName`・`colorId`、
//! `ui` の `selectedProjectId`・`sidebarWidth`・`sidebarCollapsed`・`cardAreaHeight`)と、M3 で足した
//! `projects[].tree.showHidden`(計画書 T38e)、M4 で足した `ui.displayMode`(計画書 T46c)を読み書きする。
//! `tree.expanded` は M6 まで読まず書かない。
//! 入出力はしない。読んだバイト列(無ければ `None`)を受けて結果を返し、書く文字列を返すだけ。
//!
//! - 欠けたキーは既定値で補い、未知のキーは無視する(キーの追加では版を上げない)。M2 が書いた `tree` の無いファイルもそのまま読む
//! - `tree` の形が違うときは、退避せずにそのプロジェクトのツリーの設定だけを既定値にする(計画書 T38e の【仮定】)
//! - `ui.displayMode` が無い・知らない値・型が違うときは、退避せずにタブ表示にする(計画書 M4 の【仮定】16)。
//!   タブ表示(既定)のあいだは書かない(M3 までの `state.json` と同じ形のまま。ページと拡大中は保存しない)
//! - JSON として読めない・版が正しくない・新しすぎる・形が違うときは「退避する」を返す(決定 (d))。
//!   退避の名前は [`broken_file_name`] が時計の値から作る(【仮定】3)
//! - ブックマークは base64 の文字列のまま扱い、中身は解釈しない(【仮定】2)。null のものは作り直しの指示にする(【仮定】13)
//! - ペインの寸法は数として読むだけ。範囲の検査と既定値は `layout`(T30b)の規則で行う

use std::collections::HashSet;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::layout::tiles::DisplayMode;
use crate::project::{Project, ProjectId};

/// 状態ファイルの名前。正本はポート側の定数(同じ名前を 2 か所に持たない)。
pub use crate::ports::state_file::STATE_FILE_NAME;

/// 読み書きできる `schemaVersion`。
pub const STATE_SCHEMA_VERSION: u32 = 1;

/// 1 日の秒数。
const SECONDS_PER_DAY: u64 = 86_400;

/// 保存する左ペインの寸法(CSS ピクセル)と右ペインの表示モード。読んだ値をそのまま持つ。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PersistedLayout {
    /// 左ペインの幅(`ui.sidebarWidth`)。無い・null なら `None`。
    pub sidebar_width: Option<f64>,
    /// 左ペインの折りたたみ(`ui.sidebarCollapsed`)。無ければ偽。
    pub sidebar_collapsed: bool,
    /// 左ペイン上段(カード領域)の高さ(`ui.cardAreaHeight`)。無い・null なら `None`。
    pub card_area_height: Option<f64>,
    /// 右ペインの表示モード(`ui.displayMode`)。無い・知らない値ならタブ表示。
    pub display_mode: DisplayMode,
}

/// `state.json` に保存する内容。表示値(見つからない・表示用のパス)を持たない。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PersistedState {
    /// プロジェクトの一覧(配列の順が並び順)。
    pub projects: Vec<Project>,
    /// 最後に選択していたプロジェクト(`ui.selectedProjectId`)。
    pub selected_project_id: Option<ProjectId>,
    /// 左ペインの寸法。
    pub layout: PersistedLayout,
}

/// 読めた状態と、読んだあとに呼び出し側がすること。
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedState {
    /// 読めた状態。同じ ID は先のものだけを残し、選択中の ID は一覧にあるものだけにしてある。
    pub state: PersistedState,
    /// ブックマークが null(無い)のプロジェクト(並び順)。`lastKnownPath` からブックマークを作り直す。
    pub rebuild_bookmarks: Vec<ProjectId>,
}

/// 状態ファイルを読んだ結果。
#[derive(Debug, Clone, PartialEq)]
pub enum LoadOutcome {
    /// 読めた。
    Loaded(LoadedState),
    /// ファイルが無い。空の状態で始める。
    Absent,
    /// 読めない。ファイルを消さず・上書きせず、[`broken_file_name`] の名前に改名して空の状態で始める。
    Quarantine(QuarantineReason),
}

/// 状態ファイルを退避する理由。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum QuarantineReason {
    /// JSON として読めない(UTF-8 でない場合を含む)。
    #[error("state.json を JSON として読めない: {0}")]
    Malformed(String),
    /// 最上位がオブジェクトでない。
    #[error("state.json の最上位がオブジェクトでない")]
    NotAnObject,
    /// `schemaVersion` が無い。
    #[error("state.json に schemaVersion が無い")]
    MissingVersion,
    /// `schemaVersion` が 1 以上の整数でない(中身は JSON での表記)。
    #[error("state.json の schemaVersion が 1 以上の整数でない: {0}")]
    InvalidVersion(String),
    /// `schemaVersion` が対応より新しい。
    #[error("state.json の schemaVersion {0} はこの版のアプリが読める版(1)より新しい")]
    TooNew(u64),
    /// キーの型や値が版 1 の形と違う。
    #[error("state.json の中身の形が正しくない: {0}")]
    InvalidContent(String),
}

/// 状態を書けなかった。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EncodeError {
    /// JSON にできなかった(UTF-8 でないパスなど)。
    #[error("state.json に書く内容を作れなかった: {0}")]
    Serialize(String),
}

/// `state.json` の形(版 1)。
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StateFile {
    schema_version: u32,
    #[serde(default)]
    projects: Vec<Project>,
    #[serde(default)]
    ui: UiSection,
}

/// `state.json` の `ui`。
#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct UiSection {
    #[serde(default)]
    selected_project_id: Option<ProjectId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sidebar_width: Option<f64>,
    #[serde(default)]
    sidebar_collapsed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    card_area_height: Option<f64>,
    #[serde(
        default,
        deserialize_with = "lenient_display_mode",
        skip_serializing_if = "is_tabs"
    )]
    display_mode: DisplayMode,
}

/// `ui.displayMode` を読む。知らない値・型の違う値(null を含む)は、状態ファイルを退避せずにタブ表示にする
/// (1 つの値のためにプロジェクトの一覧を退避しない。【仮定】16)。
fn lenient_display_mode<'de, D>(deserializer: D) -> Result<DisplayMode, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
}

/// タブ表示(既定)なら書かない(serde の `skip_serializing_if` は参照を渡す)。
fn is_tabs(mode: &DisplayMode) -> bool {
    *mode == DisplayMode::Tabs
}

/// 状態ファイルを読む。`contents` は読んだバイト列、ファイルが無ければ `None`。
#[must_use]
pub fn decode(contents: Option<&[u8]>) -> LoadOutcome {
    let Some(bytes) = contents else {
        return LoadOutcome::Absent;
    };
    match parse(bytes) {
        Ok(file) => LoadOutcome::Loaded(tidy(file)),
        Err(reason) => LoadOutcome::Quarantine(reason),
    }
}

/// 書く文字列(版 1、字下げした JSON、末尾に改行)。
///
/// # Errors
///
/// JSON にできないとき(UTF-8 でないパスを持つプロジェクトがあるなど)。
pub fn encode(state: &PersistedState) -> Result<String, EncodeError> {
    let file = StateFile {
        schema_version: STATE_SCHEMA_VERSION,
        projects: state.projects.clone(),
        ui: UiSection {
            selected_project_id: state.selected_project_id.clone(),
            sidebar_width: state.layout.sidebar_width,
            sidebar_collapsed: state.layout.sidebar_collapsed,
            card_area_height: state.layout.card_area_height,
            display_mode: state.layout.display_mode,
        },
    };
    let mut text = serde_json::to_string_pretty(&file)
        .map_err(|error| EncodeError::Serialize(error.to_string()))?;
    text.push('\n');
    Ok(text)
}

/// 退避の名前(`state.json.broken-20261115T093000Z`)。`unix_seconds` は UTC の UNIX 時刻(秒)。
/// `existing` は状態フォルダにある名前で、同じ名前があれば `-2`、`-3` と足して空いている名前を返す。
#[must_use]
pub fn broken_file_name<I, S>(unix_seconds: u64, existing: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let taken: HashSet<String> = existing
        .into_iter()
        .map(|name| name.as_ref().to_owned())
        .collect();
    let base = format!("{STATE_FILE_NAME}.broken-{}", utc_stamp(unix_seconds));
    // 使われている名前は有限なので、`taken.len() + 2` までに必ず空きがある。
    let mut candidate = base.clone();
    let mut suffix = 2usize;
    while taken.contains(&candidate) {
        candidate = format!("{base}-{suffix}");
        suffix += 1;
    }
    candidate
}

/// バイト列を版 1 の形として読む。
fn parse(bytes: &[u8]) -> Result<StateFile, QuarantineReason> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|error| QuarantineReason::Malformed(error.to_string()))?;
    let Value::Object(object) = &value else {
        return Err(QuarantineReason::NotAnObject);
    };
    let version = object
        .get("schemaVersion")
        .ok_or(QuarantineReason::MissingVersion)?;
    match version.as_u64() {
        Some(0) | None => return Err(QuarantineReason::InvalidVersion(version.to_string())),
        Some(found) if found > u64::from(STATE_SCHEMA_VERSION) => {
            return Err(QuarantineReason::TooNew(found));
        }
        Some(_) => {}
    }
    serde_json::from_value(value)
        .map_err(|error| QuarantineReason::InvalidContent(error.to_string()))
}

/// 同じ ID の後のものを捨て、一覧に無い選択を外し、作り直しの指示を集める。
fn tidy(file: StateFile) -> LoadedState {
    let mut seen = HashSet::new();
    let projects: Vec<Project> = file
        .projects
        .into_iter()
        .filter(|project| seen.insert(project.id().clone()))
        .collect();
    let selected_project_id = file.ui.selected_project_id.filter(|id| seen.contains(id));
    let rebuild_bookmarks = projects
        .iter()
        .filter(|project| project.root().bookmark().is_none())
        .map(|project| project.id().clone())
        .collect();
    LoadedState {
        state: PersistedState {
            projects,
            selected_project_id,
            layout: PersistedLayout {
                sidebar_width: file.ui.sidebar_width,
                sidebar_collapsed: file.ui.sidebar_collapsed,
                card_area_height: file.ui.card_area_height,
                display_mode: file.ui.display_mode,
            },
        },
        rebuild_bookmarks,
    }
}

/// UTC の日時を `20261115T093000Z` の形にする。
fn utc_stamp(unix_seconds: u64) -> String {
    let (year, month, day) = civil_from_days(unix_seconds / SECONDS_PER_DAY);
    let rest = unix_seconds % SECONDS_PER_DAY;
    format!(
        "{year:04}{month:02}{day:02}T{:02}{:02}{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// 1970-01-01 からの日数をグレゴリオ暦の年・月・日にする(H. Hinnant の `civil_from_days` を 1970 年以降に限ったもの)。
fn civil_from_days(days: u64) -> (u64, u64, u64) {
    // 0000-03-01 を起点にした日数。400 年(146,097 日)を 1 つの周期として数える。
    let z = days + 719_468;
    let era = z / 146_097;
    let day_of_era = z % 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    // 3 月を 0 とした月。
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    (year, month, day)
}
