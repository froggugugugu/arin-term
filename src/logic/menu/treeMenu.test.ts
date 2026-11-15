import { describe, expect, it } from "vitest";

import { DEFAULT_SHORTCUT_TABLE } from "../keys/defaultShortcuts";
import type { ShortcutTable } from "../keys/shortcutTable";
import {
  buildTreeMenu,
  creationParent,
  type TreeMenuContext,
  type TreeMenuEntry,
  type TreeMenuItem,
  type TreeMenuTarget,
  treeShortcutCommand,
} from "./treeMenu";

const context = (overrides: Partial<TreeMenuContext> = {}): TreeMenuContext => ({
  hasTab: true,
  showHidden: false,
  shortcuts: DEFAULT_SHORTCUT_TABLE,
  ...overrides,
});

const items = (entries: readonly TreeMenuEntry[]): TreeMenuItem[] =>
  entries.flatMap((entry) => (entry.kind === "separator" ? [] : [entry]));
const labels = (entries: readonly TreeMenuEntry[]): string[] => items(entries).map((item) => item.label);
const disabled = (entries: readonly TreeMenuEntry[]): string[] =>
  items(entries)
    .filter((item) => !item.enabled)
    .map((item) => item.label);
const byLabel = (entries: readonly TreeMenuEntry[], label: string): TreeMenuItem => {
  const item = items(entries).find((entry) => entry.label === label);
  if (item === undefined) throw new Error(`項目が無い: ${label}`);
  return item;
};

const file: TreeMenuTarget = { kind: "file", path: "src/a b.ts" };
const folder: TreeMenuTarget = { kind: "folder", path: "src" };
const root: TreeMenuTarget = { kind: "root" };
const blank: TreeMenuTarget = { kind: "blank" };

/** PRD §6.4 の表の Must の項目(この順)。 */
const ENTRY_LABELS = [
  "開く",
  "Finder で表示",
  "パスをターミナルに挿入",
  "相対パスをコピー",
  "絶対パスをコピー",
  "切り取り",
  "コピー",
  "ペースト",
  "名称変更…",
  "ゴミ箱に入れる",
  "新規ファイル…",
  "新規フォルダ…",
];

