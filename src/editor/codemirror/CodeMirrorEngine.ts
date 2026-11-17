// CodeMirror 6 で作るエディタの部品(ADR-015 の 1・6・7)。src/terminal/xterm/XtermEngine.ts にならう。
// codemirror・@codemirror/*・@lezer/*・@replit/* を import してよいのは src/editor/codemirror/ だけ(境界検査)。
// src/ipc は import しない(型は ../engine から受け取る)。EditorManager が動的 import で読む。
//
// - Markdown のモードは GFM(markdownLanguage)。`codeLanguages` を渡さない(コードの塊の言語を読み込ませない。ADR-015 の 2 の 5)
// - キー: 要素の捕捉の段階で keydown を見て、表の受け口が menu-item・edit-item のキー(editorKeyOwner が `app`)は
//   伝播だけを止めて CodeMirror に渡さない。既定の動作は止めない(メニューのキーとして OS に届かせる。⌘W・⌘T・⌘A など)。
//   `editor.save` のキーは既定の動作を止めて保存を知らせる。それ以外は CodeMirror(通常の keymap か Vim)
// - Vim: `@replit/codemirror-vim` を Compartment で付け外しする(作り直さない)。`:w` は保存
// - 改行: 中では `\n` の 1 種類で持ち(読み込みでは `\r\n`・`\r`・`\n` のどれも改行にする)、text() で文書の改行に戻す
// - 色と書体はトークン(tokens.css の CSS の変数)。本文の大きさは外側の要素の font-size(ターミナルの文字の大きさ)

import { selectAll } from "@codemirror/commands";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { Compartment, EditorSelection, EditorState, type Extension } from "@codemirror/state";
import { tags } from "@lezer/highlight";
import { Vim, vim } from "@replit/codemirror-vim";
import { basicSetup, EditorView } from "codemirror";

import { editorKeyOwner } from "../../logic/keys/editorKeyOwner";
import type { ShortcutTable } from "../../logic/keys/shortcutTable";
import type { EditorEngine, EditorId, EditorSlot, EditorSurface, EditorSurfaceOptions, LineEnding } from "../engine";
import { parseMarkdown } from "./markdownAst";

/** Markdown の色付け。色はトークンだけ(ライト・ダークに追従する)。 */
const highlight = HighlightStyle.define([
  { tag: tags.heading, fontWeight: "var(--font-weight-strong)" },
  { tag: tags.strong, fontWeight: "var(--font-weight-strong)" },
  { tag: tags.emphasis, fontStyle: "italic" },
  { tag: tags.strikethrough, textDecoration: "line-through" },
  { tag: [tags.quote, tags.link, tags.url, tags.monospace], color: "var(--color-muted)" },
  { tag: [tags.processingInstruction, tags.contentSeparator, tags.meta, tags.comment], color: "var(--color-muted)" },
]);

/** 地は置き場所(T83b)が塗る。ここは書体・色・選択・行番号だけ。 */
const theme = EditorView.theme({
  "&": { height: "100%", color: "var(--color-fg)", backgroundColor: "transparent" },
  "&.cm-focused": { outline: "none" },
  ".cm-scroller": { fontFamily: "var(--font-family-mono)", lineHeight: "var(--md-line-height)" },
  ".cm-content": { caretColor: "var(--color-fg)" },
  ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--color-fg)" },
  "&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, ::selection": {
    backgroundColor: "var(--color-text-selection)",
  },
  ".cm-gutters": { backgroundColor: "transparent", color: "var(--color-muted)", border: "none" },
  ".cm-activeLine, .cm-activeLineGutter": { backgroundColor: "var(--color-control-hover)" },
  ".cm-panels": { backgroundColor: "var(--color-surface)", color: "var(--color-fg)" },
});

/** 部品ごとの本物。Vim の `:w` から部品を引く。 */
const surfacesByView = new WeakMap<EditorView, CodeMirrorSurface>();
let vimWriteDefined = false;

/** Vim の `:w`(`:write`)を保存にする。Vim の ex のコマンドは全体で 1 つなので 1 回だけ定める。 */
function defineVimWrite(): void {
  if (vimWriteDefined) return;
  vimWriteDefined = true;
  Vim.defineEx("write", "w", (cm) => {
    surfacesByView.get(cm.cm6)?.requestSave();
  });
}

class CodeMirrorSurface implements EditorSurface {
  readonly element: HTMLElement;
  readonly #view: EditorView;
  readonly #shortcuts: () => ShortcutTable;
  readonly #vim = new Compartment();
  readonly #readOnly = new Compartment();
  readonly #changeListeners: (() => void)[] = [];
  readonly #saveListeners: (() => void)[] = [];
  #lineEnding: LineEnding;
  #vimEnabled: boolean;
  #readOnlyEnabled: boolean;
  #slot: EditorSlot | null = null;
  #disposed = false;

