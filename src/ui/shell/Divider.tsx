import { type CSSProperties, type KeyboardEvent, type PointerEvent, useRef } from "react";

import { beginDividerDrag, type DividerDrag } from "../../logic/panes/paneLayout";

/** キーボードの矢印 1 回で動かす量(CSS ピクセル)。 */
export const DIVIDER_KEY_STEP = 16;

interface DividerProps {
  /** `vertical` は左右を分ける縦の境界(x で動く)、`horizontal` は上下を分ける横の境界(y で動く)。 */
  orientation: "vertical" | "horizontal";
  label: string;
  /** 境界の手前のペインの、いまの表示値。 */
  value: number;
  min: number;
  max: number;
  /** 動かした後の値(最小・最大で止める計算。src/logic/panes の *DuringDrag)。 */
  valueDuringDrag: (drag: DividerDrag, pointer: number) => number;
  /** ドラッグ中の値を描く。null で取り消す。 */
  onPreview: (value: number | null) => void;
  /** 値を確定する(離したとき・キーで動かしたとき)。 */
  onCommit: (value: number) => void;
  controls?: string;
  style?: CSSProperties;
  testId: string;
}

/**
 * ペインの境界(FR-009)。ドラッグしている間は onPreview で表示だけを変え、離したときに onCommit で 1 回だけ確定する。
 * フォーカスを当てて矢印キーでも動かせる(1 回 16px)。
 */
export function Divider({
  orientation,
  label,
  value,
  min,
  max,
  valueDuringDrag,
  onPreview,
  onCommit,
  controls,
  style,
  testId,
}: DividerProps) {
  const drag = useRef<DividerDrag | null>(null);
  const pointerOf = (event: PointerEvent) => (orientation === "vertical" ? event.clientX : event.clientY);

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    event.currentTarget.focus();
    drag.current = beginDividerDrag(pointerOf(event), value);
  };
  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    if (drag.current === null) return;
    onPreview(valueDuringDrag(drag.current, pointerOf(event)));
  };
  const onPointerUp = (event: PointerEvent<HTMLDivElement>) => {
    const current = drag.current;
    if (current === null) return;
    drag.current = null;
    onCommit(valueDuringDrag(current, pointerOf(event)));
  };
  const cancel = () => {
    if (drag.current === null) return;
    drag.current = null;
    onPreview(null);
  };
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const [less, more] = orientation === "vertical" ? ["ArrowLeft", "ArrowRight"] : ["ArrowUp", "ArrowDown"];
    const step = event.key === less ? -DIVIDER_KEY_STEP : event.key === more ? DIVIDER_KEY_STEP : 0;
    if (step === 0) return;
    event.preventDefault();
    const next = valueDuringDrag(beginDividerDrag(0, value), step);
    if (next !== value) onCommit(next);
  };

  return (
    // 動かせる境界(フォーカスでき値を持つ separator)には相当する要素が無いので、役割を付けた div にする。
    <div
      role="separator"
      aria-orientation={orientation}
      aria-label={label}
      aria-valuenow={Math.round(value)}
      aria-valuemin={Math.round(min)}
      aria-valuemax={Math.round(max)}
      aria-controls={controls}
      tabIndex={0}
      className={`divider divider--${orientation}`}
      data-testid={testId}
      style={style}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={cancel}
      onLostPointerCapture={cancel}
      onKeyDown={onKeyDown}
    />
  );
}
