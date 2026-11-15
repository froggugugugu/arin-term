import { useStore } from "zustand";

import type { Actions } from "../../state/actions";
import type { AppStore } from "../../state/appStore";
import type { UiStore } from "../../state/uiStore";
import { ConfirmDialog } from "./ConfirmDialog";

interface RemoveProjectDialogProps {
  store: AppStore;
  ui: UiStore;
  actions: Pick<Actions, "confirmRemoveProject" | "cancelRemoveProject">;
}

/**
 * プロジェクトの削除の確認(FR-003)。削除のコマンドが「確認が要る」(動作中のターミナルがある)と返したときに開く。
 * 動作中のターミナルを並べ、「中止」なら何もしない、「削除」なら確認済みで削除する。
 */
export function RemoveProjectDialog({ store, ui, actions }: RemoveProjectDialogProps) {
  const confirmation = useStore(ui, (s) => s.removeConfirmation);
  const projects = useStore(store, (s) => s.snapshot?.projects ?? null);
  if (confirmation === null) return null;
  const name = projects?.find((p) => p.id === confirmation.projectId)?.name ?? confirmation.running[0]?.projectName;

  return (
    <ConfirmDialog
      title={name === undefined || name === null ? "プロジェクトを一覧から削除しますか?" : `「${name}」を一覧から削除しますか?`}
      confirmLabel="削除"
      cancelLabel="中止"
      destructive
      testId="remove-project-dialog"
      onConfirm={() => void actions.confirmRemoveProject()}
      onCancel={() => actions.cancelRemoveProject()}
    >
      <p>動作中のターミナルがあります。削除すると、次のターミナルは終了します。</p>
      <ul className="confirm-dialog__list">
        {confirmation.running.map((terminal) => (
          <li key={terminal.terminalId}>{terminal.label}</li>
        ))}
      </ul>
      <p>フォルダとその中身は削除されません。</p>
    </ConfirmDialog>
  );
}
