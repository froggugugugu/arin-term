// テスト用の偽のエディタのエンジン(ADR-015 の 1)。作った部品と、部品への操作を記録する。
// CodeMirror も DOM も使わない(Vitest は node の環境で動く)。取り消しの履歴は文字列の積み重ねで真似る。

import type { EditorEngine, EditorId, EditorSlot, EditorSurface, EditorSurfaceOptions, LineEnding } from "../editor/engine";
import type { MdBlock } from "../editor/markdownTypes";

export interface FakeEditorSurface extends EditorSurface {
  readonly id: EditorId;
  /** createSurface に渡された指定。 */
  readonly options: EditorSurfaceOptions;
  /** いまの文書(改行は文書の改行の 1 種類に揃えてある)。 */
  current: string;
  lineEnding: LineEnding;
  /** 取り消しの履歴(編集の前の文書の積み重ね)。setText で空になる。 */
  readonly history: string[];
  /** いま入っている置き場所。 */
  slot: EditorSlot | null;
  attachCount: number;
  detachCount: number;
  disposeCount: number;
  readonly fontSizeCalls: number[];
  readonly vimCalls: boolean[];
  readonly readOnlyCalls: boolean[];
  readonly setTextCalls: { text: string; lineEnding: LineEnding }[];
  vim: boolean;
  readOnly: boolean;
  focused: boolean;
  selectAllCount: number;
  /** 本人が打ったものとして文書を替える(履歴に積み、変更の受け取り手へ流す)。 */
  type(next: string): void;
  /** 取り消す(履歴から戻し、変更の受け取り手へ流す)。履歴が無ければ何もしない。 */
  undo(): void;
  /** 保存のキーを押したものとして、保存の受け取り手へ流す。 */
  pressSave(): void;
}

export interface FakeEditorEngine extends EditorEngine {
  /** 作った順の部品。 */
  readonly created: FakeEditorSurface[];
  /** parseMarkdown に渡された順の文書。 */
  readonly parsed: string[];
}

/** CodeMirror の部品と同じく、読み込んだ文書の改行(`\r\n`・`\r`・`\n`)を文書の改行の 1 種類に揃える。 */
const unifyLineEndings = (text: string, lineEnding: LineEnding): string =>
  text.replace(/\r\n|\r|\n/g, lineEnding === "crlf" ? "\r\n" : "\n");

/** 偽の解析の結果(文書を 1 つの段落にするだけ)。 */
export const fakeBlocks = (text: string): MdBlock[] => [{ kind: "paragraph", children: [{ kind: "text", text }] }];

export function createFakeEditorEngine(): FakeEditorEngine {
  const created: FakeEditorSurface[] = [];
  const parsed: string[] = [];
  return {
    created,
    parsed,
    parseMarkdown(text) {
      parsed.push(text);
      return fakeBlocks(text);
    },
    createSurface(id, options) {
      const changeListeners: (() => void)[] = [];
      const saveListeners: (() => void)[] = [];
      const surface: FakeEditorSurface = {
        id,
        options,
        element: { fakeEditorId: id } as unknown as HTMLElement,
        current: unifyLineEndings(options.text, options.lineEnding),
        lineEnding: options.lineEnding,
        history: [],
        slot: null,
        attachCount: 0,
        detachCount: 0,
        disposeCount: 0,
        fontSizeCalls: [],
        vimCalls: [],
        readOnlyCalls: [],
        setTextCalls: [],
        vim: options.vim,
        readOnly: options.readOnly,
        focused: false,
        selectAllCount: 0,
        attach(slot) {
          if (surface.slot === slot) return;
          surface.slot?.removeChild(surface.element);
          slot.appendChild(surface.element);
          surface.slot = slot;
          surface.attachCount++;
        },
        detach() {
          if (surface.slot === null) return;
          surface.slot.removeChild(surface.element);
          surface.slot = null;
          surface.focused = false;
          surface.detachCount++;
        },
        setText(text, lineEnding) {
          surface.current = unifyLineEndings(text, lineEnding);
          surface.lineEnding = lineEnding;
          surface.history.length = 0;
          surface.setTextCalls.push({ text, lineEnding });
        },
        text: () => surface.current,
        setFontSize(px) {
          surface.fontSizeCalls.push(px);
        },
        setVim(enabled) {
          surface.vim = enabled;
          surface.vimCalls.push(enabled);
        },
        setReadOnly(readOnly) {
          surface.readOnly = readOnly;
          surface.readOnlyCalls.push(readOnly);
        },
        focus() {
          if (surface.slot !== null && surface.disposeCount === 0) surface.focused = true;
        },
        hasFocus: () => surface.focused,
        selectAll() {
          surface.selectAllCount++;
        },
        onChange(listener) {
          changeListeners.push(listener);
        },
        onSave(listener) {
          saveListeners.push(listener);
        },
        dispose() {
          surface.disposeCount++;
        },
        type(next) {
          surface.history.push(surface.current);
          surface.current = next;
          for (const listener of changeListeners) listener();
        },
        undo() {
          const previous = surface.history.pop();
          if (previous === undefined) return;
          surface.current = previous;
          for (const listener of changeListeners) listener();
        },
        pressSave() {
          for (const listener of saveListeners) listener();
        },
      };
      created.push(surface);
      return surface;
    },
  };
}
