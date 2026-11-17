// 画面側のエントリーポイントと組み立て(設計書 §11)。
//
// 部品はここで生成して props で渡す。モジュールの読み込み時にストアを作らない。
// 起動の手順そのもの(写しの購読と取得 → 準備完了の通知、失敗の扱い)は src/state/startApp.ts にある。
// ターミナルの接続: 表示中のターミナルは置き場所(TerminalHost)が描かれたときに、隠したままのターミナル(他のタブ・
// 他のプロジェクト。FR-042)は写しが届いたときに、TerminalManager が行う(src/state/terminalSync.ts)。
// エディタ(M4.8。ADR-015): 部品は EditorManager が React の外で持ち(CodeMirror は初めて開くときに動的 import で読む)、
// 操作と写しへの追従は src/state/editorController.ts。ウィンドウが前面に戻ったら開いている文書が外で変わったかを確かめる。

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { flushSync } from "react-dom";

import { EditorManager } from "./editor/EditorManager";
import type { ArinIpc } from "./ipc/client";
import { createTauriIpc } from "./ipc/tauri";
import { createActions } from "./state/actions";
import { createAppStore } from "./state/appStore";
import { createEditorController, syncEditors } from "./state/editorController";
import { installErrorReporter } from "./state/errorReporter";
import { startApp, type StartAppResult } from "./state/startApp";
import { syncTerminals } from "./state/terminalSync";
import { createTreeController, createTreeStore } from "./state/treeStore";
import { createUiStore } from "./state/uiStore";
import { routeMenuCommand } from "./logic/keys/menuCommand";
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
  // エディタ(M4.8)。部品の知らせ(未保存の変わり目・保存のキー・プレビューの木)をコントローラーと uiStore へつなぐ。
  // コールバックは部品ができてから(開いたあと)しか呼ばれないので、下で作る editor を引いてよい。
  const editors = new EditorManager({
    shortcuts: () => ui.getState().shortcuts,
    onDirtyChange: (id, dirty) => editor.handleDirtyChange(id, dirty),
    onSave: (id) => void editor.save(id),
    onPreview: (id, blocks) => ui.getState().setEditorPreview(id, blocks),
    // 文字の大きさと Vim は写しが届くと syncEditors が当てる(部品は初めて開くときに作るので、それまでに届く)
    fontSize: 15,
    vim: false,
    reportError: reporter.report,
  });
  const editor = createEditorController({
    ipc,
    app: store,
    ui,
    editors: () => editors,
    reportError: reporter.report,
  });
  syncEditors({ app: store, ui, editors, controller: editor });
  window.addEventListener("focus", () => editor.checkAll());
  const actions = createActions({ ipc, ui, app: store, tree, treeController, reportError: reporter.report, editor });
  // 描画方式は DOM、全角幅は unicode11 に固定(M4 で決めた。ADR-001)。
  const engine = new XtermEngine({
    // Rust から受け取った表(startApp の onShortcuts。設定が変わると置き換わる)。届くまでは uiStore の既定の表の見本。
    getShortcuts: () => ui.getState().shortcuts,
  });
  const terminals = new TerminalManager(engine, ipc, { reportError: reporter.report });
  // 写しの状態(終わったシェルには入力と寸法を送らない)、消えたターミナルの破棄、表示中と隠すものを伝える。
  syncTerminals(store, terminals);
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
      // メニューの操作を振り分ける。「設定…」(⌘, もメニューの項目として届く。ADR-013 の 6)は設定画面を開き
      // (開いていれば 1 つのまま、ほかの確認が開いていれば開かない)、ターミナルには渡さない。
      // 「すべてを選択」はフォーカス位置で振り分ける(エディタなら文書の全体、ターミナルなら xterm.js の全選択)。
      onMenuCommand: (command) => {
        if (command === "open-settings") ui.getState().openSettings();
        else if (routeMenuCommand(command, { kind: editors.focusedId() === null ? "other" : "editor" }) === "select-all-editor") {
          editors.selectAllFocused();
        } else terminals.handleMenuCommand(command);
      },
      // 動作中のターミナルか未保存のエディタがあって終了が止められたら、終了の確認を開く(ダイアログの部品は T34d)。
      onQuitRequested: (request) => ui.getState().openQuitConfirmation(request.running, request.unsaved),
      // 選択中のプロジェクトのツリーの外からの変更(FR-014)。保留中は積む。開いている文書が外で変わったかも確かめる。
      onTreeChanged: (change) => {
        treeController.handleTreeChange(change);
        editor.handleTreeChange(change);
      },
      // ⌘W で表示中のエディタが未保存だった(M4.8)。閉じる確認を開く。
      onEditorCloseRequested: (request) => editor.handleCloseRequested(request.editorId),
    },
    // ショートカットの表と設定の知らせを Rust から受け取る(届くまで・失敗したときは既定の表の見本)。
    // 設定が変わったと届くたびに startApp が取り直して渡す(T65e)。
    onShortcuts: (table) => ui.getState().setShortcuts(table),
    onSettingsNotice: (notice) => ui.getState().setSettingsNotice(notice),
    reportError: reporter.report,
    mount: () => {
      // 描き終えてから準備完了を通知するため、描画を同期で済ませる。
      flushSync(() => {
        createRoot(container).render(
          <StrictMode>
            <AppShell
              store={store}
              ui={ui}
              tree={tree}
              actions={actions}
              terminals={terminals}
              editor={editor}
              editors={editors}
            />
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
