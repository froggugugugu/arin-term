// xterm.js による TerminalEngine の実装(設計書 §5.1 の XtermEngine / XtermSurface)。
// @xterm/* を import してよいのはこのフォルダだけ。Rust との通信(src/ipc)は知らない(設計書 §3.2)。
// 描画は DOM(xterm.js の既定)か WebGL(@xterm/addon-webgl)。どちらにするかは起動時の指定(M1 の検証用。T27 で固定)。
// 切り替えの判断は ../rendererControl.ts、ここは WebGL のアドオンの付け外しだけを行う。

import "@xterm/xterm/css/xterm.css";

import { FitAddon } from "@xterm/addon-fit";
import { UnicodeGraphemesAddon } from "@xterm/addon-unicode-graphemes";
import { Unicode11Addon } from "@xterm/addon-unicode11";
import { WebglAddon } from "@xterm/addon-webgl";
import { Terminal } from "@xterm/xterm";

import { terminalKeyOwner } from "../../logic/keys/clipboard";
import type {
  RendererKind,
  TerminalEngine,
  TerminalId,
  TerminalSize,
  TerminalSurface,
  UnicodeWidth,
} from "../engine";
import { type RendererBackend, RendererControl, type WebglRendererHandle } from "../rendererControl";

export type { RendererKind, UnicodeWidth };

/** スクロールバックの行数(FR-043、PRD C-09 で確定)。 */
export const SCROLLBACK_LINES = 10_000;

export interface XtermEngineOptions {
  /**
   * 全角幅と絵文字の幅の決め方。既定は `unicode11`。T26 の実機確認で比べてどちらかに決める。
   * - `unicode11`: Unicode 11 の幅の表(`@xterm/addon-unicode11`)
   * - `graphemes`: Unicode 15 の幅と書記素クラスタ(`@xterm/addon-unicode-graphemes`。結合絵文字を 1 文字に扱う)
   */
  readonly unicode?: UnicodeWidth;
  /** 描画方式。既定は `dom`。`webgl` を作れない・失ったときは DOM に戻して reportError に報告する。 */
  readonly renderer?: RendererKind;
  /** 描画方式を DOM に戻したことの報告先。既定は console.error。 */
  readonly reportError?: (message: string) => void;
}

export class XtermEngine implements TerminalEngine {
  readonly #unicode: UnicodeWidth;
  readonly #renderer: RendererKind;
  readonly #reportError: (message: string) => void;

  constructor(options: XtermEngineOptions = {}) {
    this.#unicode = options.unicode ?? "unicode11";
    this.#renderer = options.renderer ?? "dom";
    this.#reportError = options.reportError ?? ((message) => console.error(message));
  }

  createSurface(id: TerminalId): TerminalSurface {
    return new XtermSurface(id, this.#unicode, this.#renderer, this.#reportError);
  }
}

class XtermSurface implements TerminalSurface {
  readonly element: HTMLElement;
  readonly #terminal: Terminal;
  readonly #fitAddon = new FitAddon();
  /** 取り込みの完了を待っている write。破棄のときに 0 で解決する。 */
  readonly #pending = new Set<(bytes: number) => void>();
  readonly #renderer: RendererControl;
  #visible = true;
  #disposed = false;

