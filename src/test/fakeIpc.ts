// テスト用の偽の通信。呼ばれたコマンドと引数を記録し、コマンドごとの偽の応答を返す。
// 型は Rust から生成したもの(src/ipc/generated/)を使うので、Rust 側の型が変わると型検査で気づける。

import type { CommandMap, CommandName, InvokeFn } from "../ipc/client";
import type { AppSnapshot } from "../ipc/generated/AppSnapshot";

export interface RecordedCall {
  command: CommandName;
  args: unknown;
}

export type FakeHandlers = {
  [C in CommandName]: (args: CommandMap[C]["args"]) => CommandMap[C]["result"];
};

export interface FakeIpc {
  invoke: InvokeFn;
  /** 呼ばれた順のコマンドと引数。 */
  calls: RecordedCall[];
}

/** テスト用の写し。 */
export function sampleSnapshot(overrides: Partial<AppSnapshot> = {}): AppSnapshot {
  return { schemaVersion: 1, appVersion: "0.0.0-test", terminals: [], ...overrides };
}

/** 偽の通信を作る。渡さなかったコマンドは既定の応答を返す。応答の中で投げた例外は失敗として返す。 */
export function createFakeIpc(handlers: Partial<FakeHandlers> = {}): FakeIpc {
  const all: FakeHandlers = {
    get_app_snapshot: () => sampleSnapshot(),
    notify_app_ready: () => null,
    ...handlers,
  };
  const calls: RecordedCall[] = [];
  const invoke: InvokeFn = async (command, args) => {
    calls.push({ command, args });
    return all[command](args);
  };
  return { invoke, calls };
}
