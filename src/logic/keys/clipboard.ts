// コピー・ペーストの振り分け(設計書 §9.1、PRD C-04、FR-043)。DOM を使わない純粋な関数。
//
// ⌘C・⌘V は、メニューバーの「編集」の項目が画面のコピー・ペーストのイベントにする。
// 画面はそのイベントを入口にして、フォーカス位置で振り分ける。
// M1 のフォーカス位置はターミナルかそれ以外だけ。ツリー(M2・M3)は ClipboardFocus に種類を足す。
// キーの受け手(terminalKeyOwner)は ./keyOwner にある。

/** 画面のコピー・ペーストのイベントの種類。 */
export type ClipboardKind = "copy" | "paste";

/** イベントが起きたときのフォーカス位置。 */
export type ClipboardFocus =
  | { readonly kind: "terminal"; readonly hasSelection: boolean }
  | { readonly kind: "other" };

/**
 * 振り分けた先。
 * - `copy-selection`: ターミナルの選択の文字列をクリップボードに書く
 * - `paste`: クリップボードの文字列をターミナルに貼り付ける(PTY への入力になる)
 * - `none`: 何もしない(画面の既定の動作に任せる)
 */
export type ClipboardAction = "copy-selection" | "paste" | "none";

/** コピー・ペーストのイベントを、フォーカス位置で振り分ける。 */
export function routeClipboard(kind: ClipboardKind, focus: ClipboardFocus): ClipboardAction {
  if (focus.kind !== "terminal") return "none";
  if (kind === "paste") return "paste";
  return focus.hasSelection ? "copy-selection" : "none";
}
