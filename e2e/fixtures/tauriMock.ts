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
// - チャネルとイベントの購読は「壊れない」最小限(T18)。登録は受け付けるが、偽の Rust 側から中身は流さない。
//   ターミナルの画面テスト(T19)で、ここから出力やイベントを流せるようにする
// - イベントの購読・解除(`plugin:event|listen` など)は Tauri の内部の口なので、記録(__ARIN_E2E_CALLS__)には残さない
//
// # 型の合わせ方
//
// 応答の形は src/ipc/client.ts の CommandMap と、Rust から生成した型(src/ipc/generated/)から作る。
// コマンドが増えたり戻り値の型が変わったりすると、DEFAULT_REPLIES が型検査で通らなくなる
// (ただし、型検査の対象に e2e/ を入れた場合。Playwright はテストを実行するとき型を検査しない)。

import type { Page } from "@playwright/test";

import type { CommandMap, CommandName } from "../../src/ipc/client";
import type { AppSnapshot } from "../../src/ipc/generated/AppSnapshot";

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

/** テスト用の写し。 */
export function sampleSnapshot(overrides: Partial<AppSnapshot> = {}): AppSnapshot {
  return { schemaVersion: 1, appVersion: "0.0.0-e2e", terminals: [], ...overrides };
}

const DEFAULT_REPLIES: TauriMockConfig = {
  get_app_snapshot: { ok: sampleSnapshot() },
  notify_app_ready: { ok: null },
  report_frontend_error: { ok: null },
  connect_terminal: { ok: null },
  write_terminal: { ok: null },
  ack_terminal_output: { ok: null },
  resize_terminal: { ok: null },
};

/** 画面のページの中で、偽の Rust 側が記録を置く場所(テストの側からだけ読む)。 */
interface MockWindow {
  __TAURI_INTERNALS__?: Record<string, unknown>;
  __TAURI_EVENT_PLUGIN_INTERNALS__?: Record<string, unknown>;
  __ARIN_E2E_CALLS__?: RecordedCall[];
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
  w.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
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
      if (command === "plugin:event|listen") return nextEvent++;
      if (command === "plugin:event|unlisten") return null;
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
