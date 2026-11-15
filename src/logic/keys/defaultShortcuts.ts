// ショートカットの既定の表の見本(計画 T41b。前提 (e)【要件の追加(本人。2026-11-15)】)。
//
// 正本は Rust の `arin-core/shortcuts`(計画 T38d)で、既定の表は `src/ipc/generated/defaultShortcuts.json` に生成される予定。
// src/logic は src/ipc を import しないので、ここの関数は表を引数で受け取る。この見本は次の 2 つのためだけに置く:
//   - Vitest の見本(T38d の既定の表と同じ内容)
//   - 画面が Rust から表を受け取るまでの仮の値(`XtermEngine` の既定。T43a で `get_shortcuts` の表に替える)
// 【仮定】T38d の生成物ができたら、`defaultShortcuts.json` と中身が同じであることを確かめるテストを足し、
// 画面の仮の値としての使い方はやめる(キーの値を 2 か所で持たないため)。

import type { ShortcutTable } from "./shortcutTable";

/** 既定のショートカットの表(T38d の既定の表と同じ内容。順も同じ)。 */
export const DEFAULT_SHORTCUT_TABLE: ShortcutTable = [
  { command: "terminal.new-tab", accelerator: "Cmd+T", receiver: "menu-item" },
  { command: "terminal.close-tab", accelerator: "Cmd+W", receiver: "menu-item" },
  { command: "terminal.previous-tab", accelerator: "Cmd+Shift+[", receiver: "menu-item" },
  { command: "terminal.next-tab", accelerator: "Cmd+Shift+]", receiver: "menu-item" },
  { command: "edit.select-all", accelerator: "Cmd+A", receiver: "menu-item" },
  { command: "app.quit", accelerator: "Cmd+Q", receiver: "menu-item" },
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
];
