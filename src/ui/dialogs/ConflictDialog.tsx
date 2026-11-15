import { useStore } from "zustand";

import type { Actions } from "../../state/actions";
import type { UiStore } from "../../state/uiStore";
import { ConfirmDialog } from "./ConfirmDialog";

interface ConflictDialogProps {
  ui: UiStore;
  actions: Pick<Actions, "resolveConflict" | "cancelConflict">;
}

/**
 * 名前の衝突の確認(FR-030、AC-T18、設計書 §7.1「ファイル操作」)。ファイル操作が conflict を返したときに開く。
 * macOS のアラートの並びにならい、右端に既定のボタン「両方残す」(何も失わない。Return)、その左に「中止」(Esc)、
 * 離して左端に「置き換える」を置く。「置き換える」は既存をゴミ箱に入れることを文面で伝える(設計書 §1.3 #11)。
 * 中止では何も呼び直さない(actions.cancelConflict)。名前は文字として差し込む。
 */
export function ConflictDialog({ ui, actions }: ConflictDialogProps) {
  const conflict = useStore(ui, (s) => s.treeConflict);
  if (conflict === null) return null;
  const { name } = conflict;

  return (
    <ConfirmDialog
      title={`「${name}」という名前の項目がすでにあります`}
      confirmLabel="両方残す"
      cancelLabel="中止"
      initialFocus="confirm"
      alternate={{ label: "置き換える", destructive: true, onSelect: () => void actions.resolveConflict("replace") }}
      testId="tree-conflict-dialog"
      onConfirm={() => void actions.resolveConflict("keepBoth")}
      onCancel={() => actions.cancelConflict()}
    >
      <p>置き換えると、いまある「{name}」はゴミ箱に入ります。</p>
      <p>両方残すと、新しい方の名前に番号を付けます。</p>
    </ConfirmDialog>
  );
}
