// 画面の自動テスト用の偽の Rust 側(T11。設計書 §10.3、ADR-006)。
//
// # 差し込み方
//
// Playwright の page.addInitScript で、画面のスクリプトより先に window.__TAURI_INTERNALS__ を置く。
// これは Tauri が本物の WebView に注入する内部の口で、@tauri-apps/api の invoke() はここを呼ぶ
// (node_modules/@tauri-apps/api/core.js の invoke)。そのため画面は src/main.tsx → createTauriIpc()
// (src/ipc/tauri.ts)→ @tauri-apps/api の invoke() という本物の経路をそのまま通り、最後の 1 段だけが偽物になる。
//
// - 製品のソース(src/)には差し込み口を一切置かない。偽物はテストのプロセスから注入するだけなので、
//   `npm run build:web` や `npm run app` のビルド結果に偽物が入る余地が無い(N2: 外部から操作する口を作らない)
// - @tauri-apps/api/mocks の mockIPC も同じ口を置くだけの関数だが、ES モジュールで export を持つため
//   addInitScript(古典的なスクリプトとして実行される)にそのまま渡せない。Arin が使うのは invoke と
//   チャネル(Channel)とイベントの購読(listen)だけなので、必要な部分だけをここに書く
// - チャネル(T19): @tauri-apps/api の Channel は transformCallback で受け取りの関数を登録し、
//   コマンドの引数では `__CHANNEL__:<番号>` の文字列になる。Rust の送り出しと同じく、その番号の関数に
//   `{ message, index }`(index は 0 から順に増える)を渡せば出力が届く。sendChannelMessages がこれを行う
// - イベント(T19): `plugin:event|listen` で登録された受け取りの関数を覚え、emitEvent で
//   Tauri と同じ `{ event, id, payload }` の形で呼ぶ
// - イベントの購読・解除(`plugin:event|listen` など)は Tauri の内部の口なので、記録(__ARIN_E2E_CALLS__)には残さない
//
// # 型の合わせ方
//
// 応答の形は src/ipc/client.ts の CommandMap と、Rust から生成した型(src/ipc/generated/)から作る。
// コマンドが増えたり戻り値の型が変わったりすると、DEFAULT_REPLIES が型検査で通らなくなる
// (ただし、型検査の対象に e2e/ を入れた場合。Playwright はテストを実行するとき型を検査しない)。

import type { Page } from "@playwright/test";

import type { CommandMap, CommandName, EventMap, EventName } from "../../src/ipc/client";
import type { AppSnapshot } from "../../src/ipc/generated/AppSnapshot";
import type { TerminalStatus } from "../../src/ipc/generated/TerminalStatus";
import type { TerminalTabSnapshot } from "../../src/ipc/generated/TerminalTabSnapshot";

/**
 * 1 つのコマンドへの偽の応答。ok なら値を返し、error なら Rust のコマンドの失敗と同じく文字列で失敗させる。
 * 画面のページへ JSON として渡すので、JSON にできる値だけにする。
 */
export type MockReply<C extends CommandName> = { ok: CommandMap[C]["result"] } | { error: string };

/** コマンドごとの偽の応答。画面が呼ぶコマンドはすべて埋める(足りなければ型検査で気づける)。 */
export type TauriMockConfig = { [C in CommandName]: MockReply<C> };

/** 画面から偽の Rust 側に届いたコマンド 1 件。 */
export interface RecordedCall {
  command: string;
  args: unknown;
}

