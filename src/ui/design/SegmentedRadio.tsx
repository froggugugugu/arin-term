import { type KeyboardEvent, useRef } from "react";

/** 選べる 1 つ。 */
export interface SegmentedOption<T extends string> {
  readonly value: T;
  readonly label: string;
}

interface SegmentedRadioProps<T extends string> {
  /** 読み上げの名前。 */
  label: string;
  options: readonly SegmentedOption<T>[];
  value: T;
  onChange: (value: T) => void;
  /** 囲みに足すクラス(文字の幅に合わせるなどの調整)。 */
  className?: string;
  /** 各項目に足すクラス。 */
  itemClassName?: string;
  testId?: string;
}

/**
 * 文字の区切りのボタン(macOS のセグメントにならう。見た目は AppShell.css の .segmented)。
 * 読み上げは radiogroup / radio。選ばれているものだけが Tab で止まり、左右の矢印・Home・End で選び替える(端で反対側に回る)。
 */
export function SegmentedRadio<T extends string>({
  label,
  options,
  value,
  onChange,
  className,
  itemClassName,
  testId,
}: SegmentedRadioProps<T>) {
  const refs = useRef(new Map<T, HTMLButtonElement>());

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    // フォーカスのある項目を基準にする(値が写しの往復を待って替わる使い方でも、続けて押した矢印が 1 つずつ進む)。
    const focused = options.findIndex((o) => refs.current.get(o.value) === document.activeElement);
    const index = focused >= 0 ? focused : options.findIndex((o) => o.value === value);
    let next: number | null = null;
    if (event.key === "ArrowRight" || event.key === "ArrowDown") next = (index + 1) % options.length;
    else if (event.key === "ArrowLeft" || event.key === "ArrowUp") next = (index - 1 + options.length) % options.length;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = options.length - 1;
    const option = next === null ? undefined : options[next];
    if (option === undefined) return;
    event.preventDefault();
    onChange(option.value);
    refs.current.get(option.value)?.focus();
  };

  const classes = (...names: (string | undefined | false)[]) => names.filter(Boolean).join(" ");

  return (
    <div
      className={classes("segmented", className)}
      role="radiogroup"
      aria-label={label}
      data-testid={testId}
      onKeyDown={onKeyDown}
    >
      {options.map((option) => {
        const checked = option.value === value;
        return (
          <button
            key={option.value}
            ref={(el) => {
              if (el === null) refs.current.delete(option.value);
              else refs.current.set(option.value, el);
            }}
            type="button"
            role="radio"
            aria-checked={checked}
            tabIndex={checked ? 0 : -1}
            className={classes("segmented__item", checked && "segmented__item--selected", itemClassName)}
            data-value={option.value}
            onClick={() => {
              if (!checked) onChange(option.value);
            }}
          >
            {option.label}
          </button>
        );
      })}
    </div>
  );
}
