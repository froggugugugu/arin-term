// ドラッグの状態(FR-016、FR-029、FR-037、設計書 §5.1 の `DragMachine`、ADR-007 の (b))。DOM を使わない。
//
// HTML 標準のドラッグ&ドロップは使わず、pointer のイベントで自前に作る(ADR-007 の (b))。画面(計画 T43d)は
// pointerdown → `press`、pointermove → `move`、pointerup → `release`、Esc・pointercancel → `cancel` を呼び、返った結果で動く。
// 押した → しきい値を超えて動いたらドラッグ → 離す(ドロップ)/ 取り消し。しきい値を超えるまではクリックとして扱う。
// ドラッグ中(`isDragging`)はツリーの更新を保留する理由になる(FR-037)。

import { type DragSource, type DropHit, type DropResult, resolveDropTarget } from "./dropTarget";

/** ドラッグを始める動きの大きさ(画面の CSS ピクセル。【仮定】ADR-007 の (b))。これを超えたらドラッグ。 */
export const DRAG_THRESHOLD_PX = 4;

/** 画面の位置(CSS ピクセル)。 */
export interface Point {
  readonly x: number;
  readonly y: number;
}

/** 状態の段階。 */
export type DragPhase = "idle" | "pressed" | "dragging";

/**
 * 操作の結果。
 * - `none`: 画面は何もしない
 * - `click`: しきい値を超えずに離した(行のクリックとして扱う)
 * - `drag-start`: ドラッグが始まった(1 回だけ)
 * - `drop`: ドラッグ中に離した。`result` が操作
 * - `cancelled`: ドラッグ中に取り消した
 */
export type DragOutcome =
  | { readonly type: "none" }
  | { readonly type: "click"; readonly source: DragSource }
  | { readonly type: "drag-start"; readonly source: DragSource }
  | { readonly type: "drop"; readonly result: DropResult }
  | { readonly type: "cancelled" };

type State =
  | { readonly phase: "idle" }
  | { readonly phase: "pressed" | "dragging"; readonly source: DragSource; readonly origin: Point };

const NONE: DragOutcome = { type: "none" };
const IDLE: State = { phase: "idle" };

export class DragMachine {
  #state: State = IDLE;

  get phase(): DragPhase {
    return this.#state.phase;
  }

  /** ドラッグ中か(ツリーの更新を保留する理由。FR-037)。 */
  get isDragging(): boolean {
    return this.#state.phase === "dragging";
  }

  /** 押している・ドラッグしている項目。押していなければ null。 */
  get source(): DragSource | null {
    return this.#state.phase === "idle" ? null : this.#state.source;
  }

  /** 行を押した。前の状態は捨てて始め直す。 */
  press(source: DragSource, point: Point): void {
    this.#state = { phase: "pressed", source, origin: point };
  }

  /** 押したまま動いた。しきい値を超えた最初の 1 回だけ `drag-start` を返す。 */
  move(point: Point): DragOutcome {
    const state = this.#state;
    if (state.phase !== "pressed") return NONE;
    const distance = Math.hypot(point.x - state.origin.x, point.y - state.origin.y);
    if (distance <= DRAG_THRESHOLD_PX) return NONE;
    this.#state = { ...state, phase: "dragging" };
    return { type: "drag-start", source: state.source };
  }

  /** 離した。`hit` は離した位置のドロップ先、`altKey` は離したときの ⌥(FR-029)。 */
  release(hit: DropHit | null, altKey: boolean): DragOutcome {
    const state = this.#state;
    this.#state = IDLE;
    switch (state.phase) {
      case "idle":
        return NONE;
      case "pressed":
        return { type: "click", source: state.source };
      case "dragging":
        return { type: "drop", result: resolveDropTarget(state.source, hit, altKey) };
    }
  }

  /** Esc・pointercancel で取り消す。ドラッグ中なら `cancelled`、ドラッグになる前ならクリックにもせず何もしない。 */
  cancel(): DragOutcome {
    const wasDragging = this.isDragging;
    this.#state = IDLE;
    return wasDragging ? { type: "cancelled" } : NONE;
  }
}
