// エディタの ID と部品の対応(ADR-015 の 1・6・7)。src/terminal/TerminalManager.ts にならう。
//
// - 1 つの ID に 1 つの部品を持ち続ける。タブを替えて置き場所から外しても破棄しない(取り消しの履歴とカーソルが残る)。
//   破棄するのは close・retain(写しから消えた)・dispose のときだけ
// - 部品の置き場所(パネルの要素)への付け外しもここで行い、React には要素を管理させない(ADR-003)
// - エンジン(CodeMirror)は初めて開くときに動的 import で読む(起動とターミナルだけの使い方に重さを乗せない)
// - 未保存: 保存済みの文書(開いた・保存した・読み直した文書)といまの文書を比べ、変わり目だけを onDirtyChange で知らせる
//   (Rust へは変わり目だけ送る。ADR-015 の 4 の set_document_dirty)。保存のキー・`:w` は onSave で知らせる
// - プレビュー: 見えているエディタだけ、最後の変更から 150 ms 後に解析する(src/logic/editor/previewSchedule)
// - 文字の大きさ・Vim は全部の部品へ当て、新しく作る部品もその値で作る
//
// 通信(保存・dirty の送信)は持たない。呼び出し側(T83a)が onDirtyChange・onSave で受けて src/ipc を呼ぶ。

import { describeError } from "../logic/describeError";
import { createPreviewSchedule, type PreviewSchedule, type PreviewTimer } from "../logic/editor/previewSchedule";
import type { ShortcutTable } from "../logic/keys/shortcutTable";
import type { EditorEngine, EditorId, EditorSlot, EditorSurface, LineEnding } from "./engine";
import type { MdBlock } from "./markdownTypes";

/** 開く文書(`open_document` の応答の `opened` から作る)。 */
export interface EditorDocument {
  readonly text: string;
  readonly lineEnding: LineEnding;
  readonly readOnly: boolean;
}

export interface EditorManagerOptions {
  /** いまのショートカットの表(部品がキーを押すたびに引く)。 */
  shortcuts: () => ShortcutTable;
  /** 未保存の有無が変わった。 */
  onDirtyChange: (id: EditorId, dirty: boolean) => void;
  /** 保存のキー・`:w` が押された。 */
  onSave: (id: EditorId) => void;
  /** プレビューの木を作り直した(見えているエディタだけ)。 */
  onPreview?: (id: EditorId, blocks: readonly MdBlock[]) => void;
  /** 本文の文字の大きさの初めの値(CSS ピクセル。ターミナルの文字の大きさ)。 */
  fontSize: number;
  /** Vim の初めの値(写しの `editor.vim`)。 */
  vim: boolean;
  /** エンジンを読む。既定は CodeMirror の動的 import。テストでは偽のエンジンを返す。 */
  loadEngine?: () => Promise<EditorEngine>;
  /** プレビューの予約の時計。既定は setTimeout。 */
  timer?: PreviewTimer;
  /** 読み込みの失敗の報告先。既定は console.error。 */
  reportError?: (message: string) => void;
}

const defaultLoadEngine = async (): Promise<EditorEngine> => {
  const module = await import("./codemirror/CodeMirrorEngine");
  return module.createCodeMirrorEngine();
};

const defaultTimer: PreviewTimer = (run, ms) => {
  const handle = setTimeout(run, ms);
  return () => clearTimeout(handle);
};

interface Entry {
  readonly surface: EditorSurface;
  /** 保存済みの文書。いまの文書と同じなら未保存でない。 */
  saved: string;
  dirty: boolean;
  preview: readonly MdBlock[] | null;
}

