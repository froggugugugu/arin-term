import { useStore } from "zustand";

import type { AppStore } from "../../state/appStore";
import type { EditorController } from "../../state/editorController";
import type { UiStore } from "../../state/uiStore";
import { ConfirmDialog } from "./ConfirmDialog";

interface UnsavedCloseDialogProps {
  store: AppStore;
  ui: UiStore;
  editor: Pick<EditorController, "resolveUnsavedClose">;
}

/**
 * 未保存のエディタのタブを閉じる確認(M4.8。AC-E07)。タブの × と ⌘W(Rust の editor-close-requested)で開く。
 * macOS の書類を閉じるときのアラートにならい、右端に既定のボタン「保存」(Return)、その左に「キャンセル」(Esc)、
 * 離して左端に「保存しない」を置く。キャンセルではエディタにフォーカスを戻す。名前は文字として差し込む。
 */
export function UnsavedCloseDialog({ store, ui, editor }: UnsavedCloseDialogProps) {
  const pending = useStore(ui, (s) => s.unsavedClose);
  const tab = useStore(store, (s) => s.snapshot?.editors.find((e) => e.id === pending?.editorId) ?? null);
  if (pending === null) return null;
  const name = tab?.label ?? "この文書";

  return (
    <ConfirmDialog
      title={`「${name}」の変更を保存しますか?`}
      confirmLabel="保存"
      cancelLabel="キャンセル"
      initialFocus="confirm"
      alternate={{
        label: "保存しない",
        destructive: true,
        onSelect: () => void editor.resolveUnsavedClose("discard"),
      }}
      testId="unsaved-close-dialog"
      onConfirm={() => void editor.resolveUnsavedClose("save")}
      onCancel={() => void editor.resolveUnsavedClose("cancel")}
    >
      <p>保存しないと、変更は失われます。</p>
    </ConfirmDialog>
  );
}
