// 画面側の起動の手順(設計書 §11 の画面側の表)。main.tsx から呼ぶ。
//
// 通信の包み(ArinIpc)とストアは引数で受け取る。本物は main.tsx が createTauriIpc() で作り、
// テスト(と T11 の画面の自動テスト)は偽の通信を渡す。
//
// 手順: 写しの取得を始める → 描画する → 写しをストアに入れる → Rust へ準備完了を通知する。
// 失敗は握りつぶさない。原因をストアの startupError に入れて画面に出し、ログにも残す。
// 写しを取得できなかったときは準備完了を通知しない(起動スモークが失敗として気づけるように)。

import type { ArinIpc } from "../ipc/client";
import type { AppStore } from "./appStore";

export interface StartAppDeps {
  /** Rust との通信の包み。 */
  ipc: ArinIpc;
  /** 写しを入れるストア。 */
  store: AppStore;
  /** 画面を描く。呼び終わった時点で描画が済んでいること(main.tsx は flushSync で描く)。 */
  mount: () => void;
  /** 失敗のログの出し先。既定は console.error。 */
  logError?: (...args: unknown[]) => void;
}

export type StartAppResult = { ok: true } | { ok: false; error: string };

/** 失敗の原因を、画面に出せる 1 行の文字列にする。Tauri のコマンドの失敗は文字列で届く。 */
export function describeError(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  try {
    const json = JSON.stringify(error);
    if (json !== undefined) return json;
  } catch {
    // BigInt や循環参照など、JSON にできないものは下で String にする。
  }
  return String(error);
}

export async function startApp({
  ipc,
  store,
  mount,
  logError = console.error,
}: StartAppDeps): Promise<StartAppResult> {
  const fail = (what: string, cause: unknown): StartAppResult => {
    const error = `${what}: ${describeError(cause)}`;
    store.getState().reportStartupError(error);
    logError(error, cause);
    return { ok: false, error };
  };

  // 取得は描画と並行して始める。失敗は下の await で受けるので、未処理の失敗にはならない。
  const snapshotRequest = ipc.getAppSnapshot();
  mount();

  try {
    store.getState().replaceSnapshot(await snapshotRequest);
  } catch (cause) {
    return fail("状態の写しを取得できなかった", cause);
  }

  try {
    await ipc.notifyAppReady();
  } catch (cause) {
    return fail("準備完了を通知できなかった", cause);
  }
  return { ok: true };
}
