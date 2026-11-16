//! 画面へ渡す状態の写し。
//!
//! Rust の状態が変わるたびに、この写し全体を画面へ送る(設計書 §5.2)。画面はこれで丸ごと置き換える。
//! TypeScript の型は `ts-rs` で `src/ipc/generated/` に生成する(ADR-006)。
//! 生成物の書き出しはテスト(`tests/generated_types.rs`)が行い、ここでは入出力をしない。
//!
//! 絶対パスとブックマークは載せない(設計書 §12。パスのコピーは Rust が行う)。載せるのは表示用のパス(`~` 省略)だけ。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::layout::font_sizes::{FontSizeTarget, FontSizes};
use crate::layout::panes::PaneSizes;
use crate::layout::tiles::{DisplayMode, TileGrid};
use crate::layout::translucency::{Translucency, TranslucencyTarget};
use crate::project::{ProjectColor, ProjectId};
use crate::terminal::{TerminalId, TerminalStatus};

/// 写しの形の版。写しの項目の意味を変えたり項目を消したりしたときに 1 つ上げる(項目の追加では上げない)。
pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;

/// 画面へ渡す状態の写し。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    /// 写しの形の版(`SNAPSHOT_SCHEMA_VERSION`)。
    pub schema_version: u32,
    /// アプリの版(例: `0.1.0`)。
    pub app_version: String,
    /// プロジェクトの一覧(並び順のとおり)。
    pub projects: Vec<ProjectCardSnapshot>,
    /// 選択中のプロジェクト。
    pub selected_project_id: Option<ProjectId>,
    /// ターミナルの一覧(プロジェクトの並び順、その中はタブの順)。
    pub terminals: Vec<TerminalTabSnapshot>,
    /// ペインの寸法(保存値。表示値への調整は画面が行う)。
    pub layout: PaneSizes,
    /// 状態ファイルを退避したときの 1 行の知らせ。無ければ `null`。
    pub notice: Option<String>,
    /// 右ペインの表示の状態(表示モード・ページ・拡大。計画書 T46b、ADR-011)。
    pub display: DisplaySnapshot,
    /// ターミナルと画面の文字の大きさ(値・範囲・既定。計画書 T68a、ADR-013 の 2)。
    pub font_sizes: FontSizesSnapshot,
    /// ウィンドウとターミナルの背景の透過(値・範囲・既定・刻み。ADR-014 の 1)。
    pub translucency: TranslucencySnapshot,
}

/// 写しに載せる文字の大きさ(ADR-013 の 2)。範囲と既定も載せ、画面に規則の数を写さない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FontSizesSnapshot {
    /// ターミナルの文字の大きさ。
    pub terminal: FontSizeSnapshot,
    /// 画面の文字の大きさの基準値。
    pub interface: FontSizeSnapshot,
}

/// 写しに載せる 1 つの文字の大きさ(CSS ピクセルの整数)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FontSizeSnapshot {
    /// いまの値。
    pub value: u16,
    /// 範囲の最小(この値のとき画面は小さくするボタンを無効にする)。
    pub min: u16,
    /// 範囲の最大(この値のとき画面は大きくするボタンを無効にする)。
    pub max: u16,
    /// 既定の値。
    pub default_value: u16,
}

impl FontSizeSnapshot {
    fn of(sizes: FontSizes, target: FontSizeTarget) -> Self {
        Self {
            value: sizes.get(target),
            min: target.min(),
            max: target.max(),
            default_value: target.default_value(),
        }
    }
}

impl From<FontSizes> for FontSizesSnapshot {
    fn from(sizes: FontSizes) -> Self {
        Self {
            terminal: FontSizeSnapshot::of(sizes, FontSizeTarget::Terminal),
            interface: FontSizeSnapshot::of(sizes, FontSizeTarget::Interface),
        }
    }
}

/// 写しに載せる透過(ADR-014 の 1)。範囲・既定・刻みも載せ、画面に規則の数を写さない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TranslucencySnapshot {
    /// ウィンドウ(サイドバーと帯)。
    pub window: TranslucencyValueSnapshot,
    /// ターミナルの背景。
    pub terminal: TranslucencyValueSnapshot,
}

