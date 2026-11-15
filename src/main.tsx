// 画面側のエントリーポイントと組み立て(設計書 §11)。
//
// 部品はここで生成して props で渡す。モジュールの読み込み時にストアを作らない。
// 起動の手順そのもの(写しの購読と取得 → 準備完了の通知、失敗の扱い)は src/state/startApp.ts にある。
// ターミナルの接続: 表示中のターミナルは置き場所(TerminalHost)が描かれたときに、隠したままのターミナル(他のタブ・
// 他のプロジェクト。FR-042)は写しが届いたときに、TerminalManager が行う(src/state/terminalSync.ts)。

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { flushSync } from "react-dom";

import type { ArinIpc } from "./ipc/client";
import { createTauriIpc } from "./ipc/tauri";
import { createActions } from "./state/actions";
import { createAppStore } from "./state/appStore";
import { installErrorReporter } from "./state/errorReporter";
import { createWithLaunchOptions } from "./state/launchOptions";
import { startApp, type StartAppResult } from "./state/startApp";
import { syncTerminals } from "./state/terminalSync";
import { createTreeController, createTreeStore } from "./state/treeStore";
import { createUiStore } from "./state/uiStore";
import { TerminalManager } from "./terminal/TerminalManager";
import { XtermEngine } from "./terminal/xterm/XtermEngine";
import { AppShell } from "./ui/shell/AppShell";
import { createTreeClipboard } from "./ui/menu/runTreeMenuCommand";
import { installClipboardRouting } from "./ui/terminals/clipboardRouting";

/**
 * 画面を組み立てて起動する。通信の包みは引数で受け取る。
 * 本物の起動では createTauriIpc() を渡す。ブラウザだけで動かす画面の自動テスト(T11)は、ここに偽の通信を渡す。
 */
export async function boot(ipc: ArinIpc, container: HTMLElement): Promise<StartAppResult> {
  // 画面の失敗(CSP 違反など)を Rust の標準エラーへ流す。何より先に付ける。
  const reporter = installErrorReporter({
    window,
    document,
    report: (message) => ipc.reportFrontendError(message),
  });
  const store = createAppStore();
  const ui = createUiStore();
  // ツリー(M3): 写しの選択中のプロジェクトのルートを読み、tree-changed と保留(uiStore)に合わせて読み直す。
  // ツリーの部品(T43b)へは tree を props で渡す。
  const tree = createTreeStore();
  const treeController = createTreeController({ ipc, tree, app: store, ui, reportError: reporter.report });
  const actions = createActions({ ipc, ui, tree, treeController, reportError: reporter.report });
  // 描画方式と全角幅の方式は起動時の指定で選ぶ(検証用。既定は DOM と unicode11。M4 で決めて外す。ADR-001)。
  // 指定を取ってからエンジンを作る。取れなければ既定で作り、報告する。WebGL を作れなかったときの報告も同じ口へ流す。
  const { engine, verifyGrid } = await createWithLaunchOptions(ipc, reporter.report, (options) => ({
    engine: new XtermEngine({
      renderer: options.renderer,
      unicode: options.unicode,
      reportError: reporter.report,
      // Rust から受け取った表(startApp の onShortcuts)。届くまでは uiStore の既定の表の見本。
      getShortcuts: () => ui.getState().shortcuts,
    }),
    // 9 枚の検証画面(ARIN_VERIFY_GRID。M1 の検証用、M4 の T49 で外す)。
    verifyGrid: options.verifyGrid,
  }));
  const terminals = new TerminalManager(engine, ipc, { reportError: reporter.report });
  // 写しの状態(終わったシェルには入力と寸法を送らない)、消えたターミナルの破棄、表示中と隠すものを伝える。
  syncTerminals(store, terminals, { verifyGrid });
  // ⌘C・⌘X・⌘V(メニューの「編集」から来る copy・cut・paste のイベント)を、フォーカス位置で振り分ける
  // (ツリーにフォーカスがあればツリーの操作、名前の入力中は入力欄、それ以外はターミナル)。
  installClipboardRouting(
    document,
    terminals,
    createTreeClipboard({
      actions,
      ui,
      tree,
      app: store,
      isNameInputFocused: () => document.activeElement?.closest("[data-tree-name-input]") != null,
      isTreeElementFocused: () => document.activeElement?.closest("[data-tree-root]") != null,
    }),
  );
  return startApp({
    ipc,
    store,
    events: {
      // メニューの「すべてを選択」を、フォーカス位置で振り分ける(ターミナルなら xterm.js の全選択)。
      onMenuCommand: (command) => terminals.handleMenuCommand(command),
      // 動作中のターミナルがあって終了が止められたら、終了の確認を開く(ダイアログの部品は T34d)。
      onQuitRequested: (request) => ui.getState().openQuitConfirmation(request.running),
      // 選択中のプロジェクトのツリーの外からの変更(FR-014)。保留中は積む。
      onTreeChanged: (change) => treeController.handleTreeChange(change),
    },
    // ショートカットの表を Rust から受け取る(届くまで・失敗したときは既定の表の見本)。
    onShortcuts: (table) => ui.getState().setShortcuts(table),
    reportError: reporter.report,
    mount: () => {
      // 描き終えてから準備完了を通知するため、描画を同期で済ませる。
      flushSync(() => {
        createRoot(container).render(
          <StrictMode>
            <AppShell store={store} ui={ui} tree={tree} actions={actions} terminals={terminals} verifyGrid={verifyGrid} />
          </StrictMode>,
        );
      });
    },
  });
}

const container = document.getElementById("root");
if (container === null) {
  throw new Error("index.html に #root が無い");
}

void boot(createTauriIpc(), container);
