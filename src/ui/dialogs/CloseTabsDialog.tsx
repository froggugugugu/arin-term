import { useStore } from "zustand";

import type { Actions } from "../../state/actions";
import type { UiStore } from "../../state/uiStore";
import { ConfirmDialog } from "./ConfirmDialog";

interface CloseTabsDialogProps {
  ui: UiStore;
  actions: Pick<Actions, "confirmCloseTabs" | "cancelCloseTabs">;
}

/**
 * タブをまとめて閉じる確認(タブの右クリックのメニュー。ADR-019)。close_tabs が「確認が要る」(閉じるタブに動作中の
 * ターミナルか未保存のエディタがある)と返したときに開く。動作中のターミナルと未保存の文書(相対パス)を並べ、
 * 「キャンセル」なら何もしない、「閉じる」なら確認済みで同じタブを閉じる。文面は削除の確認(RemoveProjectDialog)にそろえる。
 */
export function CloseTabsDialog({ ui, actions }: CloseTabsDialogProps) {
  const confirmation = useStore(ui, (s) => s.closeTabsConfirmation);
  if (confirmation === null) return null;
  const count = confirmation.terminalIds.length + confirmation.editorIds.length;

  return (
    <ConfirmDialog
      title={count === 1 ? "タブを閉じますか?" : `${count} 個のタブを閉じますか?`}
      confirmLabel="閉じる"
      cancelLabel="キャンセル"
      destructive
      testId="close-tabs-dialog"
      onConfirm={() => void actions.confirmCloseTabs()}
      onCancel={() => actions.cancelCloseTabs()}
    >
      {confirmation.running.length > 0 && (
        <>
          <p>動作中のターミナルがあります。閉じると、次のターミナルは終了します。</p>
          <ul className="confirm-dialog__list" data-testid="close-tabs-running-list">
            {confirmation.running.map((terminal) => (
              <li key={terminal.terminalId}>{terminal.label}</li>
            ))}
          </ul>
        </>
      )}
      {confirmation.unsaved.length > 0 && (
        <>
          <p>保存していない文書があります。閉じると、次の文書の変更は失われます。</p>
          <ul className="confirm-dialog__list" data-testid="close-tabs-unsaved-list">
            {confirmation.unsaved.map((editor) => (
              <li key={editor.editorId}>{editor.path}</li>
            ))}
          </ul>
        </>
      )}
    </ConfirmDialog>
  );
}