/// 写しに載せる 1 つの透過(% の整数。0 は不透明)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TranslucencyValueSnapshot {
    /// いまの値(いつも `step` の倍数)。
    pub value: u8,
    /// 範囲の最小(この値のとき画面は下げるボタンを無効にする)。
    pub min: u8,
    /// 範囲の最大(この値のとき画面は上げるボタンを無効にする)。
    pub max: u8,
    /// 既定の値。
    pub default_value: u8,
    /// 刻み(ステッパーの 1 回の差分)。
    pub step: u8,
}

impl TranslucencyValueSnapshot {
    fn of(translucency: Translucency, target: TranslucencyTarget) -> Self {
        Self {
            value: translucency.get(target),
            min: target.min(),
            max: target.max(),
            default_value: target.default_value(),
            step: target.step(),
        }
    }
}

impl From<Translucency> for TranslucencySnapshot {
    fn from(translucency: Translucency) -> Self {
        Self {
            window: TranslucencyValueSnapshot::of(translucency, TranslucencyTarget::Window),
            terminal: TranslucencyValueSnapshot::of(translucency, TranslucencyTarget::Terminal),
        }
    }
}

/// 写しに載せる右ペインの表示の状態(FR-048・FR-051〜054、ADR-011 の 1)。
///
/// グリッドとページは Rust が計算する。画面はこれを映すだけで、グリッドの規則を持たない。
/// ページ・ページ数・グリッド・タイルの並びはタブ表示のときも計算して載せる(タイル表示にしたときに出すもの)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DisplaySnapshot {
    /// 表示モード。
    pub mode: DisplayMode,
    /// タイルを拡大中か(タイル表示のときだけ真になる)。拡大しているのは `focused_terminal_id`。
    pub maximized: bool,
    /// 表示中のページ(0 から。画面の `1/2` は 1 を足して作る)。0 枚なら 0。
    pub page: u32,
    /// ページの数。0 枚なら 0。
    pub page_count: u32,
    /// 表示中のページのグリッド(列×行。最終ページはそのページの枚数で決める)。0 枚なら 0×0。
    pub grid: TileGrid,
    /// 表示中のページのタイル(並びの順=`terminals` の順)。
    pub tile_ids: Vec<TerminalId>,
    /// フォーカス中のタイル(=選択中のプロジェクトの表示中のタブ)。無ければ `null`。
    pub focused_terminal_id: Option<TerminalId>,
    /// アテンションの印(`TerminalTabSnapshot::attention`)のあるタイルのページ(0 から。昇順で重なりなし。計画書 T48b)。
    /// 画面はページ表示の前後のボタンの印に使う(`page` より小さいものがあれば前、大きいものがあれば次。FR-055)。
    /// 印はフォーカス中のターミナルだけに立たないので、表示中のページの番号(`page`)が含まれることもある(前後には数えない)。
    pub attention_pages: Vec<u32>,
}

/// 写しに載せるプロジェクト 1 件(カード 1 枚)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCardSnapshot {
    /// 識別子。
    pub id: ProjectId,
    /// 表示名。
    pub name: String,
    /// 色の識別子。
    pub color: ProjectColor,
    /// 表示用のパス(ホームの中は `~` で始める。ツールチップ用。FR-007)。
    pub display_path: String,
    /// ルートが見つからない。
    pub missing: bool,
    /// ツリーで隠しファイルを表示する(FR-017。プロジェクトごとの保存値。計画書 T38e)。
    pub show_hidden: bool,
    /// このプロジェクトのタブ(タブの順)。
    pub terminal_ids: Vec<TerminalId>,
    /// 表示中のタブ。0 枚なら `null`。
    pub active_terminal_id: Option<TerminalId>,
}

/// 写しに載せるターミナル(タブ)1 枚。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TerminalTabSnapshot {
    /// 識別子。接続・入力などのコマンドで指す。
    pub id: TerminalId,
    /// 持ち主のプロジェクト。いまは必ず値がある(M1 の 9 枚の検証画面の `null` は M4 の T49d で無くなった。
    /// 写しの形を変えないため、型は `null` を取りうるままにしてある)。
    pub project_id: Option<ProjectId>,
    /// タブの表示名(「ターミナル N」)。
    pub label: String,
    /// 状態。
    pub status: TerminalStatus,
    /// アテンションの印(フォーカス中でないあいだに BEL・OSC 9・OSC 777 があった。フォーカス中になると消える。
    /// タイル表示では見えているタイルにも立つ。FR-055、計画書 T48b)。
    pub attention: bool,
}
