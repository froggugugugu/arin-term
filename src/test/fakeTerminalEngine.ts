// テスト用の偽のターミナルエンジン。作ったサーフェスと、サーフェスへの操作を記録する。
// xterm.js も DOM も使わない(Vitest は node の環境で動く)。

import type { RendererKind, TerminalEngine, TerminalId, TerminalSize, TerminalSurface } from "../terminal/engine";

export interface FakeSurface extends TerminalSurface {
  readonly id: TerminalId;
  /** write に渡された順のデータ。 */
  readonly written: Uint8Array[];
  /** fit が呼ばれた回数。 */
  fitCount: number;
  /** dispose が呼ばれた回数。 */
  disposeCount: number;
  /** いまの行・桁数(既定は xterm.js と同じ 24 × 80)。 */
  current: TerminalSize;
  /** 次の fit で変わる先の行・桁数。変われば寸法の受け取り手へ流す(xterm.js の onResize と同じ)。 */
  fitTo: TerminalSize | null;
  /** getSelection が返す選択の文字列(既定は空 = 選択なし)。 */
  selection: string;
  /** hasFocus が返す値(既定は false)。 */
  focused: boolean;
  /** paste に渡された順の文字列。 */
  readonly pasted: string[];
  /** いまの表示(作った直後は true)。 */
  visible: boolean;
  /** setVisible に渡された順の値(同じ値の二重の呼び出しも残す)。 */
  readonly visibleCalls: boolean[];
  /** renderer が返す描画方式(既定は dom)。 */
  rendererKind: RendererKind;
  /** 描画方式を変え、受け取り手へ流す(WebGL から DOM へ戻ったときの真似)。 */
  emitRendererChange(renderer: RendererKind): void;
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
      const rendererListeners = new Set<(renderer: RendererKind) => void>();
      const surface: FakeSurface = {
        id,
        // node の環境には DOM が無い。テストでは要素の中身を見ないので、目印のオブジェクトで代える。
        element: { fakeTerminalId: id } as unknown as HTMLElement,
        written: [],
        fitCount: 0,
        disposeCount: 0,
        current: { rows: 24, cols: 80 },
        fitTo: null,
        selection: "",
        focused: false,
        pasted: [],
        visible: true,
        visibleCalls: [],
        rendererKind: "dom",
        write(data) {
          // 隠している間も取り込む(xterm.js と同じ)。
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
          const next = surface.fitTo;
          if (next === null) return;
          if (next.rows === surface.current.rows && next.cols === surface.current.cols) return;
          surface.current = next;
          surface.emitResize(next.cols, next.rows);
        },
        size() {
          return surface.current;
        },
        setVisible(visible) {
          surface.visibleCalls.push(visible);
          surface.visible = visible;
        },
        renderer() {
          return surface.rendererKind;
        },
        onRendererChange(listener) {
          rendererListeners.add(listener);
          return () => {
            rendererListeners.delete(listener);
          };
        },
        emitRendererChange(renderer) {
          surface.rendererKind = renderer;
          for (const listener of [...rendererListeners]) listener(renderer);
        },
        getSelection() {
          return surface.selection;
        },
        paste(text) {
          // xterm.js の paste と同じく、貼り付けた文字列は入力の受け取り手へ流れる(改行の変換と囲みは xterm.js の仕事なので真似ない)。
          surface.pasted.push(text);
          surface.emitInput(text);
        },
        hasFocus() {
          return surface.focused;
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
