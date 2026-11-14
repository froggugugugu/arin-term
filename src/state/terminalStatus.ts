// ターミナルの状態(Rust から届いた写し)を、画面に出す文にする。

import type { TerminalStatus } from "../ipc/generated/TerminalStatus";

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