  constructor(options: EditorSurfaceOptions) {
    this.#shortcuts = options.shortcuts;
    this.#lineEnding = options.lineEnding;
    this.#vimEnabled = options.vim;
    this.#readOnlyEnabled = options.readOnly;
    this.element = document.createElement("div");
    this.element.className = "editor-surface";
    this.element.style.fontSize = `${options.fontSize}px`;
    this.element.addEventListener("keydown", this.#onKeyDown, { capture: true });
    this.#view = new EditorView({ state: this.#createState(options.text, 0), parent: this.element });
    surfacesByView.set(this.#view, this);
    defineVimWrite();
  }

  #extensions(): Extension[] {
    return [
      // Vim は basicSetup より前に置き、キーを先に受けさせる(mdslide と同じ)
      this.#vim.of(this.#vimEnabled ? vim() : []),
      syntaxHighlighting(highlight),
      basicSetup,
      markdown({ base: markdownLanguage }),
      theme,
      EditorView.lineWrapping,
      this.#readOnly.of(EditorState.readOnly.of(this.#readOnlyEnabled)),
      EditorView.updateListener.of((update) => {
        if (!update.docChanged) return;
        for (const listener of this.#changeListeners) listener();
      }),
    ];
  }

  #createState(text: string, cursor: number): EditorState {
    const state = EditorState.create({ doc: text, extensions: this.#extensions() });
    const pos = Math.min(cursor, state.doc.length);
    return pos === 0 ? state : state.update({ selection: EditorSelection.cursor(pos) }).state;
  }

  /** 捕捉の段階で見る(CodeMirror と Vim より先)。 */
  readonly #onKeyDown = (event: KeyboardEvent) => {
    switch (editorKeyOwner(this.#shortcuts(), event)) {
      case "app":
        // メニューのキー。CodeMirror に渡さないが、既定の動作は止めない(OS のメニューへ)
        event.stopPropagation();
        return;
      case "save":
        event.preventDefault();
        event.stopPropagation();
        this.requestSave();
        return;
      case "editor":
        return;
    }
  };

  requestSave(): void {
    if (this.#disposed) return;
    for (const listener of this.#saveListeners) listener();
  }

  attach(slot: EditorSlot): void {
    if (this.#disposed || this.#slot === slot) return;
    this.#slot?.removeChild(this.element);
    slot.appendChild(this.element);
    this.#slot = slot;
    // 付け直すと寸法を測り直す必要がある(隠していたあいだは測れない)
    this.#view.requestMeasure();
  }

  detach(): void {
    if (this.#slot === null) return;
    this.#slot.removeChild(this.element);
    this.#slot = null;
  }

  setText(text: string, lineEnding: LineEnding): void {
    if (this.#disposed) return;
    this.#lineEnding = lineEnding;
    const state = this.#view.state;
    const line = state.doc.lineAt(state.selection.main.head).number;
    const next = EditorState.create({ doc: text, extensions: [] }).doc;
    const cursor = next.line(Math.min(line, next.lines)).from;
    this.#view.setState(this.#createState(text, cursor));
  }

  text(): string {
    const doc = this.#view.state.doc;
    return doc.sliceString(0, doc.length, this.#lineEnding === "crlf" ? "\r\n" : "\n");
  }

  setFontSize(px: number): void {
    if (this.#disposed) return;
    this.element.style.fontSize = `${px}px`;
    this.#view.requestMeasure();
  }

  setVim(enabled: boolean): void {
    if (this.#disposed || enabled === this.#vimEnabled) return;
    this.#vimEnabled = enabled;
    this.#view.dispatch({ effects: this.#vim.reconfigure(enabled ? vim() : []) });
  }

  setReadOnly(readOnly: boolean): void {
    if (this.#disposed || readOnly === this.#readOnlyEnabled) return;
    this.#readOnlyEnabled = readOnly;
    this.#view.dispatch({ effects: this.#readOnly.reconfigure(EditorState.readOnly.of(readOnly)) });
  }

  focus(): void {
    if (this.#disposed || !this.element.isConnected) return;
    this.#view.focus();
  }

  hasFocus(): boolean {
    return !this.#disposed && this.#view.hasFocus;
  }

  selectAll(): void {
    if (this.#disposed) return;
    selectAll(this.#view);
  }

  onChange(listener: () => void): void {
    this.#changeListeners.push(listener);
  }

  onSave(listener: () => void): void {
    this.#saveListeners.push(listener);
  }

  dispose(): void {
    if (this.#disposed) return;
    this.#disposed = true;
    this.detach();
    this.element.removeEventListener("keydown", this.#onKeyDown, { capture: true });
    surfacesByView.delete(this.#view);
    this.#view.destroy();
  }
}

/** CodeMirror のエンジン。EditorManager の既定の読み込みが使う。 */
export function createCodeMirrorEngine(): EditorEngine {
  return {
    createSurface: (_id: EditorId, options: EditorSurfaceOptions) => new CodeMirrorSurface(options),
    parseMarkdown,
  };
}
