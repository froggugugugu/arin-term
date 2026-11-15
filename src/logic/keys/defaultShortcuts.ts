// ショートカットの既定の表の見本(計画 T41b。前提 (e)【要件の追加(本人。2026-11-15)】)。
//
// 正本は Rust の `arin-core/shortcuts`(計画 T38d)。画面は起動時に `get_shortcuts`(T42e)で表を受け取り、
// `uiStore.shortcuts` に持つ(T43a。`XtermEngine` もそこから読む)。src/logic は src/ipc を import しないので、
// ここの関数は表を引数で受け取る。この見本は次のためだけに置く:
//   - 偽の通信(`src/test/fakeIpc.ts`・`e2e/fixtures/tauriMock.ts`)の `get_shortcuts` の応答と、Vitest の見本
//   - 画面が Rust から表を受け取るまでと、受け取りに失敗したときの既定(`uiStore` の初期値・`XtermEngine` の既定)
// 生成物 `src/ipc/generated/defaultShortcuts.json` と同じ中身であることは `scripts/default-shortcuts.test.mjs` が確かめる
// (src/ と e2e/ の tsconfig は JSON を対象に含めないので、見本を TypeScript で持つ)。

import type { ShortcutTable } from "./shortcutTable";

/** 既定のショートカットの表(T38d の既定の表と同じ内容。順も同じ)。 */
export const DEFAULT_SHORTCUT_TABLE: ShortcutTable = [
  { command: "terminal.new-tab", accelerator: "Cmd+T", receiver: "menu-item" },
  { command: "terminal.close-tab", accelerator: "Cmd+W", receiver: "menu-item" },
  { command: "terminal.previous-tab", accelerator: "Cmd+Shift+[", receiver: "menu-item" },
  { command: "terminal.next-tab", accelerator: "Cmd+Shift+]", receiver: "menu-item" },
  { command: "edit.select-all", accelerator: "Cmd+A", receiver: "menu-item" },
  { command: "app.quit", accelerator: "Cmd+Q", receiver: "menu-item" },
  { command: "app.open-settings", accelerator: "Cmd+,", receiver: "menu-item" },
  { command: "edit.copy", accelerator: "Cmd+C", receiver: "edit-item" },
  { command: "edit.cut", accelerator: "Cmd+X", receiver: "edit-item" },
  { command: "edit.paste", accelerator: "Cmd+V", receiver: "edit-item" },
  { command: "tree.open", accelerator: "Cmd+O", receiver: "screen" },
  { command: "tree.reveal-in-finder", accelerator: "Cmd+Alt+R", receiver: "screen" },
  { command: "tree.insert-path", accelerator: "Cmd+Enter", receiver: "screen" },
  { command: "tree.copy-relative-path", accelerator: "Cmd+Shift+Alt+C", receiver: "screen" },
  { command: "tree.copy-absolute-path", accelerator: "Cmd+Alt+C", receiver: "screen" },
  { command: "tree.rename", accelerator: "Enter", receiver: "screen" },
  { command: "tree.trash", accelerator: "Cmd+Backspace", receiver: "screen" },
  { command: "tree.new-file", accelerator: "Cmd+N", receiver: "screen" },
  { command: "tree.new-folder", accelerator: "Cmd+Shift+N", receiver: "screen" },
  { command: "view.toggle-display-mode", accelerator: "Cmd+Shift+G", receiver: "menu-item" },
  { command: "view.previous-page", accelerator: "Cmd+Alt+ArrowLeft", receiver: "menu-item" },
  { command: "view.next-page", accelerator: "Cmd+Alt+ArrowRight", receiver: "menu-item" },
  { command: "view.toggle-maximize", accelerator: "Cmd+Shift+Enter", receiver: "menu-item" },
];
