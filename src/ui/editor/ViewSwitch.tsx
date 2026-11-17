import { type KeyboardEvent, useRef } from "react";

import type { EditorViewMode } from "../../state/uiStore";

interface ViewSwitchProps {
  value: EditorViewMode;
  onChange: (mode: EditorViewMode) => void;
}

const OPTIONS: readonly { readonly value: EditorViewMode; readonly label: string }[] = [
  { value: "edit", label: "編集" },
  { value: "split", label: "並べて" },
  { value: "preview", label: "プレビュー" },
];

/**
 * 表示の切替(ADR-015 の 8。「編集」「並べて」「プレビュー」)。macOS のセグメントにならう小さな区切りのボタン。
 * 読み上げは radiogroup / radio。選ばれているものだけが Tab で止まり、左右の矢印・Home・End で選び替える(端で反対側に回る)。
 */
export function ViewSwitch({ value, onChange }: ViewSwitchProps) {
  const refs = useRef(new Map<EditorViewMode, HTMLButtonElement>());

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const index = OPTIONS.findIndex((o) => o.value === value);
    let next: number | null = null;
    if (event.key === "ArrowRight" || event.key === "ArrowDown") next = (index + 1) % OPTIONS.length;
    else if (event.key === "ArrowLeft" || event.key === "ArrowUp") next = (index - 1 + OPTIONS.length) % OPTIONS.length;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = OPTIONS.length - 1;
    const option = next === null ? undefined : OPTIONS[next];
    if (option === undefined) return;
    event.preventDefault();
    onChange(option.value);
    refs.current.get(option.value)?.focus();
  };

  return (
    <div
      className="segmented editor-view-switch"
      role="radiogroup"
      aria-label="表示"
      data-testid="editor-view-switch"
      onKeyDown={onKeyDown}
    >
      {OPTIONS.map((option) => {
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
            className={
              checked
                ? "segmented__item segmented__item--selected editor-view-switch__item"
                : "segmented__item editor-view-switch__item"
            }
            data-view={option.value}
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
