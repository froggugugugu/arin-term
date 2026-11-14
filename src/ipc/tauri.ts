// 本物の通信の実体。@tauri-apps/api を使うのは src/ipc だけ(設計書 §3.2)。

import { invoke } from "@tauri-apps/api/core";

import { type ArinIpc, type CommandMap, type CommandName, createIpc } from "./client";

function tauriInvoke<C extends CommandName>(
  command: C,
  args: CommandMap[C]["args"],
): Promise<CommandMap[C]["result"]> {
  return invoke<CommandMap[C]["result"]>(command, args);
}

/** Tauri の IPC を使う包みを作る。 */
export function createTauriIpc(): ArinIpc {
  return createIpc(tauriInvoke);
}
