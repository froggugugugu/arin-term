// ツリーのドラッグの落とし先の目印(ADR-007 の (b)、計画 T43d)。
//
// 落とし先は、離した位置の要素(document.elementFromPoint)から data- 属性で引く:
// - ツリーの行: data-testid="tree-row" と data-path・data-kind(T43b のまま)
// - ツリーのヘッダ(ルート): data-testid="tree-header"
// - ターミナルの置き場所: data-drop-zone="terminal" と data-terminal-id(TerminalHost が付ける)。xterm.js の要素は
//   置き場所の子孫なので、カーソルが xterm.js の要素の上にあっても closest で置き場所が見つかる(xterm.js には手を入れない)
// 強調する要素には data-drop-active="true" を付ける(行・ヘッダは React の props で、置き場所は要素へ直接)。

import type { DropHit } from "../../logic/dnd/dropTarget";

/** ターミナルの置き場所の目印(TerminalHost の要素に付ける)。 */
export const TERMINAL_DROP_ZONE = "terminal";

/**
 * ターミナルへフォーカスを移す求め(落としたあと、続けて入力できるように)。置き場所の要素に発火し、
 * TerminalHost が受けて TerminalManager.focus を呼ぶ(ツリーは TerminalManager を持たない)。
 */
export const TERMINAL_FOCUS_REQUEST = "arin:terminal-focus-request";

/** TerminalHost の要素に付ける属性。 */
export function terminalDropZoneAttributes(terminalId: string): Record<string, string> {
  return { "data-drop-zone": TERMINAL_DROP_ZONE, "data-terminal-id": terminalId };
}

/** 落とし先と、強調に使う要素。 */
export interface ResolvedHit {
  readonly hit: DropHit;
  /** ターミナルの置き場所の要素(強調とフォーカスの求めに使う)。ターミナルでなければ null。 */
  readonly terminalElement: HTMLElement | null;
}

/**
 * 画面の位置 (x, y) の落とし先。ツリーの中は tree(自分のツリーの要素)の中だけを見る。
 * 行の無い余白はルートとして扱う(Finder の一覧の余白と同じ。【仮定】)。どれでもなければ null。
 */
export function dropHitAt(x: number, y: number, tree: HTMLElement): ResolvedHit | null {
  const element = document.elementFromPoint(x, y);
  if (element === null) return null;
  const terminal = element.closest<HTMLElement>(`[data-drop-zone="${TERMINAL_DROP_ZONE}"]`);
  if (terminal !== null) {
    const terminalId = terminal.dataset.terminalId;
    return terminalId === undefined || terminalId === ""
      ? null
      : { hit: { kind: "terminal", terminalId }, terminalElement: terminal };
  }
  if (!tree.contains(element)) return null;
  const rowElement = element.closest<HTMLElement>('[data-testid="tree-row"]');
  if (rowElement !== null) {
    const path = rowElement.dataset.path ?? "";
    if (path === "") return null;
    // フォルダへのリンクは展開しないので、ファイルと同じく親へ落とす(src/logic/dnd/dropTarget.ts の【仮定】)。
    const hit: DropHit = rowElement.dataset.kind === "directory" ? { kind: "folder", path } : { kind: "file", path };
    return { hit, terminalElement: null };
  }
  // 新規作成の入力欄の行(data-path が無い)には落とさない。
  if (element.closest('[role="treeitem"]:not([data-testid="tree-header"])') !== null) return null;
  if (element.closest('[data-testid="tree-header"]') !== null) return { hit: { kind: "root" }, terminalElement: null };
  if (element.closest('[data-testid="tree-scroller"]') !== null) return { hit: { kind: "root" }, terminalElement: null };
  return null;
}

/** ターミナルの置き場所の要素へ、フォーカスの求めを発火する。 */
export function requestTerminalFocus(element: HTMLElement): void {
  element.dispatchEvent(new CustomEvent(TERMINAL_FOCUS_REQUEST));
}