describe("buildTreeMenu(ツリーの右クリックのメニュー)", () => {
  it("ファイルの上のメニューは_prd_の表の_must_の項目がこの順", () => {
    const menu = buildTreeMenu(file, context());
    expect(labels(menu)).toEqual(ENTRY_LABELS);
    // PRD §6.4 の区切り(開く / ターミナル / パス / 編集 / 変更 / 新規)の間に区切りが入る
    expect(menu.map((entry) => (entry.kind === "separator" ? "|" : "*")).join("")).toBe("**|*|**|***|**|**");
    expect(disabled(menu)).toEqual([]);
    expect(byLabel(menu, "開く").command).toEqual({ type: "open", path: "src/a b.ts" });
    expect(byLabel(menu, "Finder で表示").command).toEqual({ type: "reveal", path: "src/a b.ts" });
    expect(byLabel(menu, "パスをターミナルに挿入").command).toEqual({ type: "insert-path", path: "src/a b.ts" });
    expect(byLabel(menu, "相対パスをコピー").command).toEqual({
      type: "copy-path",
      path: "src/a b.ts",
      style: "relative",
    });
    expect(byLabel(menu, "絶対パスをコピー").command).toEqual({
      type: "copy-path",
      path: "src/a b.ts",
      style: "absolute",
    });
    expect(byLabel(menu, "切り取り").command).toEqual({ type: "cut", path: "src/a b.ts" });
    expect(byLabel(menu, "コピー").command).toEqual({ type: "copy", path: "src/a b.ts" });
    expect(byLabel(menu, "名称変更…").command).toEqual({ type: "rename", path: "src/a b.ts" });
    expect(byLabel(menu, "ゴミ箱に入れる").command).toEqual({ type: "trash", path: "src/a b.ts" });
  });

  it("フォルダの上のメニューも同じ", () => {
    const menu = buildTreeMenu(folder, context());
    expect(labels(menu)).toEqual(ENTRY_LABELS);
    expect(disabled(menu)).toEqual([]);
    expect(byLabel(menu, "開く").command).toEqual({ type: "open", path: "src" });
  });

  it("余白のメニューは新規ファイル_新規フォルダ_ペースト_finder_で表示_ルートの絶対パスをコピー_隠しファイルを表示_すべて折りたたむ", () => {
    const menu = buildTreeMenu(blank, context());
    expect(labels(menu)).toEqual([
      "新規ファイル…",
      "新規フォルダ…",
      "ペースト",
      "Finder で表示",
      "ルートの絶対パスをコピー",
      "隠しファイルを表示",
      "すべて折りたたむ",
    ]);
    expect(disabled(menu)).toEqual([]);
    expect(byLabel(menu, "ペースト").command).toEqual({ type: "paste", destination: "" });
    expect(byLabel(menu, "Finder で表示").command).toEqual({ type: "reveal", path: "" });
    expect(byLabel(menu, "ルートの絶対パスをコピー").command).toEqual({
      type: "copy-path",
      path: "",
      style: "absolute",
    });
    expect(byLabel(menu, "隠しファイルを表示").command).toEqual({ type: "toggle-hidden" });
    expect(byLabel(menu, "すべて折りたたむ").command).toEqual({ type: "collapse-all" });
  });

  it("隠しファイルを表示の印は、いまの切替を映す", () => {
    expect(byLabel(buildTreeMenu(blank, context()), "隠しファイルを表示").checked).toBe(false);
    expect(byLabel(buildTreeMenu(blank, context({ showHidden: true })), "隠しファイルを表示").checked).toBe(true);
  });

  it("ルートでは名称変更_切り取り_ゴミ箱が無効", () => {
    const menu = buildTreeMenu(root, context());
    expect(labels(menu)).toEqual(ENTRY_LABELS);
    expect(disabled(menu)).toEqual(["切り取り", "名称変更…", "ゴミ箱に入れる"]);
    expect(byLabel(menu, "コピー").command).toEqual({ type: "copy", path: "" });
  });

  it("タブが無いとパスをターミナルに挿入が無効", () => {
    for (const target of [file, folder, root]) {
      expect(disabled(buildTreeMenu(target, context({ hasTab: false })))).toContain("パスをターミナルに挿入");
    }
  });

  it("メニューの項目に表のキーが出る", () => {
    const menu = buildTreeMenu(file, context());
    expect(items(menu).map((item) => [item.label, item.shortcut])).toEqual([
      ["開く", "⌘O"],
      ["Finder で表示", "⌥⌘R"],
      ["パスをターミナルに挿入", "⌘↩"],
      ["相対パスをコピー", "⌥⇧⌘C"],
      ["絶対パスをコピー", "⌥⌘C"],
      ["切り取り", "⌘X"],
      ["コピー", "⌘C"],
      ["ペースト", "⌘V"],
      ["名称変更…", "↩"],
      ["ゴミ箱に入れる", "⌘⌫"],
      ["新規ファイル…", "⌘N"],
      ["新規フォルダ…", "⇧⌘N"],
    ]);
    const blankMenu = buildTreeMenu(blank, context());
    expect(byLabel(blankMenu, "隠しファイルを表示").shortcut).toBeNull();
    expect(byLabel(blankMenu, "すべて折りたたむ").shortcut).toBeNull();
  });

  it("表のキーを変えるとメニューの表示も変わり、表に無い行はキーを出さない", () => {
    const table: ShortcutTable = DEFAULT_SHORTCUT_TABLE.flatMap((binding) => {
      if (binding.command === "tree.trash") return [];
      return binding.command === "tree.open" ? [{ ...binding, accelerator: "Cmd+Shift+O" }] : [binding];
    });
    const menu = buildTreeMenu(file, context({ shortcuts: table }));
    expect(byLabel(menu, "開く").shortcut).toBe("⇧⌘O");
    expect(byLabel(menu, "ゴミ箱に入れる").shortcut).toBeNull();
  });

  it("新規作成の作成先はフォルダならその中_ファイルなら同じ階層_余白ならルート", () => {
    const parents = (target: TreeMenuTarget) => {
      const menu = buildTreeMenu(target, context());
      return [byLabel(menu, "新規ファイル…").command, byLabel(menu, "新規フォルダ…").command];
    };
    expect(parents(folder)).toEqual([
      { type: "new-file", parent: "src" },
      { type: "new-folder", parent: "src" },
    ]);
    expect(parents(file)).toEqual([
      { type: "new-file", parent: "src" },
      { type: "new-folder", parent: "src" },
    ]);
    expect(parents({ kind: "file", path: "README.md" })).toEqual([
      { type: "new-file", parent: "" },
      { type: "new-folder", parent: "" },
    ]);
    expect(parents(blank)).toEqual([
      { type: "new-file", parent: "" },
      { type: "new-folder", parent: "" },
    ]);
    expect(parents(root)).toEqual([
      { type: "new-file", parent: "" },
      { type: "new-folder", parent: "" },
    ]);
  });

  it("ペーストの貼り付け先も、フォルダならその中_ファイルなら同じ階層", () => {
    expect(byLabel(buildTreeMenu(folder, context()), "ペースト").command).toEqual({
      type: "paste",
      destination: "src",
    });
    expect(byLabel(buildTreeMenu(file, context()), "ペースト").command).toEqual({
      type: "paste",
      destination: "src",
    });
    expect(byLabel(buildTreeMenu(root, context()), "ペースト").command).toEqual({ type: "paste", destination: "" });
  });
});

