// Rust との通信の型付きの包み(設計書 §4・§5.2)。
// 通信の実体(InvokeFn)は外から受け取る。本物は ./tauri.ts、テストでは src/test/fakeIpc.ts を渡す。

import type { AppSnapshot } from "./generated/AppSnapshot";

/**
 * 画面から呼べる Rust のコマンドと、その引数・戻り値の対応。
 * 名前は src-tauri/src/commands/ の関数名と合わせる。戻り値の無いコマンドは null が返る。
 */
export interface CommandMap {
  get_app_snapshot: { args: undefined; result: AppSnapshot };
  notify_app_ready: { args: undefined; result: null };
}

export type CommandName = keyof CommandMap;

/** 通信の実体。コマンド名と引数を Rust へ送り、戻り値を返す。 */
export type InvokeFn = <C extends CommandName>(
  command: C,
  args: CommandMap[C]["args"],
) => Promise<CommandMap[C]["result"]>;

/** 画面が使う Rust の操作。 */
export interface ArinIpc {
  /** 状態の写しを取得する。 */
  getAppSnapshot(): Promise<AppSnapshot>;
  /** 画面の準備ができたことを Rust へ知らせる。 */
  notifyAppReady(): Promise<void>;
}

export function createIpc(invoke: InvokeFn): ArinIpc {
  return {
    getAppSnapshot: () => invoke("get_app_snapshot", undefined),
    notifyAppReady: async () => {
      await invoke("notify_app_ready", undefined);
    },
  };
}
