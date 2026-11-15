// 画面側の起動の手順(設計書 §11 の画面側の表)。main.tsx から呼ぶ。
//
// 通信の包み(ArinIpc)とストアは引数で受け取る。本物は main.tsx が createTauriIpc() で作り、
// テスト(と T11 の画面の自動テスト)は偽の通信を渡す。
//
// 手順: 写しの更新のイベント(と、渡されたメニューの操作・終了の確認の要求)を購読する → 写しの取得を始める → 描画する → 写しをストアに入れる → Rust へ準備完了を通知する。
// 購読を取得より先にするのは、取得と購読のあいだの変更を取りこぼさないため。
// 取得の応答より先にイベントで写しが届いたら、そちらが新しいので取得の応答は捨てる。
// 失敗は握りつぶさない。原因をストアの startupError に入れて画面に出し、ログにも残す。
// 写しを取得できなかったときは準備完了を通知しない(起動スモークが失敗として気づけるように)。

import type { ArinIpc } from "../ipc/client";
import type { MenuCommand } from "../ipc/generated/MenuCommand";
import type { QuitRequest } from "../ipc/generated/QuitRequest";
import { describeError } from "../logic/describeError";
import type { AppStore } from "./appStore";

export { describeError };

export interface StartAppDeps {
  /** Rust との通信の包み。 */
  ipc: ArinIpc;
  /** 写しを入れるストア。 */
  store: AppStore;
  /** 画面を描く。呼び終わった時点で描画が済んでいること(main.tsx は flushSync で描く)。 */
  mount: () => void;
  /** 失敗のログの出し先。既定は console.error。 */
  logError?: (...args: unknown[]) => void;
  /**
   * Rust から届くイベントの受け手。渡したものだけを、写しの更新と同じく準備完了の通知より先に購読する
   * (準備完了のあとに届く終了の確認の要求を取りこぼさないため)。購読の失敗は起動の失敗として扱う。
   */
  events?: {
    /** メニューの操作(「すべてを選択」)。main.tsx が TerminalManager.handleMenuCommand へつなぐ。 */
    onMenuCommand?: (command: MenuCommand) => void;
    /** 終了の確認の要求(FR-063)。main.tsx が uiStore の終了の確認を開く。 */
    onQuitRequested?: (request: QuitRequest) => void;
  };
}

export type StartAppResult = { ok: true } | { ok: false; error: string };

export async function startApp({
  ipc,
  store,
  mount,
  logError = console.error,
  events = {},
}: StartAppDeps): Promise<StartAppResult> {
  const fail = (what: string, cause: unknown): StartAppResult => {
    const error = `${what}: ${describeError(cause)}`;
    store.getState().reportStartupError(error);
    logError(error, cause);
    return { ok: false, error };
  };

  let updated = false;
  try {
    await ipc.onAppSnapshot((snapshot) => {
      updated = true;
      store.getState().replaceSnapshot(snapshot);
    });
  } catch (cause) {
    mount();
    return fail("状態の更新を購読できなかった", cause);
  }
  const { onMenuCommand, onQuitRequested } = events;
  try {
    if (onMenuCommand !== undefined) await ipc.onMenuCommand(onMenuCommand);
  } catch (cause) {
    mount();
    return fail("メニューの操作を購読できなかった", cause);
  }
  try {
    if (onQuitRequested !== undefined) await ipc.onQuitRequested(onQuitRequested);
  } catch (cause) {
    mount();
    return fail("終了の確認の要求を購読できなかった", cause);
  }

  // 取得は描画と並行して始める。失敗は下の await で受けるので、未処理の失敗にはならない。
  const snapshotRequest = ipc.getAppSnapshot();
  mount();

  try {
    const snapshot = await snapshotRequest;
    if (!updated) store.getState().replaceSnapshot(snapshot);
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
