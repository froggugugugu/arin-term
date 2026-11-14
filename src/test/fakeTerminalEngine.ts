// テスト用の偽のターミナルエンジン。作ったサーフェスと、サーフェスへの操作を記録する。
// xterm.js も DOM も使わない(Vitest は node の環境で動く)。

import type { TerminalEngine, TerminalId, TerminalSurface } from "../terminal/engine";

export interface FakeSurface extends TerminalSurface {
  readonly id: TerminalId;
  /** write に渡された順のデータ。 */
  readonly written: Uint8Array[];
  /** fit が呼ばれた回数。 */
  fitCount: number;
  /** dispose が呼ばれた回数。 */
  disposeCount: number;
  /** 登録された入力の受け取り手へ、キー入力を流す。 */
  emitInput(data: string): void;
  /** 登録された寸法の受け取り手へ、寸法の変更を流す。 */
  emitResize(cols: number, rows: number): void;
}

export interface FakeTerminalEngine extends TerminalEngine {
  /** 作った順のサーフェス。 */
  readonly created: FakeSurface[];
  /** 次の createSurface で作るサーフェスの dispose を失敗させる(破棄の異常系のテスト用)。 */
  failDisposeOnNext: boolean;
}

export function createFakeTerminalEngine(): FakeTerminalEngine {
  const created: FakeSurface[] = [];
  const engine: FakeTerminalEngine = {
    created,
    failDisposeOnNext: false,
    createSurface(id) {
      const failDispose = engine.failDisposeOnNext;
      engine.failDisposeOnNext = false;
      const inputListeners: ((data: string) => void)[] = [];
      const resizeListeners: ((cols: number, rows: number) => void)[] = [];
      const surface: FakeSurface = {
        id,
        // node の環境には DOM が無い。テストでは要素の中身を見ないので、目印のオブジェクトで代える。
        element: { fakeTerminalId: id } as unknown as HTMLElement,
        written: [],
        fitCount: 0,
        disposeCount: 0,
        write(data) {
          surface.written.push(data);
          return Promise.resolve(surface.disposeCount > 0 ? 0 : data.byteLength);
        },
        onInput(listener) {
          inputListeners.push(listener);
        },
        onResize(listener) {
          resizeListeners.push(listener);
        },
        fit() {
          surface.fitCount += 1;
        },
        dispose() {
          surface.disposeCount += 1;
          if (failDispose) throw new Error(`偽のサーフェス ${id} の破棄に失敗した`);
        },
        emitInput(data) {
          for (const listener of inputListeners) listener(data);
        },
        emitResize(cols, rows) {
          for (const listener of resizeListeners) listener(cols, rows);
        },
      };
      created.push(surface);
      return surface;
    },
  };
  return engine;
}
