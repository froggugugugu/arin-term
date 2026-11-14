// ターミナルエンジンの抽象(設計書 §5.1、PRD §7 C4)。
// 画面と TerminalManager はこの形だけを知り、xterm.js(src/terminal/xterm/)を直接使わない。
// このファイルは @xterm/* も src/ipc も import しない。

/**
 * ターミナルの ID。Rust 側の `TerminalId` の文字列表現。
 * 【仮定】Rust から生成した型(src/ipc/generated/)が入るまでは文字列で持つ(T18 で差し替える)。
 */
export type TerminalId = string;

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
  /** 行・桁数が変わったときに受け取る(FR-043 のリサイズ追従)。 */
  onResize(listener: (cols: number, rows: number) => void): void;
  /** 置き場所の大きさに合わせて行・桁数を決め直す。置き場所が画面に無いときは何もしない。 */
  fit(): void;
  /** 資源を手放す。2 回目以降は何もしない。 */
  dispose(): void;
}

/** サーフェスを作るもの。差し替えの単位(PRD §7 C4)。 */
export interface TerminalEngine {
  createSurface(id: TerminalId): TerminalSurface;
}