/** テスト用の写し。既定はプロジェクト 0 件・ターミナル 0 枚で、ペインの寸法は Rust の初期値と同じ。 */
export function sampleSnapshot(overrides: Partial<AppSnapshot> = {}): AppSnapshot {
  return {
    schemaVersion: 1,
    appVersion: "0.0.0-e2e",
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

/** テスト用のプロジェクトの ID(projectSnapshot が使う)。 */
export const SAMPLE_PROJECT_ID = "p-e2e-1";

/**
 * ターミナルを 1 つのプロジェクトのタブとして載せた写し。プロジェクトを選択中にし、最初のタブを表示中にする
 * (画面は選択中のプロジェクトの表示中のタブを出す。T34b)。
 */
export function projectSnapshot(terminals: readonly TerminalTabSnapshot[], overrides: Partial<AppSnapshot> = {}): AppSnapshot {
  return sampleSnapshot({
    projects: [
      {
        id: SAMPLE_PROJECT_ID,
        name: "e2e",
        color: "blue",
        displayPath: "~/e2e",
        missing: false,
        terminalIds: terminals.map((t) => t.id),
        activeTerminalId: terminals[0]?.id ?? null,
      },
    ],
    selectedProjectId: SAMPLE_PROJECT_ID,
    terminals: terminals.map((t) => ({ ...t, projectId: SAMPLE_PROJECT_ID })),
    ...overrides,
  });
}

const DEFAULT_REPLIES: TauriMockConfig = {
  get_app_snapshot: { ok: sampleSnapshot() },
  get_launch_options: { ok: { renderer: "dom", unicode: "unicode11", verifyGrid: false } },
  notify_app_ready: { ok: null },
  report_frontend_error: { ok: null },
  confirm_quit: { ok: null },
  connect_terminal: { ok: { sizeApplied: true } },
  write_terminal: { ok: null },
  ack_terminal_output: { ok: null },
  resize_terminal: { ok: null },
  write_terminal_bytes: { ok: null },
  new_terminal: { ok: "terminal-new" },
  close_terminal: { ok: null },
  select_terminal: { ok: null },
  add_project_by_dialog: { ok: { kind: "cancelled" } },
  select_project: { ok: null },
  remove_project: { ok: { kind: "removed" } },
  relink_project: { ok: false },
  set_project_color: { ok: null },
  copy_project_path: { ok: null },
  set_pane_sizes: { ok: null },
  set_sidebar_collapsed: { ok: null },
};

/** 画面のページの中で、偽の Rust 側が記録を置く場所(テストの側からだけ読む)。 */
interface MockWindow {
  __TAURI_INTERNALS__?: Record<string, unknown>;
  __TAURI_EVENT_PLUGIN_INTERNALS__?: Record<string, unknown>;
  __ARIN_E2E_CALLS__?: RecordedCall[];
  __ARIN_E2E__?: MockControls;
}

/** 偽の Rust 側から画面へ送るための操作(画面のページの中にある。テストの側から page.evaluate で呼ぶ)。 */
interface MockControls {
  /** チャネルに中身を順に送る(index はチャネルごとに続きから振る)。受け取りの関数が無ければ false。 */
  sendChannel(channelId: number, messages: unknown[]): boolean;
  /** イベントを、購読している受け取りの関数すべてに届ける。届けた数を返す。 */
  emit(event: string, payload: unknown): number;
}

/**
 * 画面のページの中で実行する本体。Playwright はこの関数を文字列にして新しい文書の先頭で実行するので、
 * 外の変数や import を参照せず、引数の replies だけを使う(型の import は実行時には消える)。
 */
function injectTauriMock(replies: Record<string, { ok: unknown } | { error: string }>): void {
  const w = window as unknown as MockWindow;
  const calls: RecordedCall[] = [];
  w.__ARIN_E2E_CALLS__ = calls;
  // Channel と listen が渡す受け取りの関数(@tauri-apps/api の transformCallback)。
  const callbacks = new Map<number, (payload: unknown) => void>();
  let nextCallback = 1;
  let nextEvent = 1;
  // チャネルごとの次の index(Rust の Channel と同じく 0 から)。
  const channelIndex = new Map<number, number>();
  // イベントの購読: 購読の番号 → { イベント名, 受け取りの関数の番号 }。
  const listeners = new Map<number, { event: string; handler: number }>();
  w.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener: (_event: string, eventId: number) => {
      listeners.delete(eventId);
    },
  };
  w.__ARIN_E2E__ = {
    sendChannel: (channelId, messages) => {
      const callback = callbacks.get(channelId);
      if (callback === undefined) return false;
      for (const message of messages) {
        const index = channelIndex.get(channelId) ?? 0;
        channelIndex.set(channelId, index + 1);
        callback({ message, index });
      }
      return true;
    },
    emit: (event, payload) => {
      let delivered = 0;
      for (const [id, listener] of listeners) {
        if (listener.event !== event) continue;
        callbacks.get(listener.handler)?.({ event, id, payload });
        delivered++;
      }
      return delivered;
    },
  };
  w.__TAURI_INTERNALS__ = {
    transformCallback: (callback: (payload: unknown) => void): number => {
      const id = nextCallback++;
      callbacks.set(id, callback);
      return id;
    },
    unregisterCallback: (id: number): void => {
      callbacks.delete(id);
    },
    runCallback: (id: number, payload: unknown): void => {
      callbacks.get(id)?.(payload);
    },
    invoke: async (command: string, args: unknown): Promise<unknown> => {
      if (command === "plugin:event|listen") {
        const { event, handler } = args as { event: string; handler: number };
        const id = nextEvent++;
        listeners.set(id, { event, handler });
        return id;
      }
      if (command === "plugin:event|unlisten") {
        listeners.delete((args as { eventId: number }).eventId);
        return null;
      }
      // 画面が渡した引数をそのまま残すと、あとで画面が書き換えたときに記録も変わるので、写しを取る。
      calls.push({ command, args: args === undefined ? undefined : JSON.parse(JSON.stringify(args)) });
      const reply = replies[command];
      if (reply === undefined) {
        // 本物の Tauri も、登録されていないコマンドは文字列で失敗させる。
        throw `command ${command} not found`;
      }
      if ("error" in reply) throw reply.error;
      // 画面が受け取った値を書き換えても、次の応答に影響しないよう、毎回作り直す。
      return JSON.parse(JSON.stringify(reply.ok));
    },
  };
}

/**
 * 偽の Rust 側を差し込む。page.goto の前に呼ぶ。渡さなかったコマンドは既定の応答を返す。
 * 再読み込みのたびに差し込み直され、記録も空から始まる。
 */
export async function installTauriMock(page: Page, overrides: Partial<TauriMockConfig> = {}): Promise<void> {
  const replies: TauriMockConfig = { ...DEFAULT_REPLIES, ...overrides };
  await page.addInitScript(injectTauriMock, replies);
}

/** 画面から届いたコマンドを、届いた順に返す。 */
export async function readRecordedCalls(page: Page): Promise<RecordedCall[]> {
  return page.evaluate(() => (window as unknown as MockWindow).__ARIN_E2E_CALLS__ ?? []);
}

/** 画面から届いた、指定のコマンドの引数だけを届いた順に返す。 */
export async function readCallArgs<C extends CommandName>(page: Page, command: C): Promise<RecordedArgs<C>[]> {
  const calls = await readRecordedCalls(page);
  return calls.filter((call) => call.command === command).map((call) => call.args as RecordedArgs<C>);
}

/**
 * 記録に残る引数の形。JSON にした写しなので、チャネル(ChannelToken)は `__CHANNEL__:<番号>` の文字列になる。
 */
export type RecordedArgs<C extends CommandName> = {
  [K in keyof NonNullable<CommandMap[C]["args"]>]: K extends "onOutput"
    ? string
    : NonNullable<CommandMap[C]["args"]>[K];
};

/** `__CHANNEL__:<番号>` から番号を取り出す。形が違えば例外。 */
export function parseChannelId(token: string): number {
  const match = /^__CHANNEL__:(\d+)$/.exec(token);
  if (match === null) throw new Error(`チャネルの形ではない: ${token}`);
  return Number(match[1]);
}

/** チャネルで送る 1 回ぶん。文字列は UTF-8 にして送る。数の配列はそのままのバイト列(文字の途中で切った出力を作れる)。 */
export type OutputChunk = string | readonly number[];

/**
 * チャネルへ生のバイト列を順に送る(Tauri が生のバイト列を ArrayBuffer で渡すのと同じ形)。
 * 送ったバイト数の合計を返す。
 */
export async function sendChannelBytes(
  page: Page,
  channelId: number,
  chunks: readonly OutputChunk[],
): Promise<number> {
  return page.evaluate(
    ({ channelId, chunks }) => {
      const encoder = new TextEncoder();
      const buffers = chunks.map((chunk) => {
        const bytes = typeof chunk === "string" ? encoder.encode(chunk) : Uint8Array.from(chunk);
        // 中身ちょうどの ArrayBuffer にする(encode の結果は余白を持たないが、念のため切り出す)。
        return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
      });
      const controls = (window as unknown as MockWindow).__ARIN_E2E__;
      if (controls === undefined || !controls.sendChannel(channelId, buffers)) {
        throw new Error(`チャネル ${channelId} の受け取りの関数が無い`);
      }
      return buffers.reduce((sum, buffer) => sum + buffer.byteLength, 0);
    },
    { channelId, chunks: chunks.map((chunk) => (typeof chunk === "string" ? chunk : [...chunk])) },
  );
}

/** Rust からのイベントを画面へ届ける。購読が無ければ例外(届かないまま気づかずに進まないように)。 */
export async function emitEvent<E extends EventName>(page: Page, event: E, payload: EventMap[E]): Promise<void> {
  const delivered = await page.evaluate(
    ({ event, payload }) => (window as unknown as MockWindow).__ARIN_E2E__?.emit(event, payload) ?? 0,
    { event, payload },
  );
  if (delivered === 0) throw new Error(`イベント ${event} の購読が無い`);
}
