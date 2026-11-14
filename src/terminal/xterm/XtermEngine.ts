// xterm.js による TerminalEngine の実装(設計書 §5.1 の XtermEngine / XtermSurface)。
// @xterm/* を import してよいのはこのフォルダだけ。Rust との通信(src/ipc)は知らない(設計書 §3.2)。
// 描画は DOM(xterm.js の既定)。WebGL は T21 で RendererKind に足す。

import "@xterm/xterm/css/xterm.css";

import { FitAddon } from "@xterm/addon-fit";
import { UnicodeGraphemesAddon } from "@xterm/addon-unicode-graphemes";
import { Unicode11Addon } from "@xterm/addon-unicode11";
import { Terminal } from "@xterm/xterm";

import { terminalKeyOwner } from "../../logic/keys/clipboard";
import type { TerminalEngine, TerminalId, TerminalSize, TerminalSurface } from "../engine";

/** スクロールバックの行数(FR-043、PRD C-09 で確定)。 */
export const SCROLLBACK_LINES = 10_000;

/**
 * 全角幅と絵文字の幅の決め方。T26 の実機確認で比べてどちらかに決める。
 * - `unicode11`: Unicode 11 の幅の表(`@xterm/addon-unicode11`)
 * - `graphemes`: Unicode 15 の幅と書記素クラスタ(`@xterm/addon-unicode-graphemes`。結合絵文字を 1 文字に扱う)
 */
export type UnicodeWidth = "unicode11" | "graphemes";

/** 描画方式。いまは DOM だけ(T21 で `webgl` を足す)。 */
export type RendererKind = "dom";

export interface XtermEngineOptions {
  /** 既定は `unicode11`。 */
  readonly unicode?: UnicodeWidth;
  /** 既定は `dom`。 */
  readonly renderer?: RendererKind;
}

export class XtermEngine implements TerminalEngine {
  readonly #unicode: UnicodeWidth;
  readonly #renderer: RendererKind;

  constructor(options: XtermEngineOptions = {}) {
    this.#unicode = options.unicode ?? "unicode11";
    this.#renderer = options.renderer ?? "dom";
  }

  createSurface(_id: TerminalId): TerminalSurface {
    return new XtermSurface(this.#unicode, this.#renderer);
  }
}

class XtermSurface implements TerminalSurface {
  readonly element: HTMLElement;
  readonly #terminal: Terminal;
  readonly #fitAddon = new FitAddon();
  /** 取り込みの完了を待っている write。破棄のときに 0 で解決する。 */
  readonly #pending = new Set<(bytes: number) => void>();
  #disposed = false;

  constructor(unicode: UnicodeWidth, renderer: RendererKind) {
    // xterm.js の要素を入れる箱。UI はこの箱を置き場所に入れるだけ(ADR-003)。
    // 寸法合わせは箱の大きさを見るので、置き場所いっぱいに広げる。
    this.element = document.createElement("div");
    this.element.style.width = "100%";
    this.element.style.height = "100%";

    // `unicode` の API が proposed 扱いのため allowProposedApi が要る。
    this.#terminal = new Terminal({ scrollback: SCROLLBACK_LINES, allowProposedApi: true });
    this.#terminal.loadAddon(this.#fitAddon);
    loadUnicodeAddon(this.#terminal, unicode);
    loadRenderer(this.#terminal, renderer);
    // ⌘C・⌘V は xterm.js に処理させず、取り消しもしない。WKWebView はキーを画面に渡したあと、
    // 取り消されていなければメニューの「編集」の項目へ回し、それが画面のコピー・ペーストのイベントになる(設計書 §9.1)。
    // ほかのキー(Ctrl の組み合わせ、Option)は xterm.js の既定のまま。
    this.#terminal.attachCustomKeyEventHandler((event) => terminalKeyOwner(event) === "terminal");
    this.#terminal.open(this.element);
  }

  write(data: Uint8Array): Promise<number> {
    if (this.#disposed) return Promise.resolve(0);
    const bytes = data.byteLength;
    return new Promise((resolve) => {
      const settle = (processed: number) => {
        this.#pending.delete(settle);
        resolve(processed);
      };
      this.#pending.add(settle);
      // コールバックはパーサーが data を取り込み終えたときに呼ばれる。
      this.#terminal.write(data, () => settle(bytes));
    });
  }

  onInput(listener: (data: string) => void): void {
    this.#terminal.onData(listener);
  }

  onResize(listener: (cols: number, rows: number) => void): void {
    this.#terminal.onResize(({ cols, rows }) => listener(cols, rows));
  }

  fit(): void {
    if (this.#disposed || !this.element.isConnected) return;
    this.#fitAddon.fit();
  }

  size(): TerminalSize {
    return { rows: this.#terminal.rows, cols: this.#terminal.cols };
  }

  getSelection(): string {
    if (this.#disposed) return "";
    return this.#terminal.getSelection();
  }

  paste(text: string): void {
    if (this.#disposed) return;
    // 改行の CR へのそろえと、ブラケットペーストのモード(DECSET 2004)のときの囲みは xterm.js が行う。
    this.#terminal.paste(text);
  }

  hasFocus(): boolean {
    const active = this.element.ownerDocument.activeElement;
    return !this.#disposed && active !== null && this.element.contains(active);
  }

  dispose(): void {
    if (this.#disposed) return;
    this.#disposed = true;
    // 取り込み前に破棄した分は、取り込まれなかったものとして 0 で返す(待ち続けさせない)。
    for (const settle of [...this.#pending]) settle(0);
    // アドオンと onData・onResize の購読も、端末の破棄でまとめて外れる。
    this.#terminal.dispose();
    this.element.remove();
  }
}

function loadUnicodeAddon(terminal: Terminal, unicode: UnicodeWidth): void {
  switch (unicode) {
    case "unicode11":
      terminal.loadAddon(new Unicode11Addon());
      terminal.unicode.activeVersion = "11";
      return;
    case "graphemes":
      // 読み込むと自分で activeVersion を切り替える。
      terminal.loadAddon(new UnicodeGraphemesAddon());
      return;
  }
}

function loadRenderer(_terminal: Terminal, renderer: RendererKind): void {
  switch (renderer) {
    case "dom":
      // xterm.js の既定の描画。アドオンは要らない。
      return;
  }
}
