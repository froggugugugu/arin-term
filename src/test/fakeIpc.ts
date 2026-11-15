// テスト用の偽の通信。呼ばれたコマンドと引数を記録し、コマンドごとの偽の応答を返す。
// チャネル(Rust → 画面の出力)とイベントの購読も偽物にし、テストから中身を流せるようにする。
// 型は Rust から生成したもの(src/ipc/generated/)を使うので、Rust 側の型が変わると型検査で気づける。

import type {
  ChannelToken,
  CommandMap,
  CommandName,
  EventMap,
  EventName,
  InvokeFn,
  IpcTransport,
  OutputChannel,
} from "../ipc/client";
import type { AppSnapshot } from "../ipc/generated/AppSnapshot";
import type { TerminalStatus } from "../ipc/generated/TerminalStatus";
import type { TerminalTabSnapshot } from "../ipc/generated/TerminalTabSnapshot";

export interface RecordedCall {
  command: CommandName;
  args: unknown;
}

export type FakeHandlers = {
  [C in CommandName]: (args: CommandMap[C]["args"]) => CommandMap[C]["result"] | Promise<CommandMap[C]["result"]>;
};

/** 偽のチャネル 1 本。 */
export interface FakeChannel {
  /** 作った順の番号(0 から)。connect_terminal の引数 onOutput はこの番号を持つ目印になる。 */
  readonly index: number;
  readonly token: ChannelToken;
  /** close が呼ばれたか。 */
  closed: boolean;
  /** Rust が送ったものとして中身を流す(close の後でも流せる。受け取る側が捨てるかを確かめるため)。 */
  send(message: unknown): void;
}

export interface FakeIpc {
  transport: IpcTransport;
  invoke: InvokeFn;
  /** 呼ばれた順のコマンドと引数。 */
  calls: RecordedCall[];
  /** 作られた順のチャネル。 */
  channels: FakeChannel[];
  /** connect_terminal の引数 onOutput が指すチャネル。 */
  channelOf(token: unknown): FakeChannel | undefined;
  /** 購読中のイベントの受け取り手の数。 */
  listenerCount(event: EventName): number;
  /** Rust が送ったものとしてイベントを流す。 */
  emit<E extends EventName>(event: E, payload: EventMap[E]): void;
  /** 次の listen を失敗させる。 */
  failListen: unknown;
}

/** テスト用の写し。既定はプロジェクト 0 件・ターミナル 0 枚で、ペインの寸法は Rust の初期値と同じ。 */
export function sampleSnapshot(overrides: Partial<AppSnapshot> = {}): AppSnapshot {
  return {
    schemaVersion: 1,
    appVersion: "0.0.0-test",
    projects: [],
    selectedProjectId: null,
    terminals: [],
    layout: { sidebarWidth: 260, sidebarCollapsed: false, cardAreaHeight: 200 },
    notice: null,
    ...overrides,
  };
}

/** テスト用の写しのターミナル 1 枚。既定はプロジェクトを持たない(検証画面と同じ形)。 */
export function sampleTerminal(
  id: string,
  status: TerminalStatus,
  overrides: Partial<TerminalTabSnapshot> = {},
): TerminalTabSnapshot {
  return { id, projectId: null, label: "ターミナル 1", status, ...overrides };
}

/** 偽の通信を作る。渡さなかったコマンドは既定の応答を返す。応答の中で投げた例外は失敗として返す。 */
export function createFakeIpc(handlers: Partial<FakeHandlers> = {}): FakeIpc {
  const all: FakeHandlers = {
    get_app_snapshot: () => sampleSnapshot(),
    get_launch_options: () => ({ renderer: "dom", unicode: "unicode11", verifyGrid: false }),
    notify_app_ready: () => null,
    report_frontend_error: () => null,
    connect_terminal: () => null,
    write_terminal: () => null,
    ack_terminal_output: () => null,
    resize_terminal: () => null,
    ...handlers,
  };
  const calls: RecordedCall[] = [];
  const channels: FakeChannel[] = [];
  const listeners = new Map<EventName, Set<(payload: never) => void>>();

  const invoke: InvokeFn = async (command, args) => {
    calls.push({ command, args });
    return all[command](args);
  };

  const fake: FakeIpc = {
    invoke,
    calls,
    channels,
    failListen: undefined,
    transport: {
      invoke,
      openChannel(onMessage): OutputChannel {
        const channel: FakeChannel = {
          index: channels.length,
          token: { fakeChannel: channels.length } as unknown as ChannelToken,
          closed: false,
          send: (message) => onMessage(message),
        };
        channels.push(channel);
        return {
          token: channel.token,
          close: () => {
            channel.closed = true;
          },
        };
      },
      async listen(event, handler) {
        if (fake.failListen !== undefined) {
          const cause = fake.failListen;
          fake.failListen = undefined;
          throw cause;
        }
        const set = listeners.get(event) ?? new Set();
        listeners.set(event, set);
        const entry = handler as (payload: never) => void;
        set.add(entry);
        return () => {
          set.delete(entry);
        };
      },
    },
    channelOf: (token) => channels.find((c) => c.token === token),
    listenerCount: (event) => listeners.get(event)?.size ?? 0,
    emit(event, payload) {
      for (const listener of [...(listeners.get(event) ?? [])]) {
        (listener as (p: EventMap[typeof event]) => void)(payload);
      }
    },
  };
  return fake;
}
