// コピー・切り取り・ペーストの振り分け(設計書 §9.1、PRD C-04、FR-043、FR-027)。DOM を使わない純粋な関数。
//
// ⌘C・⌘X・⌘V は、メニューバーの「編集」の項目が画面のコピー・切り取り・ペーストのイベントにする(「切り取り」の項目は計画 T42e)。
// 画面はそのイベントを入口にして、フォーカス位置で振り分ける。ツリーにフォーカスがあればツリーの操作にし、ターミナルには貼り付けない。
// キーの受け手(terminalKeyOwner)は ./keyOwner にある。

/** 画面のコピー・切り取り・ペーストのイベントの種類。 */
export type ClipboardKind = "copy" | "cut" | "paste";

/**
 * ツリーで選んでいるもの。
 * - `entry`: ファイルかフォルダ(ルート以外)
 * - `root`: ルート(ヘッダ。計画【仮定】1)。切り取りの対象にしない(FR-035)
 * - `none`: 何も選んでいない
 */
export type TreeSelectionKind = "entry" | "root" | "none";

/** イベントが起きたときのフォーカス位置。 */
export type ClipboardFocus =
  | { readonly kind: "terminal"; readonly hasSelection: boolean }
  | { readonly kind: "tree"; readonly selection: TreeSelectionKind }
  | { readonly kind: "name-input" }
  | { readonly kind: "other" };

/**
 * 振り分けた先。
 * - `copy-selection`: ターミナルの選択の文字列をクリップボードに書く
 * - `paste`: クリップボードの文字列をターミナルに貼り付ける(PTY への入力になる)
 * - `tree-copy`・`tree-cut`: ツリーで選んだ項目をコピー・切り取りする
 * - `tree-paste`: ツリーへ貼り付ける(貼り付け先は選んだ項目から決める。何も選んでいなければルート)
 * - `none`: 何もしない(画面の既定の動作に任せる。名前の入力欄では入力欄の文字の操作になる)
 */
export type ClipboardAction = "copy-selection" | "paste" | "tree-copy" | "tree-cut" | "tree-paste" | "none";

/** コピー・切り取り・ペーストのイベントを、フォーカス位置で振り分ける。 */
export function routeClipboard(kind: ClipboardKind, focus: ClipboardFocus): ClipboardAction {
  switch (focus.kind) {
    case "terminal":
      if (kind === "paste") return "paste";
      if (kind === "copy" && focus.hasSelection) return "copy-selection";
      return "none";
    case "tree":
      if (kind === "paste") return "tree-paste";
      if (focus.selection === "none") return "none";
      if (kind === "copy") return "tree-copy";
      return focus.selection === "root" ? "none" : "tree-cut";
    case "name-input":
    case "other":
      return "none";
  }
}
