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
// ショートカットの表(T43a): onShortcuts を渡したときだけ、写しが届いたあと準備完了の通知の前に get_shortcuts で受け取る。
// 取得の失敗は起動の失敗にしない(既定の表の見本のまま動く)。reportError に 1 行で報告する。
// 設定への追従(T65e): onShortcuts か onSettingsNotice を渡したときは、取得の前に settings-changed を購読し、届くたびに
// 表と設定の知らせを両方取り直して受け手に渡す。どちらかを取り直せなければ、どちらも渡さない(今の表のまま。報告だけする)。
// 続けて届いたときは、最後に始めた取り直しの結果だけを渡す(古い応答があとから届いても戻さない)。
// 購読の失敗も起動の失敗にしない(起動時の表のまま動く)。

import type { ArinIpc } from "../ipc/client";
import type { MenuCommand } from "../ipc/generated/MenuCommand";
import type { QuitRequest } from "../ipc/generated/QuitRequest";
import type { ShortcutTable } from "../ipc/generated/ShortcutTable";
import type { TreeChange } from "../ipc/generated/TreeChange";
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
    /** ツリーの外からの変更(FR-014)。main.tsx が TreeController.handleTreeChange へつなぐ。 */
    onTreeChanged?: (change: TreeChange) => void;
  };
  /** Rust から受け取ったショートカットの表の受け手(main.tsx が uiStore.setShortcuts へつなぐ)。渡さなければ取得しない。 */
  onShortcuts?: (table: ShortcutTable) => void;
  /** Rust から受け取った設定の知らせ(1 行か null)の受け手(main.tsx が uiStore.setSettingsNotice へつなぐ)。渡さなければ取得しない。 */
  onSettingsNotice?: (notice: string | null) => void;
  /** 起動を止めない失敗(ショートカットの表・設定の知らせを取得できなかった、設定の変更を購読できなかった)の報告先。既定は logError。 */
  reportError?: (message: string) => void;
}

export type StartAppResult = { ok: true } | { ok: false; error: string };

export async function startApp({
  ipc,
  store,
  mount,
  logError = console.error,
  events = {},
  onShortcuts,
  onSettingsNotice,
  reportError,
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
  const { onMenuCommand, onQuitRequested, onTreeChanged } = events;
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
  try {
    if (onTreeChanged !== undefined) await ipc.onTreeChanged(onTreeChanged);
  } catch (cause) {
    mount();
    return fail("ツリーの変更を購読できなかった", cause);
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

  if (onShortcuts !== undefined || onSettingsNotice !== undefined) {
    await followSettings({ ipc, onShortcuts, onSettingsNotice, report: reportError ?? ((m) => logError(m)) });
  }

  try {
    await ipc.notifyAppReady();
  } catch (cause) {
    return fail("準備完了を通知できなかった", cause);
  }
  return { ok: true };
}

interface FollowSettingsDeps {
  ipc: ArinIpc;
  onShortcuts: ((table: ShortcutTable) => void) | undefined;
  onSettingsNotice: ((notice: string | null) => void) | undefined;
  report: (message: string) => void;
}

/** 設定の変更を購読してから、起動時の表と設定の知らせを受け取る(受け手を渡したものだけ)。 */
async function followSettings({ ipc, onShortcuts, onSettingsNotice, report }: FollowSettingsDeps): Promise<void> {
  // 始めた取り直しの番号。応答が届いた時点で最新でなければ捨てる。
  let latest = 0;
  const fetchTable = () => (onShortcuts === undefined ? Promise.resolve(null) : ipc.getShortcuts());
  const fetchNotice = () => (onSettingsNotice === undefined ? Promise.resolve(null) : ipc.getSettingsNotice());

  const refresh = async () => {
    latest += 1;
    const seq = latest;
    try {
      const [table, notice] = await Promise.all([fetchTable(), fetchNotice()]);
      if (seq !== latest) return;
      if (onShortcuts !== undefined && table !== null) onShortcuts(table);
      onSettingsNotice?.(notice);
    } catch (cause) {
      report(`設定の変更を取り直せなかったので、今の表のまま動かす: ${describeError(cause)}`);
    }
  };

  try {
    await ipc.onSettingsChanged(() => void refresh());
  } catch (cause) {
    report(`設定の変更を購読できなかったので、起動時の表のまま動かす: ${describeError(cause)}`);
  }

  // 起動時は、取れたほうだけを渡す(表を取れなければ既定の表の見本のまま、知らせを取れなければ知らせ無し)。
  latest += 1;
  const seq = latest;
  const [table, notice] = await Promise.allSettled([fetchTable(), fetchNotice()]);
  // 取得のあいだに設定の変更が届いていたら、その取り直しが新しい結果を渡す。
  if (seq !== latest) return;
  if (table.status === "fulfilled") {
    if (onShortcuts !== undefined && table.value !== null) onShortcuts(table.value);
  } else {
    report(`ショートカットの表を取得できなかったので、既定の表で動かす: ${describeError(table.reason)}`);
  }
  if (notice.status === "fulfilled") onSettingsNotice?.(notice.value);
  else report(`設定の知らせを取得できなかった: ${describeError(notice.reason)}`);
}