  constructor(
    id: TerminalId,
    unicode: UnicodeWidth,
    renderer: RendererKind,
    reportError: (message: string) => void,
  ) {
    // xterm.js の要素を入れる箱。UI はこの箱を置き場所に入れるだけ(ADR-003)。
    // 寸法合わせは箱の大きさを見るので、置き場所いっぱいに広げる。
    this.element = document.createElement("div");
    this.element.style.width = "100%";
    this.element.style.height = "100%";

    // `unicode` の API が proposed 扱いのため allowProposedApi が要る。
    this.#terminal = new Terminal({
      scrollback: SCROLLBACK_LINES,
      allowProposedApi: true,
      // TUI がマウスを取っているあいだも、Option を押しながらのドラッグで文字を選択できるようにする
      // (macOS では既定だと選択を強制する手段が無く、コピーができない)。
      macOptionClickForcesSelection: true,
    });
    this.#terminal.loadAddon(this.#fitAddon);
    loadUnicodeAddon(this.#terminal, unicode);
    // ⌘C・⌘V は xterm.js に処理させず、取り消しもしない。WKWebView はキーを画面に渡したあと、
    // 取り消されていなければメニューの「編集」の項目へ回し、それが画面のコピー・ペーストのイベントになる(設計書 §9.1)。
    // ほかのキー(Ctrl の組み合わせ、Option)は xterm.js の既定のまま。
    this.#terminal.attachCustomKeyEventHandler((event) => terminalKeyOwner(event) === "terminal");
    this.#terminal.open(this.element);
    // WebGL のアドオンは端末を開いたあとに読み込む(開く前だと、開くまで描画の部品を作らない)。
    this.#renderer = new RendererControl({
      requested: renderer,
      backend: webglBackend(this.#terminal, this.element),
      label: id,
      reportError,
    });
    this.#renderer.show();
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
    if (this.#disposed || !this.#visible || !this.element.isConnected) return;
    this.#fitAddon.fit();
  }

  setVisible(visible: boolean): void {
    if (this.#disposed || this.#visible === visible) return;
    this.#visible = visible;
    if (visible) {
      this.element.style.display = "";
      this.#renderer.show();
      this.fit();
    } else {
      // 要素を隠すと、xterm.js は画面に出ていないことを IntersectionObserver で知り、描画を止める
      // (取り込みは続け、表示に戻ったときに全体を描き直す。xterm.js 6.0.0 の RenderService の _isPaused)。
      // DOM の描画で手放すものはそれだけ(行の要素は数十個で、描画を止めれば処理はしない)。
      // WebGL の描画はアドオンごと破棄してコンテキストを返す(同時に持てる数に上限があるため。設計書 §2.2)。
      this.element.style.display = "none";
      this.#renderer.hide();
    }
  }

  renderer(): RendererKind {
    return this.#renderer.active;
  }

  onRendererChange(listener: (renderer: RendererKind) => void): () => void {
    return this.#renderer.onChange(listener);
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

  focus(): void {
    if (this.#disposed || !this.#visible) return;
    this.#terminal.focus();
  }

  dispose(): void {
    if (this.#disposed) return;
    this.#disposed = true;
    // 取り込み前に破棄した分は、取り込まれなかったものとして 0 で返す(待ち続けさせない)。
    for (const settle of [...this.#pending]) settle(0);
    this.#renderer.dispose();
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

/** WebGL のアドオンを付け外しする(判断は RendererControl)。 */
function webglBackend(terminal: Terminal, element: HTMLElement): RendererBackend {
  return {
    loadWebgl(onContextLoss): WebglRendererHandle {
      const before = new Set(element.querySelectorAll("canvas"));
      const addon = new WebglAddon();
      // WebGL2 のコンテキストを作れなければ、ここで例外になる(RendererControl が DOM に戻す)。
      // xterm.js は読み込みに失敗したアドオンも一覧に残すので、作りかけの分ごと破棄してから投げ直す。
      try {
        terminal.loadAddon(addon);
      } catch (cause) {
        addon.dispose();
        throw cause;
      }
      addon.onContextLoss(onContextLoss);
      // アドオンが置いた描画用の canvas(リンクの下線用の 2D の canvas も含む)。
      const added = [...element.querySelectorAll("canvas")].filter((canvas) => !before.has(canvas));
      let disposed = false;
      return {
        dispose() {
          if (disposed) return;
          disposed = true;
          // 破棄すると xterm.js は DOM の描画に戻り、canvas を外す(addon-webgl 0.19.0 の WebglAddon・WebglRenderer)。
          addon.dispose();
          // コンテキストの片付けはガベージコレクションまで待たされるので、ここで明示的に返す
          // (WebKit は同時に持てる WebGL のコンテキストの数に上限があり、超えると古いものから失われる)。
          // 2D の canvas には webgl2 のコンテキストは無いので null が返り、何もしない。
          for (const canvas of added) {
            canvas.getContext("webgl2")?.getExtension("WEBGL_lose_context")?.loseContext();
          }
        },
      };
    },
  };
}
