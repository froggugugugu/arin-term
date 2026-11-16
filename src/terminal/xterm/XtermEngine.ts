// xterm.js による TerminalEngine の実装(設計書 §5.1 の XtermEngine / XtermSurface)。
// @xterm/* を import してよいのはこのフォルダだけ。Rust との通信(src/ipc)は知らない(設計書 §3.2)。
// 描画は xterm.js の既定の DOM 描画、全角幅は Unicode 11 の幅の表(@xterm/addon-unicode11)に固定する
// (M4 で複数枚を実測して決めた。ADR-001)。

import "@xterm/xterm/css/xterm.css";
import "./XtermEngine.css";

import { FitAddon } from "@xterm/addon-fit";
import { Unicode11Addon } from "@xterm/addon-unicode11";
import { Terminal } from "@xterm/xterm";

import { DEFAULT_SHORTCUT_TABLE } from "../../logic/keys/defaultShortcuts";
import { terminalKeyOwner } from "../../logic/keys/keyOwner";
import { isShiftEnter, newlineInput } from "../../logic/keys/newline";
import type { ShortcutTable } from "../../logic/keys/shortcutTable";
import type { SurfaceOptions, TerminalEngine, TerminalId, TerminalSize, TerminalSurface } from "../engine";

/** スクロールバックの行数(FR-043、PRD C-09 で確定)。 */
export const SCROLLBACK_LINES = 10_000;

/** xterm.js の地の色(完全な透明)。 */
const TRANSPARENT = "#00000000";

export interface XtermEngineOptions {
  /**
   * ショートカットの表を返す関数。キーを押すたびに呼び、受け口がメニューの項目・「編集」の項目のキーを xterm.js に処理させない。
   * 既定は src/logic/keys/defaultShortcuts.ts の見本。main.tsx は uiStore の表(起動時に `get_shortcuts` で受け取ったもの)を渡す。
   */
  readonly getShortcuts?: () => ShortcutTable;
}

export class XtermEngine implements TerminalEngine {
  readonly #getShortcuts: () => ShortcutTable;

  constructor(options: XtermEngineOptions = {}) {
    this.#getShortcuts = options.getShortcuts ?? (() => DEFAULT_SHORTCUT_TABLE);
  }

  createSurface(_id: TerminalId, options: SurfaceOptions = {}): TerminalSurface {
    return new XtermSurface(this.#getShortcuts, options);
  }
}

class XtermSurface implements TerminalSurface {
  readonly element: HTMLElement;
  readonly #terminal: Terminal;
  readonly #fitAddon = new FitAddon();
  /** 取り込みの完了を待っている write。破棄のときに 0 で解決する。 */
  readonly #pending = new Set<(bytes: number) => void>();
  #visible = true;
  #disposed = false;
  /** 隠しているあいだに求められた文字の大きさ。表示に戻すときに当てる(ADR-013 の 3)。 */
  #pendingFontSize: number | null = null;

  constructor(getShortcuts: () => ShortcutTable, options: SurfaceOptions) {
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
      // 地は透明にし、置き場所の地(--color-terminal-bg。透過の設定ぶん透かした黒)を見せる(ADR-014 の 2)。
      // 透過の値が変わっても xterm.js には何も送らない(CSS の変数だけが変わる)。色付きのセル・選択・カーソルは今のまま。
      allowTransparency: true,
      theme: { background: TRANSPARENT },
      // 指定が無ければ xterm.js の既定(15。ADR-013 の 1 の既定と同じ)のまま。
      ...(options.fontSize === undefined ? {} : { fontSize: options.fontSize }),
    });
    this.#terminal.loadAddon(this.#fitAddon);
    this.#terminal.loadAddon(new Unicode11Addon());
    this.#terminal.unicode.activeVersion = "11";
    // 表の受け口がメニューの項目・「編集」の項目のキー(⌘C・⌘V など)は xterm.js に処理させず、取り消しもしない。
    // WKWebView はキーを画面に渡したあと、取り消されていなければメニューの項目へ回し、「編集」の項目は画面の
    // コピー・切り取り・ペーストのイベントになる(設計書 §9.1)。ほかのキー(Ctrl の組み合わせ、Option)は xterm.js の既定のまま。
    // Shift+Enter(ADR-014 の 5)は ESC + CR を送り、xterm.js の既定の処理(CR)はさせない。keypress も止める。
    this.#terminal.attachCustomKeyEventHandler((event) => {
      if (isShiftEnter(event)) {
        const input = newlineInput(event);
        if (input !== null) {
          event.preventDefault();
          this.#terminal.input(input);
        }
        return false;
      }
      return terminalKeyOwner(getShortcuts(), event) === "terminal";
    });
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

  onBinary(listener: (data: string) => void): void {
    this.#terminal.onBinary(listener);
  }

  onResize(listener: (cols: number, rows: number) => void): void {
    this.#terminal.onResize(({ cols, rows }) => listener(cols, rows));
  }

  resize(size: TerminalSize): void {
    if (this.#disposed) return;
    if (size.rows === this.#terminal.rows && size.cols === this.#terminal.cols) return;
    // 変われば xterm.js が onResize を流す。
    this.#terminal.resize(size.cols, size.rows);
  }

  fit(): void {
    if (this.#disposed || !this.#visible || !this.element.isConnected) return;
    this.#fitAddon.fit();
  }

  setFontSize(px: number): void {
    if (this.#disposed) return;
    if (!this.#visible) {
      // display: none の中では xterm.js が文字の寸法を 0 で測りうるので、表示に戻すまで当てない。
      this.#pendingFontSize = px;
      return;
    }
    this.#pendingFontSize = null;
    this.#applyFontSize(px);
  }

  /** 文字の大きさを当て、置き場所に合わせて行・桁を決め直す(行・桁が変われば onResize が流れる)。 */
  #applyFontSize(px: number): void {
    if (this.#terminal.options.fontSize === px) return;
    this.#terminal.options.fontSize = px;
    this.fit();
  }

  setVisible(visible: boolean): void {
    if (this.#disposed || this.#visible === visible) return;
    this.#visible = visible;
    if (visible) {
      this.element.style.display = "";
      const pending = this.#pendingFontSize;
      this.#pendingFontSize = null;
      if (pending !== null) this.#applyFontSize(pending);
      this.fit();
    } else {
      // 要素を隠すと、xterm.js は画面に出ていないことを IntersectionObserver で知り、描画を止める
      // (取り込みは続け、表示に戻ったときに全体を描き直す。xterm.js 6.0.0 の RenderService の _isPaused)。
      // DOM の描画で手放すものはそれだけ(行の要素は数十個で、描画を止めれば処理はしない)。
      this.element.style.display = "none";
    }
  }

  size(): TerminalSize {
    return { rows: this.#terminal.rows, cols: this.#terminal.cols };
  }

  getSelection(): string {
    if (this.#disposed) return "";
    return this.#terminal.getSelection();
  }

  selectAll(): void {
    if (this.#disposed) return;
    this.#terminal.selectAll();
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
    // アドオンと onData・onResize の購読も、端末の破棄でまとめて外れる。
    this.#terminal.dispose();
    this.element.remove();
  }
}
