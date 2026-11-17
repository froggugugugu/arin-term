import { type KeyboardEvent, type SyntheticEvent, useLayoutEffect, useRef } from "react";

import type { SelectionRange } from "../../logic/tree/renameSelection";

/** 入力を閉じた理由。escape と commit はツリーにフォーカスを戻し、blur は戻さない(フォーカスは移った先のまま)。 */
export type InlineEditEnd = "commit" | "escape" | "blur";

interface InlineNameInputProps {
  /** 入力欄の最初の文字(名称変更は今の名前、新規作成は空)。 */
  initialName: string;
  /** 最初に選ぶ範囲(名称変更。src/logic/tree/renameSelection.ts)。null なら末尾にカーソルを置く。 */
  selection: SelectionRange | null;
  /** 読み上げの名前。 */
  label: string;
  /**
   * 入力を閉じる。commit のときだけ name に入力した名前が入る。
   * 呼び出し側が確定(actions.commitInlineEdit)か取り消し(uiStore.closeInlineEdit)を行う。
   */
  onEnd: (end: InlineEditEnd, name: string) => void;
}

/** 行の上のクリック・ダブルクリック・右クリックを、行とツリーに伝えない(入力欄の中の操作にする)。 */
const stop = (event: SyntheticEvent) => event.stopPropagation();

/**
 * ツリーの名前の入力欄(新規作成・名称変更のインライン編集。FR-025・FR-026、計画【仮定】9)。行の中の名前の位置に出す
 * (Finder の名称変更のように。行の高さは変えない)。出たらフォーカスし、名称変更では拡張子を除いた部分を選ぶ。
 * - Return で確定、Esc で取り消し、フォーカスが外れたら取り消す(意図せず作らないため)
 * - IME の変換中の Return では確定しない(isComposing。WebKit は変換を確定する Return を keyCode 229 で送ることがある)
 * - キーは入力欄で止め、ツリーのショートカットにしない(⌘⌫ などは文字の操作のまま)。コピー・ペーストのイベントは
 *   入口(clipboardRouting.ts)が入力欄の既定の動作に任せる(data-tree-name-input で見分ける)
 * - 閉じるのは 1 回だけ(確定のあとに外れたフォーカスで取り消しにしない)
 */
export function InlineNameInput({ initialName, selection, label, onEnd }: InlineNameInputProps) {
  const inputRef = useRef<HTMLInputElement>(null);
  const ended = useRef(false);
  const selectionRef = useRef(selection);

  useLayoutEffect(() => {
    const input = inputRef.current;
    if (input === null) return;
    input.focus();
    const range = selectionRef.current;
    if (range !== null) input.setSelectionRange(range.start, range.end);
    else input.setSelectionRange(input.value.length, input.value.length);
  }, []);

  const end = (how: InlineEditEnd) => {
    if (ended.current) return;
    ended.current = true;
    onEnd(how, inputRef.current?.value ?? "");
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    event.stopPropagation();
    if (event.nativeEvent.isComposing || event.keyCode === 229) return;
    if (event.key === "Enter") {
      event.preventDefault();
      end("commit");
    } else if (event.key === "Escape") {
      event.preventDefault();
      end("escape");
    }
  };

  return (
    <input
      ref={inputRef}
      className="tree-name-input"
      data-testid="tree-name-input"
      data-tree-name-input=""
      aria-label={label}
      defaultValue={initialName}
      spellCheck={false}
      autoComplete="off"
      autoCorrect="off"
      autoCapitalize="off"
      onKeyDown={onKeyDown}
      onBlur={() => end("blur")}
      onClick={stop}
      onDoubleClick={stop}
      onContextMenu={stop}
    />
  );
}
