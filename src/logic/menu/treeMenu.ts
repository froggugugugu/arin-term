// ツリーの右クリックのメニュー(FR-024、FR-025、FR-035、PRD §6.4、設計書 §9.1)。DOM を使わない純粋な関数。
//
// 画面(src/ui/menu)は、ここで組み立てた項目をそのまま映し、選ばれた項目の `command` を操作に渡す。
// M3 に出すのは PRD §6.4 の Must の項目だけ(計画【仮定】7)。キーの表示はショートカットの表から引く(前提 (e))。
// ショートカットで同じ操作をするときも `treeShortcutCommand` でここの項目から引き、有効・無効をメニューとそろえる。
//
// パスはルート基準の相対パス(区切りは `/`、空の文字列はルート)。src/logic は生成した型を import しないので、文字列で持つ。

import { type ShortcutCommandId, type ShortcutTable, shortcutLabel } from "../keys/shortcutTable";

/**
 * メニューを開いた場所。
 * - `file`: ファイルの行(【仮定】リンクの行もここ。フォルダへのリンクは展開しないので、中に作らない)
 * - `folder`: フォルダの行
 * - `root`: ルート(ツリーのヘッダ。計画【仮定】1)
 * - `blank`: 余白(=ルート)
 */
export type TreeMenuTarget =
  | { readonly kind: "file"; readonly path: string }
  | { readonly kind: "folder"; readonly path: string }
  | { readonly kind: "root" }
  | { readonly kind: "blank" };

/** メニューを組み立てるのに要る、画面の値。 */
export interface TreeMenuContext {
  /** 選択中のプロジェクトに表示中のタブがあるか(無ければ「パスをターミナルに挿入」は無効。計画【仮定】16)。 */
  readonly hasTab: boolean;
  /** 隠しファイルを表示しているか(余白の「隠しファイルを表示」の印)。 */
  readonly showHidden: boolean;
  /** ショートカットの表(項目のキーの表示に使う)。 */
  readonly shortcuts: ShortcutTable;
}

/** 選ばれたときの操作。`path`・`parent`・`destination` はルート基準の相対パス(空の文字列はルート)。 */
export type TreeMenuCommand =
  | { readonly type: "open"; readonly path: string }
  | { readonly type: "reveal"; readonly path: string }
  | { readonly type: "insert-path"; readonly path: string }
  | { readonly type: "copy-path"; readonly path: string; readonly style: "relative" | "absolute" }
  | { readonly type: "cut"; readonly path: string }
  | { readonly type: "copy"; readonly path: string }
  | { readonly type: "paste"; readonly destination: string }
  | { readonly type: "rename"; readonly path: string }
  | { readonly type: "trash"; readonly path: string }
  | { readonly type: "new-file"; readonly parent: string }
  | { readonly type: "new-folder"; readonly parent: string }
  | { readonly type: "toggle-hidden" }
  | { readonly type: "collapse-all" };

/** メニューの 1 項目。`shortcut` は表のキーの表示(無ければ null)。`checked` は切替の項目だけが持つ。 */
export interface TreeMenuItem {
  readonly kind: "command";
  readonly label: string;
  readonly command: TreeMenuCommand;
  readonly enabled: boolean;
  /** 同じ操作のショートカットのコマンドの ID(無ければ null)。 */
  readonly shortcutId: ShortcutCommandId | null;
  readonly shortcut: string | null;
  readonly checked?: boolean;
}

/** メニューの 1 行。 */
export type TreeMenuEntry = TreeMenuItem | { readonly kind: "separator" };

const SEPARATOR: TreeMenuEntry = { kind: "separator" };

/** 相対パスの親(`src/a.ts` → `src`、`a.ts` → ルート)。 */
function parentOf(path: string): string {
  const slash = path.lastIndexOf("/");
  return slash < 0 ? "" : path.slice(0, slash);
}

