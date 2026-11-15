// 画面のコピー・切り取り・ペーストのイベントの入口(設計書 §9.1、PRD C-04、FR-043、FR-027)。
//
// ⌘C・⌘X・⌘V はメニューバーの「編集」の項目(src-tauri/src/menu.rs)が copy・cut・paste のイベントにする。
// ここでは文書の捕捉の段階で受け、フォーカス位置で振り分ける(src/logic/keys/clipboard.ts の routeClipboard):
// - ツリーにフォーカスがあれば(M3。T43c)ツリーの操作(項目のコピー・切り取り・ペースト)にし、ターミナルには渡さない
// - ツリーの名前の入力欄にフォーカスがあれば何もしない(入力欄の文字のコピー・ペーストは画面の既定の動作に任せる)
// - それ以外はターミナルへ(M1 のまま。フォーカスのあるターミナルが無ければ何もしない)
// 振り分けて処理したイベントは、既定の動作と xterm.js 自身の受け取り(要素と textarea に付く)を止める。
// 二重に貼り付けないためと、入口を 1 つにするため。処理しなかったものは何もせず既定の動作に任せる
// (ツリーで切り取れないときなども、空の中身でクリップボードを書き換えないよう止めない)。
// クリップボードを読むのはイベントの clipboardData だけで、navigator.clipboard は使わない
// (WKWebView で貼り付けの許可を求める吹き出しを出さないため。設計書 §2.2)。ツリーの操作はクリップボードの中身を
// 画面で読まない(ファイルのペーストボードは Rust が読み書きする)。

import { type ClipboardFocus, type ClipboardKind, routeClipboard } from "../../logic/keys/clipboard";
import type { TerminalManager } from "../../terminal/TerminalManager";

/** イベントを受ける先(本物は document)。 */
export type ClipboardEventTarget = Pick<Document, "addEventListener" | "removeEventListener">;

/** ツリーの操作になる振り分け先。 */
export type TreeClipboardAction = "tree-copy" | "tree-cut" | "tree-paste";

/** ツリーの受け口(本物は src/ui/menu/runTreeMenuCommand.ts の createTreeClipboard)。 */
export interface TreeClipboardTarget {
  /** ツリーか名前の入力欄にフォーカスがあれば、その位置。どちらでもなければ null(ターミナルへの振り分けに任せる)。 */
  focus(): ClipboardFocus | null;
  /** ツリーの操作を行う。 */
  run(action: TreeClipboardAction): void;
}

const KINDS: readonly ClipboardKind[] = ["copy", "cut", "paste"];

/** コピー・切り取り・ペーストのイベントの受け取りを付ける。外す関数を返す。 */
export function installClipboardRouting(
  target: ClipboardEventTarget,
  terminals: Pick<TerminalManager, "handleClipboard">,
  tree?: TreeClipboardTarget,
): () => void {
  const handled = (event: ClipboardEvent) => {
    event.preventDefault();
    event.stopPropagation();
  };
  const listen = (kind: ClipboardKind) => (event: ClipboardEvent) => {
    const focus = tree?.focus() ?? null;
    if (focus !== null) {
      const action = routeClipboard(kind, focus);
      if (action === "tree-copy" || action === "tree-cut" || action === "tree-paste") {
        tree?.run(action);
        handled(event);
      }
      return;
    }
    const data = event.clipboardData;
    if (data === null) return;
    if (!terminals.handleClipboard(kind, data)) return;
    handled(event);
  };
  const listeners = KINDS.map((kind) => [kind, listen(kind)] as const);
  for (const [kind, listener] of listeners) target.addEventListener(kind, listener, { capture: true });
  return () => {
    for (const [kind, listener] of listeners) target.removeEventListener(kind, listener, { capture: true });
  };
}
