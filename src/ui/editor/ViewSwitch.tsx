import type { EditorViewMode } from "../../state/uiStore";
import { type SegmentedOption, SegmentedRadio } from "../design/SegmentedRadio";

interface ViewSwitchProps {
  value: EditorViewMode;
  onChange: (mode: EditorViewMode) => void;
}

const OPTIONS: readonly SegmentedOption<EditorViewMode>[] = [
  { value: "edit", label: "編集" },
  { value: "split", label: "並べて" },
  { value: "preview", label: "プレビュー" },
];

/** 表示の切替(ADR-015 の 8。「編集」「並べて」「プレビュー」)。操作と読み上げは SegmentedRadio。 */
export function ViewSwitch({ value, onChange }: ViewSwitchProps) {
  return (
    <SegmentedRadio
      label="表示"
      options={OPTIONS}
      value={value}
      onChange={onChange}
      className="editor-view-switch"
      itemClassName="editor-view-switch__item"
      testId="editor-view-switch"
    />
  );
}
