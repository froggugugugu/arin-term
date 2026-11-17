// テスト用の偽のターミナルエンジン。作ったサーフェスと、サーフェスへの操作を記録する。
// xterm.js も DOM も使わない(Vitest は node の環境で動く)。

import type { TerminalEngine, TerminalId, TerminalSize, TerminalSurface } from "../terminal/engine";

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
  /** hasFocus が返す値(既定は false)。focus で true になる(隠しているときは変えない)。 */
  focused: boolean;
  /** paste に渡された順の文字列。 */
  readonly pasted: string[];
  /** いまの表示(作った直後は true)。 */
  visible: boolean;
  /** setVisible に渡された順の値(同じ値の二重の呼び出しも残す)。 */
  readonly visibleCalls: boolean[];
  /** 登録された入力の受け取り手へ、キー入力を流す。 */
  emitInput(data: string): void;
  /** 登録された寸法の受け取り手へ、寸法の変更を流す。 */
  emitResize(cols: number, rows: number): void;
  /** 登録されたバイト列の入力の受け取り手へ、xterm.js の onBinary と同じ文字列を流す。 */
  emitBinary(data: string): void;
  /** resize に渡された順の行・桁数。 */
  readonly resized: TerminalSize[];
  /** selectAll が呼ばれた回数。 */
  selectAllCount: number;
  /** createSurface に渡された初めの文字の大きさ(渡されなければ undefined)。 */
  readonly createdFontSize: number | undefined;
  /** いま当てている文字の大きさ(既定は xterm.js と同じ 15)。 */
  fontSize: number;
  /** setFontSize に渡された順の値。 */
  readonly fontSizeCalls: number[];
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
    createSurface(id, options = {}) {
      const failDispose = engine.failDisposeOnNext;
      engine.failDisposeOnNext = false;
      const inputListeners: ((data: string) => void)[] = [];
      const resizeListeners: ((cols: number, rows: number) => void)[] = [];
      const binaryListeners: ((data: string) => void)[] = [];
      /** 隠しているあいだに覚えた文字の大きさ(XtermSurface と同じく、表示に戻すときに当てる)。 */
      let pendingFontSize: number | null = null;
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
        resized: [],
        selectAllCount: 0,
        createdFontSize: options.fontSize,
        fontSize: options.fontSize ?? 15,
        fontSizeCalls: [],
        setFontSize(px) {
          surface.fontSizeCalls.push(px);
          if (!surface.visible) {
            pendingFontSize = px;
            return;
          }
          surface.fontSize = px;
          surface.fit();
        },
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
        onBinary(listener) {
          binaryListeners.push(listener);
        },
        resize(size) {
          // xterm.js の resize と同じく、変われば寸法の受け取り手へ流す(隠していても変える)。
          surface.resized.push(size);
          if (size.rows === surface.current.rows && size.cols === surface.current.cols) return;
          surface.current = size;
          surface.emitResize(size.cols, size.rows);
        },
        selectAll() {
          surface.selectAllCount += 1;
        },
        fit() {
          // xterm.js の実装と同じく、隠している間は寸法を決め直さない(置き場所の大きさが 0 になるため)。
          if (!surface.visible) return;
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
          if (visible && pendingFontSize !== null) {
            surface.fontSize = pendingFontSize;
            pendingFontSize = null;
            surface.fit();
          }
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
        focus() {
          if (surface.visible) surface.focused = true;
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
        emitBinary(data) {
          for (const listener of binaryListeners) listener(data);
        },
      };
      created.push(surface);
      return surface;
    },
  };
  return engine;
}
