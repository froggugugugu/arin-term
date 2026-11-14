// ターミナルの状態(Rust から届いた写し)を、画面に出す文にする。

import type { AppSnapshot } from "../ipc/generated/AppSnapshot";
import type { TerminalId } from "../ipc/generated/TerminalId";
import type { TerminalStatus } from "../ipc/generated/TerminalStatus";
import type { AppStore } from "./appStore";

/** 起動前と実行中は null(何も出さない)。終了と起動の失敗は 1 行の文。 */
export function describeTerminalStatus(status: TerminalStatus): string | null {
  switch (status.kind) {
    case "notStarted":
    case "running":
      return null;
    case "exited": {
      const details = [
        status.code === null ? null : `終了コード ${status.code}`,
        status.signal === null ? null : `シグナル ${status.signal}`,
      ].filter((d) => d !== null);
      return details.length === 0 ? "シェルが終了した" : `シェルが終了した(${details.join("、")})`;
    }
    case "failed":
      return `シェルを起動できなかった: ${status.message}`;
  }
}

/**
 * いまの写しと、写しが届くたびに、写しにある全ターミナルの状態を onStatus へ伝える。外す関数を返す。
 * main.tsx が TerminalManager.setStatus につなぐ(終わったシェルへ入力と寸法を送らないため)。
 * State は src/terminal に依存しない(設計書 §3.1)ので、送り先は関数で受け取る。
 */
export function forwardTerminalStatuses(
  store: AppStore,
  onStatus: (id: TerminalId, status: TerminalStatus) => void,
): () => void {
  const forward = (snapshot: AppSnapshot | null) => {
    for (const terminal of snapshot?.terminals ?? []) onStatus(terminal.id, terminal.status);
  };
  forward(store.getState().snapshot);
  return store.subscribe((state, previous) => {
    if (state.snapshot !== previous.snapshot) forward(state.snapshot);
  });
}
