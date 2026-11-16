import { useStore } from "zustand";

import type { AppStore } from "../../state/appStore";
import type { EditorController } from "../../state/editorController";
import type { UiStore } from "../../state/uiStore";
import { ConfirmDialog } from "./ConfirmDialog";

interface SaveConflictDialogProps {
  store: AppStore;
  ui: UiStore;
  editor: Pick<EditorController, "resolveSaveConflict">;
}

/**
 * 保存の衝突の確認(M4.8。ADR-015 の 5、AC-E05)。保存が changed-outside・missing を返したときに開く。
 * - 外で変わっていた: 「上書き保存」(右端。外の変更を失うので警告の色)・「キャンセル」(既定のボタン。Esc)・
 *   離して左端に「読み直す」(ここでの変更を捨てる)
 * - 消えていた: 「作る」(同じ名前で作る。上書きはしない。Return)・「キャンセル」
 * キャンセルでは何も呼ばず、エディタにフォーカスを戻す。名前は文字として差し込む。
 */
export function SaveConflictDialog({ store, ui, editor }: SaveConflictDialogProps) {
  const conflict = useStore(ui, (s) => s.saveConflict);
  const tab = useStore(store, (s) => s.snapshot?.editors.find((e) => e.id === conflict?.editorId) ?? null);
  if (conflict === null) return null;
  const name = tab?.label ?? "この文書";

  if (conflict.kind === "missing") {
    return (
      <ConfirmDialog
        title={`「${name}」が見つかりません`}
        confirmLabel="作る"
        cancelLabel="キャンセル"
        initialFocus="confirm"
        testId="save-missing-dialog"
        onConfirm={() => void editor.resolveSaveConflict("create")}
        onCancel={() => void editor.resolveSaveConflict("cancel")}
      >
        <p>ほかのアプリで移動したか、名前を変えたか、ゴミ箱に入れた可能性があります。</p>
        <p>「作る」を選ぶと、同じ名前のファイルを作って保存します。</p>
      </ConfirmDialog>
    );
  }

  return (
    <ConfirmDialog
      title={`「${name}」はほかのアプリで変更されています`}
      confirmLabel="上書き保存"
      cancelLabel="キャンセル"
      destructive
      alternate={{ label: "読み直す", destructive: true, onSelect: () => void editor.resolveSaveConflict("reload") }}
      testId="save-conflict-dialog"
      onConfirm={() => void editor.resolveSaveConflict("overwrite")}
      onCancel={() => void editor.resolveSaveConflict("cancel")}
    >
      <p>上書き保存すると、ほかのアプリでの変更は失われます。</p>
      <p>読み直すと、ここでの変更は失われます。</p>
    </ConfirmDialog>
  );
}
