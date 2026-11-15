//! ファイル操作で画面とやり取りする型(ts-rs で生成する。計画書 T38a)。
//!
//! 名前が衝突したら、コマンドは衝突を結果で返し、画面が対処(置き換える / 両方残す)を付けて呼び直す(設計書 §5.2)。
//! 絶対パスは載せない。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::paths::RelPath;

/// ファイル操作のコマンドの応答(T42c)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum FileOpOutcome {
    /// 済んだ。
    Done {
        /// 操作のあとに選ぶ項目(作った・移した先の項目)。選ぶものが無ければ `null`(ゴミ箱に入れたときなど)。
        select: Option<RelPath>,
    },
    /// 対象が実行の時点で無かった。何も変えていない(FR-036)。
    NotFound,
    /// 置く先に同じ名前があった。何も変えていない。画面が確認を出し、対処を付けて呼び直す(FR-030)。
    Conflict {
        /// 衝突した名前。
        name: String,
    },
    /// 名前が正しくない。何も変えていない(FR-025・FR-026)。
    InvalidName {
        /// 理由。
        reason: InvalidNameReason,
    },
    /// 対象にできない。何も変えていない(FR-035)。
    NotAllowed {
        /// 理由。
        reason: NotAllowedReason,
    },
    /// それ以外の失敗。何も変えていないとは限らない(フォルダのコピーの途中など)。
    Failed {
        /// 画面に出す 1 行の文言。
        message: String,
    },
}

/// 名前が正しくない理由(設計書 §7.2 の `NameValidator`)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum InvalidNameReason {
    /// 空。
    Empty,
    /// `/` を含む。
    ContainsSlash,
    /// NUL を含む。
    ContainsNul,
    /// `.` か `..`。
    DotName,
    /// UTF-8 で 255 バイトを超える。
    TooLong,
}

/// 対象にできない理由(設計書 §7.2 の `PathScopeGuard`)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum NotAllowedReason {
    /// ルートの外(シンボリックリンクで外へ出るものを含む)。
    OutsideRoot,
    /// ルート自体(名称変更・移動・切り取り・ゴミ箱の対象にしない)。
    RootItself,
    /// フォルダを自分自身かその子孫へ移す・写す。
    IntoDescendant,
}

/// 衝突したときの対処(確認ダイアログで選ばれたもの。FR-030)。中止は呼び直さないことで表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ConflictResolution {
    /// 既存をゴミ箱に入れてから置く(上書きしない)。
    Replace,
    /// 番号付きの名前で置く(`a.md` → `a 2.md`)。
    KeepBoth,
}

/// 新しく作る項目の種類(「新規ファイル…」「新規フォルダ…」。FR-025)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum NewEntryKind {
    /// 空のファイル。
    File,
    /// フォルダ。
    Folder,
}