export class EditorManager {
  readonly #options: EditorManagerOptions;
  readonly #loadEngine: () => Promise<EditorEngine>;
  readonly #reportError: (message: string) => void;
  readonly #entries = new Map<EditorId, Entry>();
  /** 開いている途中の ID(読み込み中に閉じたら作らない)。 */
  readonly #opening = new Set<EditorId>();
  /** ID ごとの置き場所(部品を作る前に渡されたものも持つ)。 */
  readonly #slots = new Map<EditorId, EditorSlot>();
  /** 見えると伝えられたプレビュー(部品を作る前のものも持つ)。 */
  readonly #previewShown = new Set<EditorId>();
  readonly #preview: PreviewSchedule;
  #engine: Promise<EditorEngine> | null = null;
  #loadedEngine: EditorEngine | null = null;
  #fontSize: number;
  #vim: boolean;
  #disposed = false;

  constructor(options: EditorManagerOptions) {
    this.#options = options;
    this.#loadEngine = options.loadEngine ?? defaultLoadEngine;
    this.#reportError = options.reportError ?? ((message) => console.error(message));
    this.#fontSize = options.fontSize;
    this.#vim = options.vim;
    this.#preview = createPreviewSchedule((id) => this.#parse(id), options.timer ?? defaultTimer);
  }

