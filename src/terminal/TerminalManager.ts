// ターミナルの ID とサーフェスの対応(設計書 §5.1)。1 つの ID に 1 つのサーフェスを持ち続ける。
// 非表示のあいだも破棄しない(設計書 §1.3 #4)。破棄するのは呼び出し側が dispose を呼んだときだけ。
// エンジンは外から受け取る。本物は src/terminal/xterm/、テストでは src/test/fakeTerminalEngine.ts を渡す。

import type { TerminalEngine, TerminalId, TerminalSurface } from "./engine";

export class TerminalManager {
  readonly #engine: TerminalEngine;
  readonly #surfaces = new Map<TerminalId, TerminalSurface>();

  constructor(engine: TerminalEngine) {
    this.#engine = engine;
  }

  /** 持っているサーフェスの数。 */
  get size(): number {
    return this.#surfaces.size;
  }

  /** ID のサーフェスを返す。無ければ undefined。 */
  get(id: TerminalId): TerminalSurface | undefined {
    return this.#surfaces.get(id);
  }

  /** ID のサーフェスを返す。無ければエンジンで作って覚える。同じ ID で二重に作らない。 */
  getOrCreate(id: TerminalId): TerminalSurface {
    const existing = this.#surfaces.get(id);
    if (existing) return existing;
    const surface = this.#engine.createSurface(id);
    this.#surfaces.set(id, surface);
    return surface;
  }

  /**
   * ID のサーフェスを破棄し、対応を外す。持っていなければ何もせず false を返す。
   * サーフェスの dispose が例外を投げても、対応は先に外してあるので残らない(例外はそのまま伝える)。
   */
  dispose(id: TerminalId): boolean {
    const surface = this.#surfaces.get(id);
    if (!surface) return false;
    this.#surfaces.delete(id);
    surface.dispose();
    return true;
  }
}
