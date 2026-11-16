import { type KeyboardEvent, type MouseEvent, useRef } from "react";

import type { Snapshot } from "../snapshotTypes";

/** 写しの文字の大きさ 1 つぶん(値・最小・最大・既定)。 */
export type FontSizeValue = Snapshot["fontSizes"]["terminal"];

interface FontSizeStepperProps {
  /** 読み上げの名前(「ターミナルの文字の大きさ」など)。 */
  label: string;
  /** 写しの値と範囲(正本は Rust。ここでは値を持たない)。 */
  size: FontSizeValue;
  /** 差分だけ動かす(範囲の外へ出る差分は渡さない)。 */
  onStep: (delta: number) => void;
}

/**
 * 文字の大きさのステッパー(ADR-013 の 5。macOS の NSStepper にならい、値の右に ▲▼ を縦に重ねた小さな部品)。
 * 値は `role="spinbutton"` で Tab に止まり、↑↓ で 1 ずつ、Home で最小・End で最大へ動かす(Home・End は範囲の幅ぶんの差分を送る)。▲▼ のボタンはマウス用で
 * Tab に止めず(`tabindex="-1"`)、押してもフォーカスは値に残す。範囲の端では ▲ か ▼ を無効にし、キーでも送らない。
 * 値は写しが届くまで変えない(押した回数だけ差分を送り、Rust が順に足す)。
 */
export function FontSizeStepper({ label, size, onStep }: FontSizeStepperProps) {
  const { value, min, max } = size;
  const canUp = value < max;
  const canDown = value > min;
  const spinRef = useRef<HTMLDivElement>(null);

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.metaKey || event.ctrlKey || event.altKey) return;
    // Home・End は範囲の幅ぶんの差分を送り、端に留めるのは Rust に任せる。いま見えている値からの差分にすると、
    // 写しが届く前に続けて押したとき(↑ のあとに Home など)に端まで届かない。
    const span = max - min;
    const deltas: Readonly<Record<string, number>> = {
      ArrowUp: 1,
      ArrowDown: -1,
      Home: -span,
      End: span,
    };
    const delta = deltas[event.key];
    if (delta === undefined) return;
    event.preventDefault();
    // 端では送らない(▲▼ が無効なのと同じ)。
    if (delta > 0 ? canUp : canDown) onStep(delta);
  };

  /** ▲▼ を押してもフォーカスを値に残す(macOS のステッパーと同じくキーの行き先を変えない)。 */
  const keepFocus = (event: MouseEvent<HTMLButtonElement>) => {
    event.preventDefault();
    spinRef.current?.focus();
  };

  return (
    <div className="font-stepper">
      <div
        ref={spinRef}
        role="spinbutton"
        tabIndex={0}
        aria-label={label}
        aria-valuenow={value}
        aria-valuemin={min}
        aria-valuemax={max}
        aria-valuetext={`${value} ポイント`}
        className="font-stepper__value"
        onKeyDown={onKeyDown}
      >
        {value}
        <span className="font-stepper__unit"> pt</span>
      </div>
      <div className="font-stepper__arrows">
        <button
          type="button"
          tabIndex={-1}
          className="font-stepper__arrow"
          aria-label="大きくする"
          disabled={!canUp}
          onMouseDown={keepFocus}
          onClick={() => onStep(1)}
        >
          <ArrowIcon direction="up" />
        </button>
        <button
          type="button"
          tabIndex={-1}
          className="font-stepper__arrow"
          aria-label="小さくする"
          disabled={!canDown}
          onMouseDown={keepFocus}
          onClick={() => onStep(-1)}
        >
          <ArrowIcon direction="down" />
        </button>
      </div>
    </div>
  );
}

/** ステッパーの小さな山形(単色の線画。飾りなので読み上げない)。 */
function ArrowIcon({ direction }: { direction: "up" | "down" }) {
  return (
    <svg width="7" height="4" viewBox="0 0 7 4" fill="none" aria-hidden focusable={false}>
      <path
        d={direction === "up" ? "M0.75 3.25 3.5 0.75 6.25 3.25" : "M0.75 0.75 3.5 3.25 6.25 0.75"}
        stroke="currentColor"
        strokeWidth="1.2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}
