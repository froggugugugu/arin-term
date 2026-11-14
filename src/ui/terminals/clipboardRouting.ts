// 画面のコピー・ペーストのイベントの入口(設計書 §9.1、PRD C-04、FR-043)。
//
// ⌘C・⌘V はメニューバーの「編集」の項目(src-tauri/src/menu.rs)が copy・paste のイベントにする。
// ここでは文書の捕捉の段階で受け、フォーカス位置で振り分ける(M1 はターミナルだけ。ツリーは M2・M3 で足す)。
// 振り分けて処理したイベントは、既定の動作と xterm.js 自身の受け取り(要素と textarea に付く)を止める。
// 二重に貼り付けないためと、入口を 1 つにするため。処理しなかったものは何もせず既定の動作に任せる。
// クリップボードを読むのはイベントの clipboardData だけで、navigator.clipboard は使わない
// (WKWebView で貼り付けの許可を求める吹き出しを出さないため。設計書 §2.2)。

import type { ClipboardKind } from "../../logic/keys/clipboard";
import type { TerminalManager } from "../../terminal/TerminalManager";

/** イベントを受ける先(本物は document)。 */
export type ClipboardEventTarget = Pick<Document, "addEventListener" | "removeEventListener">;

/** コピー・ペーストのイベントの受け取りを付ける。外す関数を返す。 */
export function installClipboardRouting(
  target: ClipboardEventTarget,
  terminals: Pick<TerminalManager, "handleClipboard">,
): () => void {
  const listen = (kind: ClipboardKind) => (event: ClipboardEvent) => {
    const data = event.clipboardData;
    if (data === null) return;
    if (!terminals.handleClipboard(kind, data)) return;
    event.preventDefault();
    event.stopPropagation();
  };
  const onCopy = listen("copy");
  const onPaste = listen("paste");
  target.addEventListener("copy", onCopy, { capture: true });
  target.addEventListener("paste", onPaste, { capture: true });
  return () => {
    target.removeEventListener("copy", onCopy, { capture: true });
    target.removeEventListener("paste", onPaste, { capture: true });
  };
}
