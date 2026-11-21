import { describe, expect, it } from "vitest";

import { isTreeArrowKey, treeArrowAction } from "./treeKeys";
import type { TreeRow } from "./treeModel";

const dir = (path: string, depth: number, expanded: boolean, loading = false): TreeRow => ({
  path,
  name: path.slice(path.lastIndexOf("/") + 1),
  kind: "directory",
  depth,
  expandable: true,
  expanded,
  loading,
});
const file = (path: string, depth: number): TreeRow => ({
  path,
  name: path.slice(path.lastIndexOf("/") + 1),
  kind: "file",
  depth,
  expandable: false,
  expanded: false,
  loading: false,
});

// docs(閉) / src(開) ─ src/ui(開) ─ src/ui/App.tsx、src/main.ts / README.md
const ROWS: readonly TreeRow[] = [
  dir("docs", 0, false),
  dir("src", 0, true),
  dir("src/ui", 1, true),
  file("src/ui/App.tsx", 2),
  file("src/main.ts", 1),
  file("README.md", 0),
];

describe("isTreeArrowKey", () => {
  it("4 つの矢印だけを受ける", () => {
    expect(["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].every(isTreeArrowKey)).toBe(true);
    expect(["Enter", "Home", "a", " "].some(isTreeArrowKey)).toBe(false);
  });
});

describe("treeArrowAction(↓ / ↑)", () => {
  it("↓ は次の行、↑ は前の行を選ぶ(階層をまたぐ)", () => {
    expect(treeArrowAction(ROWS, "src/ui", "ArrowDown")).toEqual({ type: "select", path: "src/ui/App.tsx" });
    expect(treeArrowAction(ROWS, "src/ui/App.tsx", "ArrowDown")).toEqual({ type: "select", path: "src/main.ts" });
    expect(treeArrowAction(ROWS, "src/main.ts", "ArrowUp")).toEqual({ type: "select", path: "src/ui/App.tsx" });
    expect(treeArrowAction(ROWS, "src", "ArrowUp")).toEqual({ type: "select", path: "docs" });
  });

  it("端では止まる", () => {
    expect(treeArrowAction(ROWS, "README.md", "ArrowDown")).toBeNull();
    expect(treeArrowAction(ROWS, "docs", "ArrowUp")).toBeNull();
  });

  it("何も選んでいなければ ↓ は先頭、↑ は末尾", () => {
    expect(treeArrowAction(ROWS, null, "ArrowDown")).toEqual({ type: "select", path: "docs" });
    expect(treeArrowAction(ROWS, null, "ArrowUp")).toEqual({ type: "select", path: "README.md" });
  });

  it("ルートの見出しを選んでいるとき ↓ は先頭の行、↑ は動かない", () => {
    expect(treeArrowAction(ROWS, "", "ArrowDown")).toEqual({ type: "select", path: "docs" });
    expect(treeArrowAction(ROWS, "", "ArrowUp")).toBeNull();
  });

  it("選択中の行が見えなくなっていたら(親を閉じた)、選択なしと同じ", () => {
    expect(treeArrowAction(ROWS, "docs/gone.md", "ArrowDown")).toEqual({ type: "select", path: "docs" });
    expect(treeArrowAction(ROWS, "docs/gone.md", "ArrowUp")).toEqual({ type: "select", path: "README.md" });
  });

  it("行が無ければ何もしない", () => {
    expect(treeArrowAction([], null, "ArrowDown")).toBeNull();
    expect(treeArrowAction([], null, "ArrowUp")).toBeNull();
    expect(treeArrowAction([], "", "ArrowDown")).toBeNull();
  });
});

describe("treeArrowAction(→)", () => {
  it("閉じたフォルダは開く", () => {
    expect(treeArrowAction(ROWS, "docs", "ArrowRight")).toEqual({ type: "expand", path: "docs" });
  });

  it("開いているフォルダは最初の子へ移る", () => {
    expect(treeArrowAction(ROWS, "src", "ArrowRight")).toEqual({ type: "select", path: "src/ui" });
  });

  it("開いているが子がまだ無い(空・読み込み中)フォルダでは動かない", () => {
    const rows = [dir("a", 0, true, true), file("b.txt", 0)];
    expect(treeArrowAction(rows, "a", "ArrowRight")).toBeNull();
  });

  it("ファイル・フォルダへのリンク・選択なし・ルートでは何もしない", () => {
    const link: TreeRow = { ...file("link", 0), kind: "directoryLink" };
    expect(treeArrowAction(ROWS, "README.md", "ArrowRight")).toBeNull();
    expect(treeArrowAction([link], "link", "ArrowRight")).toBeNull();
    expect(treeArrowAction(ROWS, null, "ArrowRight")).toBeNull();
    expect(treeArrowAction(ROWS, "", "ArrowRight")).toBeNull();
  });
});

describe("treeArrowAction(←)", () => {
  it("開いているフォルダは閉じる", () => {
    expect(treeArrowAction(ROWS, "src/ui", "ArrowLeft")).toEqual({ type: "collapse", path: "src/ui" });
  });

  it("ファイルと閉じたフォルダは親のフォルダへ移る(親は閉じない)", () => {
    expect(treeArrowAction(ROWS, "src/ui/App.tsx", "ArrowLeft")).toEqual({ type: "select", path: "src/ui" });
    expect(treeArrowAction(ROWS, "src/main.ts", "ArrowLeft")).toEqual({ type: "select", path: "src" });
    const rows = [dir("src", 0, true), dir("src/ui", 1, false)];
    expect(treeArrowAction(rows, "src/ui", "ArrowLeft")).toEqual({ type: "select", path: "src" });
  });

  it("いちばん上の階層のファイル・閉じたフォルダ、選択なし、ルートでは何もしない", () => {
    expect(treeArrowAction(ROWS, "README.md", "ArrowLeft")).toBeNull();
    expect(treeArrowAction(ROWS, "docs", "ArrowLeft")).toBeNull();
    expect(treeArrowAction(ROWS, null, "ArrowLeft")).toBeNull();
    expect(treeArrowAction(ROWS, "", "ArrowLeft")).toBeNull();
  });
});
