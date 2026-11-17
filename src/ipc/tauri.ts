// 本物の通信の実体。@tauri-apps/api を使うのは src/ipc だけ(設計書 §3.2)。

import { Channel, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import {
  type ArinIpc,
  type ChannelToken,
  type CommandMap,
  type CommandName,
  createIpc,
  type IpcTransport,
} from "./client";

function tauriInvoke<C extends CommandName>(
  command: C,
  args: CommandMap[C]["args"],
): Promise<CommandMap[C]["result"]> {
  // ChannelToken の中身は Channel のインスタンス(openChannel が作る)。invoke が `__CHANNEL__:<番号>` にして送る。
  return invoke<CommandMap[C]["result"]>(command, args as Record<string, unknown> | undefined);
}

const transport: IpcTransport = {
  invoke: tauriInvoke,
  openChannel(onMessage) {
    const channel = new Channel<unknown>(onMessage);
    let closed = false;
    return {
      token: channel as unknown as ChannelToken,
      close: () => {
        if (closed) return;
        closed = true;
        // 受け取りの関数を外す。以後に Rust が送っても画面には届かない。
        channel.onmessage = () => {};
      },
    };
  },
  listen: (event, handler) => listen<Parameters<typeof handler>[0]>(event, (e) => handler(e.payload)),
};

/** Tauri の IPC を使う包みを作る。 */
export function createTauriIpc(): ArinIpc {
  return createIpc(transport);
}
