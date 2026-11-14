// 画面側のエントリーポイントと組み立て(設計書 §11)。
//
// 部品はここで生成して props で渡す。モジュールの読み込み時にストアを作らない。
// 起動の手順そのもの(写しの購読と取得 → 準備完了の通知、失敗の扱い)は src/state/startApp.ts にある。
// ターミナルの接続は、写しにあるターミナルの置き場所(TerminalHost)が描かれたときに TerminalManager が行う。

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { flushSync } from "react-dom";

import type { ArinIpc } from "./ipc/client";
import { createTauriIpc } from "./ipc/tauri";
import { createAppStore } from "./state/appStore";
import { installErrorReporter } from "./state/errorReporter";
import { startApp, type StartAppResult } from "./state/startApp";
import { TerminalManager } from "./terminal/TerminalManager";
import { XtermEngine } from "./terminal/xterm/XtermEngine";
import { AppShell } from "./ui/shell/AppShell";

/**
 * 画面を組み立てて起動する。通信の包みは引数で受け取る。
 * 本物の起動では createTauriIpc() を渡す。ブラウザだけで動かす画面の自動テスト(T11)は、ここに偽の通信を渡す。
 */
export function boot(ipc: ArinIpc, container: HTMLElement): Promise<StartAppResult> {
  // 画面の失敗(CSP 違反など)を Rust の標準エラーへ流す。何より先に付ける。
  const reporter = installErrorReporter({
    window,
    document,
    report: (message) => ipc.reportFrontendError(message),
  });
  const store = createAppStore();
  const terminals = new TerminalManager(new XtermEngine(), ipc, { reportError: reporter.report });
  return startApp({
    ipc,
    store,
    mount: () => {
      // 描き終えてから準備完了を通知するため、描画を同期で済ませる。
      flushSync(() => {
        createRoot(container).render(
          <StrictMode>
            <AppShell store={store} terminals={terminals} />
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
