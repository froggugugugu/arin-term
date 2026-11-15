// ターミナルエンジンの抽象(設計書 §5.1、PRD §7 C4)。
// 画面と TerminalManager はこの形だけを知り、xterm.js(src/terminal/xterm/)を直接使わない。
// このファイルは @xterm/* を import しない。src/ipc からは Rust から生成した型だけを使う。

import type { RendererKind } from "../ipc/generated/RendererKind";
import type { TerminalId } from "../ipc/generated/TerminalId";
import type { UnicodeWidth } from "../ipc/generated/UnicodeWidth";

/**
 * 描画方式(`dom` / `webgl`)と全角幅の方式(`unicode11` / `graphemes`)。型は Rust の起動時の指定
 * (`arin_core::launch_options`)から生成したもの。既定は DOM と unicode11(ADR-001)。複数枚で実測する M4 で決める。
 */
export type { RendererKind, TerminalId, UnicodeWidth };

/** 行・桁数。 */
export interface TerminalSize {
  readonly rows: number;
  readonly cols: number;
}

/** ターミナル 1 枚ぶんの表示と入出力。1 つの ID に 1 つ。 */
export interface TerminalSurface {
  /** 置き場所に入れる要素。React の管理の外に置き、再描画で作り直さない(ADR-003)。 */
  readonly element: HTMLElement;
  /**
   * PTY の出力を取り込む。取り込みが済んだら、そのバイト数で解決する(流量制御に使う。設計書 §1.3 #3)。
   * 渡した配列は取り込みが済むまで書き換えない。破棄の後は 0 で解決する。
   */
  write(data: Uint8Array): Promise<number>;
  /** キー入力・貼り付けで PTY へ送る文字列を受け取る。 */
  onInput(listener: (data: string) => void): void;
  /**
   * PTY へそのまま送るバイト列を受け取る(xterm.js の onBinary。古い形式のマウスの報告など)。
   * 文字列の各文字の値(0〜255)が 1 バイトを表す。
   */
  onBinary(listener: (data: string) => void): void;
  /** 行・桁数が変わったときに受け取る(FR-043 のリサイズ追従)。 */
  onResize(listener: (cols: number, rows: number) => void): void;
  /**
   * 行・桁数を指定して変える(置き場所の大きさを測らない)。隠しているときも変える。
   * 変われば onResize の受け取り手に届く。隠したまま作ったサーフェスを、表示中のターミナルの寸法で始めるのに使う(設計書 §2.2)。
   */
  resize(size: TerminalSize): void;
  /** 置き場所の大きさに合わせて行・桁数を決め直す。置き場所が画面に無いとき・隠しているときは何もしない。 */
  fit(): void;
  /**
   * 表示と非表示を切り替える(設計書 §5.1、§2.2「非表示のインスタンス」。NFR-009)。作った直後は表示。
   * - false: 要素を隠し、描画の資源を手放す(WebGL なら描画のアドオンを破棄してコンテキストを返す)
   * - true: 要素を戻し、描画の資源を持ち直して、寸法を合わせ直す
   * 隠している間も write は取り込みを続け、取り込みが済んだバイト数で解決する。同じ値で呼んでも何もしない。
   */
  setVisible(visible: boolean): void;
  /**
   * いまの描画方式(表示しているときに使う方式)。WebGL を指定しても作れなかった・失ったときは `dom` になる
   * (そのときは診断に報告してある)。
   */
  renderer(): RendererKind;
  /** 描画方式が変わったときに受け取る(WebGL から DOM へ戻ったとき)。外す関数を返す。 */
  onRendererChange(listener: (renderer: RendererKind) => void): () => void;
  /** いまの行・桁数。 */
  size(): TerminalSize;
  /** 選択している文字列。選択が無ければ空文字列。 */
  getSelection(): string;
  /** 画面とスクロールバックの全体を選択する(メニューの「すべてを選択」)。破棄の後は何もしない。 */
  selectAll(): void;
  /**
   * 文字列を貼り付ける。PTY への入力として onInput の受け取り手に届く。
   * 改行を CR にそろえ、ブラケットペーストのモードならその印で囲む(xterm.js の paste)。
   */
  paste(text: string): void;
  /** 入力のフォーカスがこのサーフェスの中にあるか(コピー・ペーストの振り分けに使う。設計書 §9.1)。 */
  hasFocus(): boolean;
  /** 入力のフォーカスをこのサーフェスへ移す。隠しているとき・破棄の後は何もしない。 */
  focus(): void;
  /** 資源を手放す。2 回目以降は何もしない。 */
  dispose(): void;
}

/** サーフェスを作るもの。差し替えの単位(PRD §7 C4)。 */
export interface TerminalEngine {
  createSurface(id: TerminalId): TerminalSurface;
}
