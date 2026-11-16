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
// 既定の表の見本。生成した src/ipc/generated/defaultShortcuts.json と同じ中身であることを
// scripts/default-shortcuts.test.mjs が確かめる(src/ の tsconfig は JSON を対象に含めないので見本を使う)。
import { DEFAULT_SHORTCUT_TABLE } from "../logic/keys/defaultShortcuts";

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
  /** このイベントの listen を失敗させる(cause で失敗する)。 */
  failListenOf: { event: EventName; cause: unknown } | undefined;
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
    // 表示の状態の既定(タブ表示・拡大なし・0 枚)。タイル表示の振る舞いは T47d・T47e で扱う
    display: {
      mode: "tabs",
      maximized: false,
      page: 0,
      pageCount: 0,
      grid: { cols: 0, rows: 0 },
      tileIds: [],
      focusedTerminalId: null,
      attentionPages: [],
    },
    // 文字の大きさの既定(Rust の `FontSizes::default()` と範囲。ADR-013 の 1)
    fontSizes: {
      terminal: { value: 15, min: 9, max: 28, defaultValue: 15 },
      interface: { value: 13, min: 11, max: 18, defaultValue: 13 },
    },
    ...overrides,
  };
}

/** テスト用の写しのターミナル 1 枚。既定はプロジェクトを持たない(持ち主の無いタブ。Rust 側は T49d で無くす)。 */
export function sampleTerminal(
  id: string,
  status: TerminalStatus,
  overrides: Partial<TerminalTabSnapshot> = {},
): TerminalTabSnapshot {
  return { id, projectId: null, label: "ターミナル 1", status, attention: false, ...overrides };
}

/** 外から解決・失敗させられる Promise(遅れて届く応答を作るため)。 */
export interface Deferred<T> {
  readonly promise: Promise<T>;
  resolve(value: T): void;
  reject(cause: unknown): void;
}

export function deferred<T>(): Deferred<T> {
  let resolve!: (value: T) => void;
  let reject!: (cause: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

/** 偽の通信を作る。渡さなかったコマンドは既定の応答を返す。応答の中で投げた例外は失敗として返す。 */
export function createFakeIpc(handlers: Partial<FakeHandlers> = {}): FakeIpc {
  const all: FakeHandlers = {
    get_app_snapshot: () => sampleSnapshot(),
    get_shortcuts: () => DEFAULT_SHORTCUT_TABLE.map((binding) => ({ ...binding })),
    get_settings_notice: () => null,
    notify_app_ready: () => null,
    report_frontend_error: () => null,
    confirm_quit: () => null,
    connect_terminal: () => ({ sizeApplied: true }),
    write_terminal: () => null,
    ack_terminal_output: () => null,
    resize_terminal: () => null,
    write_terminal_bytes: () => null,
    new_terminal: () => "terminal-new",
    close_terminal: () => null,
    select_terminal: () => null,
    add_project_by_dialog: () => ({ kind: "cancelled" }),
    select_project: () => null,
    remove_project: () => ({ kind: "removed" }),
    relink_project: () => false,
    set_project_color: () => null,
    copy_project_path: () => null,
    set_pane_sizes: () => null,
    set_sidebar_collapsed: () => null,
    set_display_mode: () => null,
    focus_terminal: () => null,
    next_tile_page: () => null,
    previous_tile_page: () => null,
    toggle_tile_maximize: () => null,
    step_font_size: () => null,
    reset_font_sizes: () => null,
    open_settings_file: () => null,
    list_directory: () => ({ kind: "listed", entries: [] }),
    set_show_hidden: () => null,
    create_entry: () => ({ kind: "done", select: null }),
    rename_entry: () => ({ kind: "done", select: null }),
    move_entry: () => ({ kind: "done", select: null }),
    copy_entry: () => ({ kind: "done", select: null }),
    trash_entry: () => ({ kind: "done", select: null }),
    copy_entries: () => ({ kind: "done", select: null }),
    cut_entries: () => ({ kind: "done", select: null }),
    paste_entries: () => ({ kind: "done", select: null }),
    reveal_entry: () => ({ kind: "done", select: null }),
    open_entry: () => ({ kind: "done", select: null }),
    copy_entry_path: () => ({ kind: "done", select: null }),
    insert_entry_path: () => ({ kind: "done", select: null }),
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
    failListenOf: undefined,
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
        if (fake.failListenOf?.event === event) throw fake.failListenOf.cause;
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