  /** 部品を作る(エンジンを読んでいなければ読む)。既に開いていれば何もしない。読み込めなければ報告して失敗する。 */
  async open(id: EditorId, document: EditorDocument): Promise<void> {
    if (this.#disposed || this.#entries.has(id)) return;
    this.#opening.add(id);
    let engine: EditorEngine;
    try {
      engine = await this.#engineOnce();
    } catch (error) {
      this.#opening.delete(id);
      throw error;
    }
    // 読み込み中に閉じた・破棄した・同じ ID を先に開いた
    if (!this.#opening.delete(id) || this.#disposed || this.#entries.has(id)) return;
    const surface = engine.createSurface(id, {
      text: document.text,
      lineEnding: document.lineEnding,
      readOnly: document.readOnly,
      fontSize: this.#fontSize,
      vim: this.#vim,
      shortcuts: this.#options.shortcuts,
    });
    // 基準は部品が改行を揃えたあとの文書(生の document.text だと、改行が混ざった文書は戻しても未保存のまま。replaceText と同じ)
    const entry: Entry = { surface, saved: surface.text(), dirty: false, preview: null };
    this.#entries.set(id, entry);
    surface.onChange(() => {
      this.#updateDirty(id, entry);
      this.#preview.changed(id);
    });
    surface.onSave(() => this.#options.onSave(id));
    const slot = this.#slots.get(id);
    if (slot !== undefined) surface.attach(slot);
    // 開く前にプレビューが見えると伝えられていたら、作ったのですぐ解析する
    if (this.#previewShown.has(id)) this.#preview.setVisible(id, true);
  }

  #engineOnce(): Promise<EditorEngine> {
    if (this.#engine === null) {
      this.#engine = this.#loadEngine().then(
        (engine) => {
          this.#loadedEngine = engine;
          return engine;
        },
        (error: unknown) => {
          // 次に開くときにもう一度読む
          this.#engine = null;
          this.#reportError(`エディタの部品を読み込めませんでした: ${describeError(error)}`);
          throw error;
        },
      );
    }
    return this.#engine;
  }

  /** 部品があるか。 */
  has(id: EditorId): boolean {
    return this.#entries.has(id);
  }

  /** 部品の要素を置き場所に入れる。部品がまだ無ければ作ったときに入れる。外す関数を返す(ほかへ移したあとは何もしない)。 */
  attach(id: EditorId, slot: EditorSlot): () => void {
    this.#slots.set(id, slot);
    this.#entries.get(id)?.surface.attach(slot);
    return () => {
      if (this.#slots.get(id) !== slot) return;
      this.#slots.delete(id);
      this.#entries.get(id)?.surface.detach();
    };
  }

  /** いまの文書。部品が無ければ null。 */
  text(id: EditorId): string | null {
    return this.#entries.get(id)?.surface.text() ?? null;
  }

  /** 未保存の変更があるか。 */
  isDirty(id: EditorId): boolean {
    return this.#entries.get(id)?.dirty ?? false;
  }

  /** 保存できた。保存した文書(保存を始めたときの text())を基準にし直す。保存中に打った分は未保存のまま。 */
  markSaved(id: EditorId, savedText: string): void {
    const entry = this.#entries.get(id);
    if (entry === undefined) return;
    entry.saved = savedText;
    this.#updateDirty(id, entry);
  }

  /** 外での変更を読み直した。文書を差し替え(履歴は新しく始める)、未保存を消す。 */
  replaceText(id: EditorId, text: string, lineEnding: LineEnding): void {
    const entry = this.#entries.get(id);
    if (entry === undefined) return;
    entry.surface.setText(text, lineEnding);
    entry.saved = entry.surface.text();
    this.#updateDirty(id, entry);
    this.#preview.changed(id);
  }

  setReadOnly(id: EditorId, readOnly: boolean): void {
    this.#entries.get(id)?.surface.setReadOnly(readOnly);
  }

  /** 本文の文字の大きさを全部の部品へ当てる。今と同じ値なら何もしない。 */
  setFontSize(px: number): void {
    if (px === this.#fontSize) return;
    this.#fontSize = px;
    for (const entry of this.#entries.values()) entry.surface.setFontSize(px);
  }

  /** Vim を全部の部品で付け外しする(作り直さない)。今と同じ値なら何もしない。 */
  setVim(enabled: boolean): void {
    if (enabled === this.#vim) return;
    this.#vim = enabled;
    for (const entry of this.#entries.values()) entry.surface.setVim(enabled);
  }

  focus(id: EditorId): void {
    this.#entries.get(id)?.surface.focus();
  }

  /** 入力のフォーカスがある部品の ID。無ければ null。 */
  focusedId(): EditorId | null {
    for (const [id, entry] of this.#entries) if (entry.surface.hasFocus()) return id;
    return null;
  }

  /** フォーカスのある部品の全体を選ぶ(メニューの「すべてを選択」)。フォーカスのある部品が無ければ偽。 */
  selectAllFocused(): boolean {
    const id = this.focusedId();
    if (id === null) return false;
    this.#entries.get(id)?.surface.selectAll();
    return true;
  }

  /** プレビューが見えるか(並べて・プレビューの表示で、そのタブが表示中)。見えるようになったらすぐ解析する。 */
  setPreviewVisible(id: EditorId, visible: boolean): void {
    if (visible) this.#previewShown.add(id);
    else this.#previewShown.delete(id);
    if (this.#entries.has(id)) this.#preview.setVisible(id, visible);
  }

  /** いちばん新しいプレビューの木。まだ解析していなければ null。 */
  preview(id: EditorId): readonly MdBlock[] | null {
    return this.#entries.get(id)?.preview ?? null;
  }

  /** 部品を破棄して忘れる(タブを閉じた)。 */
  close(id: EditorId): void {
    this.#opening.delete(id);
    this.#previewShown.delete(id);
    this.#preview.forget(id);
    const entry = this.#entries.get(id);
    if (entry === undefined) return;
    this.#entries.delete(id);
    if (this.#slots.has(id)) entry.surface.detach();
    this.#slots.delete(id);
    entry.surface.dispose();
  }

  /** 写しに残っている ID のほかを閉じる。 */
  retain(ids: ReadonlySet<EditorId>): void {
    for (const id of [...this.#entries.keys(), ...this.#opening]) if (!ids.has(id)) this.close(id);
  }

  /** 全部の部品を手放し、以後は開かない。 */
  dispose(): void {
    if (this.#disposed) return;
    for (const id of [...this.#entries.keys(), ...this.#opening]) this.close(id);
    this.#disposed = true;
    this.#preview.dispose();
  }

  #updateDirty(id: EditorId, entry: Entry): void {
    const dirty = entry.surface.text() !== entry.saved;
    if (dirty === entry.dirty) return;
    entry.dirty = dirty;
    this.#options.onDirtyChange(id, dirty);
  }

  #parse(id: EditorId): void {
    const entry = this.#entries.get(id);
    const engine = this.#loadedEngine;
    if (entry === undefined || engine === null) return;
    entry.preview = engine.parseMarkdown(entry.surface.text());
    this.#options.onPreview?.(id, entry.preview);
  }
}
