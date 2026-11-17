import { type KeyboardEvent, type MouseEvent, useRef } from "react";

/** ステッパーが出す値と範囲(写しの文字の大きさ・透過 1 つぶん。正本は Rust で、ここでは値を持たない)。 */
export interface StepperValue {
  readonly value: number;
  readonly min: number;
  readonly max: number;
}

/** 値の単位。 */
export interface StepperUnit {
  /** 値の後ろに出す文字(「 pt」「%」)。 */
  readonly suffix: string;
  /** 読み上げの単位(「ポイント」「パーセント」)。 */
  readonly spoken: string;
}

interface ValueStepperProps {
  /** 読み上げの名前(「ターミナルの文字の大きさ」など)。 */
  label: string;
  /** 写しの値と範囲。 */
  size: StepperValue;
  /** 単位。 */
  unit: StepperUnit;
  /** 1 回の差分(▲▼・↑↓)。既定は 1。 */
  step?: number;
  /** ▲▼ のボタンの読み上げの名前。 */
  upLabel: string;
  downLabel: string;
  /** 差分だけ動かす(範囲の外へ出る差分は渡さない)。 */
  onStep: (delta: number) => void;
}

/**
 * 値のステッパー(ADR-013 の 5、ADR-014 の 3。macOS の NSStepper にならい、値の右に ▲▼ を縦に重ねた小さな部品)。
 * 文字の大きさ(1 pt 刻み)と透過(5% 刻み)で共用する。
 * 値は `role="spinbutton"` で Tab に止まり、↑↓ で 1 刻みずつ、Home で最小・End で最大へ動かす(Home・End は範囲の幅ぶんの差分を送る)。▲▼ のボタンはマウス用で
 * Tab に止めず(`tabindex="-1"`)、押してもフォーカスは値に残す。範囲の端では ▲ か ▼ を無効にし、キーでも送らない。
 * 値は写しが届くまで変えない(押した回数だけ差分を送り、Rust が順に足す)。
 */
export function ValueStepper({ label, size, unit, step = 1, upLabel, downLabel, onStep }: ValueStepperProps) {
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
      ArrowUp: step,
      ArrowDown: -step,
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
    <div className="value-stepper">
      <div
        ref={spinRef}
        role="spinbutton"
        tabIndex={0}
        aria-label={label}
        aria-valuenow={value}
        aria-valuemin={min}
        aria-valuemax={max}
        aria-valuetext={`${value} ${unit.spoken}`}
        className="value-stepper__value"
        onKeyDown={onKeyDown}
      >
        {value}
        <span className="value-stepper__unit">{unit.suffix}</span>
      </div>
      <div className="value-stepper__arrows">
        <button
          type="button"
          tabIndex={-1}
          className="value-stepper__arrow"
          aria-label={upLabel}
          disabled={!canUp}
          onMouseDown={keepFocus}
          onClick={() => onStep(step)}
        >
          <ArrowIcon direction="up" />
        </button>
        <button
          type="button"
          tabIndex={-1}
          className="value-stepper__arrow"
          aria-label={downLabel}
          disabled={!canDown}
          onMouseDown={keepFocus}
          onClick={() => onStep(-step)}
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