/** 新規作成・ペーストの置き場所: フォルダならその中、ファイルなら同じ階層、ルートと余白ならルート(FR-025、FR-027)。 */
export function creationParent(target: TreeMenuTarget): string {
  switch (target.kind) {
    case "folder":
      return target.path;
    case "file":
      return parentOf(target.path);
    case "root":
    case "blank":
      return "";
  }
}

/**
 * ツリーの右クリックのメニューを組み立てる。
 * - ファイル・フォルダ・ルート: 開く・Finder で表示 / パスをターミナルに挿入 / 相対パス・絶対パスをコピー /
 *   切り取り・コピー・ペースト / 名称変更…・ゴミ箱に入れる / 新規ファイル…・新規フォルダ…(PRD §6.4 の Must の項目と区切り)
 * - ルートでは名称変更・切り取り・ゴミ箱に入れるを無効にする(FR-035、AC-T23)
 * - 余白: 新規ファイル…・新規フォルダ… / ペースト / Finder で表示・ルートの絶対パスをコピー / 隠しファイルを表示・すべて折りたたむ
 */
export function buildTreeMenu(target: TreeMenuTarget, context: TreeMenuContext): TreeMenuEntry[] {
  const item = (
    label: string,
    command: TreeMenuCommand,
    shortcutId: ShortcutCommandId | null,
    enabled = true,
  ): TreeMenuItem => ({
    kind: "command",
    label,
    command,
    enabled,
    shortcutId,
    shortcut: shortcutId === null ? null : shortcutLabel(context.shortcuts, shortcutId),
  });
  const parent = creationParent(target);
  const newItems = [
    item("新規ファイル…", { type: "new-file", parent }, "tree.new-file"),
    item("新規フォルダ…", { type: "new-folder", parent }, "tree.new-folder"),
  ];
  const paste = item("ペースト", { type: "paste", destination: parent }, "edit.paste");

  if (target.kind === "blank") {
    return [
      ...newItems,
      SEPARATOR,
      paste,
      SEPARATOR,
      item("Finder で表示", { type: "reveal", path: "" }, "tree.reveal-in-finder"),
      item("ルートの絶対パスをコピー", { type: "copy-path", path: "", style: "absolute" }, "tree.copy-absolute-path"),
      SEPARATOR,
      { ...item("隠しファイルを表示", { type: "toggle-hidden" }, null), checked: context.showHidden },
      item("すべて折りたたむ", { type: "collapse-all" }, null),
    ];
  }

  const path = target.kind === "root" ? "" : target.path;
  const movable = target.kind !== "root";
  return [
    item("開く", { type: "open", path }, "tree.open"),
    item("Finder で表示", { type: "reveal", path }, "tree.reveal-in-finder"),
    SEPARATOR,
    item("パスをターミナルに挿入", { type: "insert-path", path }, "tree.insert-path", context.hasTab),
    SEPARATOR,
    item("相対パスをコピー", { type: "copy-path", path, style: "relative" }, "tree.copy-relative-path"),
    item("絶対パスをコピー", { type: "copy-path", path, style: "absolute" }, "tree.copy-absolute-path"),
    SEPARATOR,
    item("切り取り", { type: "cut", path }, "edit.cut", movable),
    item("コピー", { type: "copy", path }, "edit.copy"),
    paste,
    SEPARATOR,
    item("名称変更…", { type: "rename", path }, "tree.rename", movable),
    item("ゴミ箱に入れる", { type: "trash", path }, "tree.trash", movable),
    SEPARATOR,
    ...newItems,
  ];
}

/**
 * ショートカットのコマンドの ID を、その場所のメニューの同じ項目の操作にする(FR-024「同じ操作はショートカットでも」)。
 * メニューに無い・無効の項目なら null(何もしない)。ルートでの ⌘⌫ などはここで止まる(FR-035)。
 */
export function treeShortcutCommand(
  id: ShortcutCommandId,
  target: TreeMenuTarget,
  context: TreeMenuContext,
): TreeMenuCommand | null {
  for (const entry of buildTreeMenu(target, context)) {
    if (entry.kind === "command" && entry.shortcutId === id) return entry.enabled ? entry.command : null;
  }
  return null;
}