describe("creationParent(新規作成・ペーストの置き場所)", () => {
  it("フォルダはその中、ファイルは親、ルートと余白はルート", () => {
    expect(creationParent({ kind: "folder", path: "src/lib" })).toBe("src/lib");
    expect(creationParent({ kind: "file", path: "src/lib/a.ts" })).toBe("src/lib");
    expect(creationParent({ kind: "file", path: "a.ts" })).toBe("");
    expect(creationParent(root)).toBe("");
    expect(creationParent(blank)).toBe("");
  });
});

describe("treeShortcutCommand(ショートカットのコマンドの id をメニューと同じ操作にする)", () => {
  it("メニューにある有効な項目の操作を返す", () => {
    expect(treeShortcutCommand("tree.trash", file, context())).toEqual({ type: "trash", path: "src/a b.ts" });
    expect(treeShortcutCommand("tree.new-file", file, context())).toEqual({ type: "new-file", parent: "src" });
    expect(treeShortcutCommand("edit.paste", folder, context())).toEqual({ type: "paste", destination: "src" });
    expect(treeShortcutCommand("tree.copy-absolute-path", blank, context())).toEqual({
      type: "copy-path",
      path: "",
      style: "absolute",
    });
  });

  it("無効の項目と、メニューに無い id は null", () => {
    expect(treeShortcutCommand("tree.trash", root, context())).toBeNull();
    expect(treeShortcutCommand("tree.rename", root, context())).toBeNull();
    expect(treeShortcutCommand("edit.cut", root, context())).toBeNull();
    expect(treeShortcutCommand("tree.insert-path", file, context({ hasTab: false }))).toBeNull();
    expect(treeShortcutCommand("tree.rename", blank, context())).toBeNull();
    expect(treeShortcutCommand("terminal.new-tab", file, context())).toBeNull();
  });

  it("表に無い行でも、メニューにある操作なら返す(キーの表示が無いだけ)", () => {
    expect(treeShortcutCommand("tree.open", file, context({ shortcuts: [] }))).toEqual({
      type: "open",
      path: "src/a b.ts",
    });
  });
});
