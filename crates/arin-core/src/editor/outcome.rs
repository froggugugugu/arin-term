//! 文書のコマンドの応答とイベントの中身(ADR-015 の 4、計画書 T81d)。
//!
//! 画面とやり取りするのはプロジェクトの ID・相対パス・エディタの ID だけで、絶対パスは載せない。
//! 失敗の文言(`failed { message }`)は決まった日本語の 1 行で、ポートの失敗の文言(絶対パスを含む)は使わない。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{EditorId, LineEnding, ReadOnlyReason};

/// 文書を開かない理由(`open_document` の `not-allowed`)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum DocumentNotAllowedReason {
    /// 拡張子が `.md`・`.markdown` でない。
    NotMarkdown,
    /// シンボリックリンク(リンク自体を開かない)。
    Link,
    /// フォルダ。
    Directory,
    /// 1 MiB を超える。
    TooLarge,
    /// UTF-8 でない、または NUL を含む。
    NotText,
    /// ルートの外(途中のリンクがルートの外へ出る)。
    OutsideRoot,
}

/// `open_document` の応答。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum OpenDocumentOutcome {
    /// 新しいエディタのタブで開いて表示した(写しにも載る)。
    Opened {
        /// 開いたエディタ。
        editor_id: EditorId,
        /// 本文(先頭の BOM は外してある)。
        text: String,
        /// 改行(CodeMirror の `lineSeparator` をこれに合わせる)。
        line_ending: LineEnding,
        /// 読み取り専用で開いた理由。書けるなら `null`。
        read_only: Option<ReadOnlyReason>,
    },
    /// 同じファイルが既に開いていたので、そのタブを表示しただけ(本文は送らない。画面の持っている本文を使う)。
    AlreadyOpen {
        /// 表示したエディタ。
        editor_id: EditorId,
    },
    /// 開かない(理由つき)。何も変えていない。
    NotAllowed {
        /// 理由。
        reason: DocumentNotAllowedReason,
    },
    /// ファイルが無い。何も変えていない。
    NotFound,
    /// 開けなかった。何も変えていない。
    Failed {
        /// 画面に出す 1 行の文言(絶対パスを含まない)。
        message: String,
    },
}

/// `save_document` の保存のしかた(本人が保存の衝突の確認で選んだもの)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum SaveMode {
    /// ふつうの保存。開いたとき(最後に保存・当てたとき)の版と同じときだけ置き換え、無ければ作らない。
    Normal,
    /// 「上書き保存」。版を比べずに置き換える。無ければ作らない(`missing`)。
    Overwrite,
    /// 「同じ名前で作る」。無いときだけ作る(`RENAME_EXCL`)。同じ名前があれば書かずに `changed-outside`。
    Create,
}

/// `save_document` の応答。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum SaveDocumentOutcome {
    /// 保存した(版を持ち直す)。未保存の印は替えない(保存中に打った分があるかは画面しか知らないので、画面が送る)。
    Saved,
    /// 開いたあと(最後に保存・当てたあと)に外で変わっていた、または「作る」で同じ名前があったので書いていない。
    /// 本人が「上書き保存」を選んだら `overwrite` で呼び直す。
    ChangedOutside,
    /// ファイルが無い(外で消えた・名前が変わった)ので書いていない。本人が「同じ名前で作る」を選んだら
    /// `create` で呼び直す(作るときは上書きしない)。
    Missing,
    /// 読み取り専用で開いた文書なので書いていない。
    ReadOnly,
    /// 保存すると 1 MiB を超えるので書いていない。
    TooLarge,
    /// 保存できなかった(保存先のフォルダが無いときを含む)。元のファイルは変わっていない。
    Failed {
        /// 画面に出す 1 行の文言(絶対パスを含まない)。
        message: String,
    },
}

/// `check_document` で本文を受け取るか。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum CheckText {
    /// 受け取らない(未保存の変更があるとき)。
    Never,
    /// 外で変わっていたら受け取る(未保存の変更が無いとき)。
    IfChanged,
    /// 変わっていなくても受け取る(本人が衝突の確認で「読み直す」を選んだとき)。
    Always,
}

/// `check_document` の応答(外で変わったかの確認。ADR-015 の 5)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum CheckDocumentOutcome {
    /// 変わっていない(`touch` だけのときもこれ)。`withText` が `always` で読めたときは `changed` になる。
    Unchanged,
    /// 外で変わった(`withText` が `always` なら、変わっていなくても読めればこれ)。確認はエディタの状態を変えない。
    /// `text` があるときは `token` に読んだ版の印が入る。画面は本文を実際に当てたあとにだけ、その印で
    /// `adopt_document` を呼ぶ(そのとき Rust は版を持ち直す)。当てなければ版は古いままで、保存は `changed-outside` になる。
    Changed {
        /// 新しい本文(先頭の BOM は外してある)。`withText` が `never`・文字として読めない・大きすぎるときは `null`。
        text: Option<String>,
        /// 新しい本文の改行(`text` が `null` のときは `lf`)。
        line_ending: LineEnding,
        /// 読んだ版の印(`text` があるときだけ。版の中身ではない)。
        token: Option<u32>,
    },
    /// ファイルが無い(ふつうのファイルでなくなった場合を含む)。
    Missing,
}

/// `editor-close-requested` イベントの中身(⌘W で表示中のエディタが未保存のとき。ADR-015 の 4)。
/// 画面が「保存」「保存しない」「キャンセル」を聞き、閉じるなら `close_editor` を呼ぶ。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EditorCloseRequest {
    /// 閉じようとしたエディタ。
    pub editor_id: EditorId,
}
