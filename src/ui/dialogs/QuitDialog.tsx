import { useStore } from "zustand";

import { APP_NAME } from "../../logic/index";
import type { Actions } from "../../state/actions";
import type { UiStore } from "../../state/uiStore";
import type { RunningTerminal } from "../snapshotTypes";
import { ConfirmDialog } from "./ConfirmDialog";

interface QuitDialogProps {
  ui: UiStore;
  actions: Pick<Actions, "confirmQuit" | "cancelQuit">;
}

/** 一覧の 1 行。プロジェクトのターミナルは「プロジェクト名: タブ名」、検証画面のターミナルはタブ名だけ。 */
function runningLabel(terminal: RunningTerminal): string {
  return terminal.projectName === null ? terminal.label : `${terminal.projectName}: ${terminal.label}`;
}

/**
 * 終了の確認(FR-063)。動作中のターミナルがあって Rust が終了を止めたとき(quit-requested)に開く。
 * 動作中のターミナルを並べ、「終了」で終了を確定し、「取り消し」(Esc)なら何もしない。
 * 要求が続けて届いても確認は 1 つ(uiStore が一覧を置き換える)。検証画面でも出す。
 */
export function QuitDialog({ ui, actions }: QuitDialogProps) {
  const confirmation = useStore(ui, (s) => s.quitConfirmation);
  if (confirmation === null) return null;

  return (
    <ConfirmDialog
      title={`${APP_NAME} を終了しますか?`}
      confirmLabel="終了"
      cancelLabel="取り消し"
      destructive
      testId="quit-dialog"
      onConfirm={() => void actions.confirmQuit()}
      onCancel={() => actions.cancelQuit()}
    >
      <p>動作中のターミナルがあります。終了すると、次のターミナルも終了します。</p>
      <ul className="confirm-dialog__list">
        {confirmation.running.map((terminal) => (
          <li key={terminal.terminalId}>{runningLabel(terminal)}</li>
        ))}
      </ul>
    </ConfirmDialog>
  );
}
