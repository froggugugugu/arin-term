// エディタの部品の抽象(ADR-015 の 1。src/terminal/engine.ts にならう)。
// 画面と EditorManager はこの形だけを知り、CodeMirror(src/editor/codemirror/)を直接使わない。
// このファイルは codemirror・@codemirror/*・@lezer/*・@replit/* を import しない(境界検査が見る)。
// src/ipc からは Rust から生成した型だけを使う。

import type { EditorId } from "../ipc/generated/EditorId";
import type { LineEnding } from "../ipc/generated/LineEnding";
import type { ShortcutTable } from "../logic/keys/shortcutTable";
import type { MdBlock } from "./markdownTypes";

export type { EditorId, LineEnding };

/** 部品の要素を入れる置き場所(パネルの要素)。 */
export interface EditorSlot {
  appendChild(element: HTMLElement): unknown;
  removeChild(element: HTMLElement): unknown;
}

/**
 * エディタ 1 つぶんの表示と編集。1 つの ID に 1 つ。要素は React の管理の外に置き、タブを替えても作り直さない
 * (取り消しの履歴・カーソル・スクロールが残る。ADR-003 の xterm と同じ)。
 */
export interface EditorSurface {
  /** 置き場所に入れる要素。 */
  readonly element: HTMLElement;
  /** 要素を置き場所に入れる(ほかの置き場所に入っていれば移す)。 */
  attach(slot: EditorSlot): void;
  /** 要素を置き場所から外す(破棄しない)。入っていなければ何もしない。 */
  detach(): void;
  /**
   * 文書を丸ごと差し替える(外での変更の読み直し)。取り消しの履歴は新しく始め、カーソルは同じ行に寄せる。
   * 改行は `lineEnding` で書き出す(読み込みでは `\r\n`・`\r`・`\n` のどれも 1 つの改行にする)。onChange は呼ばない。
   */
  setText(text: string, lineEnding: LineEnding): void;
  /** いまの文書(改行は文書の `lineEnding`)。 */
  text(): string;
  /** 本文の文字の大きさ(CSS ピクセル。ターミナルの文字の大きさ。ADR-015 の 7)。 */
  setFontSize(px: number): void;
  /** Vim のキー操作を付け外しする(作り直さない。ADR-015 の 6)。 */
  setVim(enabled: boolean): void;
  /** 読み取り専用にする(カーソルの移動と選択・コピーはできる)。 */
  setReadOnly(readOnly: boolean): void;
  /** 入力のフォーカスを移す。置き場所に無いとき・破棄の後は何もしない。 */
  focus(): void;
  /** 入力のフォーカスがこの部品の中にあるか。 */
  hasFocus(): boolean;
  /** 文書の全体を選ぶ(メニューの「すべてを選択」)。 */
  selectAll(): void;
  /** 本人の編集で文書が変わったときに受け取る(setText では呼ばない)。 */
  onChange(listener: () => void): void;
  /** 保存のキー(表の `editor.save`)か Vim の `:w` で受け取る。 */
  onSave(listener: () => void): void;
  /** 資源を手放す。2 回目以降は何もしない。 */
  dispose(): void;
}

/** 部品を作るときの指定。 */
export interface EditorSurfaceOptions {
  readonly text: string;
  readonly lineEnding: LineEnding;
  readonly readOnly: boolean;
  /** 本文の文字の大きさ(CSS ピクセル)。 */
  readonly fontSize: number;
  readonly vim: boolean;
  /** いまのショートカットの表。キーを押すたびに引く(表を差し替えても部品を作り直さない)。 */
  readonly shortcuts: () => ShortcutTable;
}

/** 部品を作るもの。本物は src/editor/codemirror/CodeMirrorEngine.ts(動的 import で読む)。 */
export interface EditorEngine {
  createSurface(id: EditorId, options: EditorSurfaceOptions): EditorSurface;
  /** プレビューの木を作る(GFM。src/editor/codemirror/markdownAst.ts)。 */
  parseMarkdown(text: string): MdBlock[];
}
