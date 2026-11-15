import { useStore } from "zustand";

import { nameOf } from "../../logic/tree/treePath";
import type { Actions } from "../../state/actions";
import type { UiStore } from "../../state/uiStore";
import { ConfirmDialog } from "./ConfirmDialog";

interface MoveConfirmDialogProps {
  ui: UiStore;
  actions: Pick<Actions, "confirmMove" | "cancelMove">;
  /** ルートの表示名(移動先がルートのときに出す。プロジェクトの名前)。 */
  rootName: string;
}

/**
 * 移動の確認(FR-029、C-05、計画【仮定】12)。ツリー内のドロップでの移動にだけ出す(⌥ のコピーと、切り取り → ペーストには
 * 出さない)。ほかの確認と同じ ConfirmDialog で、右端に既定の「移動」、その左に「中止」を置く。開いたときのフォーカスは
 * 「中止」(誤って移さない)。中止では何も呼ばない(actions.cancelMove)。名前は文字として差し込む。
 */
export function MoveConfirmDialog({ ui, actions, rootName }: MoveConfirmDialogProps) {
  const confirmation = useStore(ui, (s) => s.moveConfirmation);
  if (confirmation === null) return null;
  const name = nameOf(confirmation.source);
  const destination = confirmation.destDir === "" ? rootName : nameOf(confirmation.destDir);

  return (
    <ConfirmDialog
      title={`「${name}」を「${destination}」に移動しますか?`}
      confirmLabel="移動"
      cancelLabel="中止"
      testId="move-confirm-dialog"
      onConfirm={() => void actions.confirmMove()}
      onCancel={() => actions.cancelMove()}
    >
      <p>元の場所からは無くなります。</p>
    </ConfirmDialog>
  );
}
