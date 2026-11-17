import { describe, expect, it } from "vitest";

import {
  applyDirListing,
  collapseAll,
  collapseDir,
  createTreeModel,
  dirsToReload,
  expandDir,
  flattenRows,
  forgetDir,
  isLoaded,
  loadedDirs,
  selectPath,
  type TreeModel,
  type TreeRow,
  toggleDir,
} from "./treeModel";
import type { TreeEntry } from "./types";

const folder = (name: string): TreeEntry => ({ name, kind: "directory" });
const file = (name: string): TreeEntry => ({ name, kind: "file" });

/** 行を「字下げ + 名前」の文字列にして比べやすくする。 */
const outline = (rows: readonly TreeRow[]): string[] =>
  rows.map((row) => `${"  ".repeat(row.depth)}${row.name}`);

/** ルートに src(フォルダ)・docs(フォルダ)・README.md を読み込んだモデル。 */
function withRoot(): TreeModel {
  return applyDirListing(createTreeModel(), "", [folder("src"), folder("docs"), file("README.md")]);
}

/** フォルダを展開して中身を読み込む。 */
function open(model: TreeModel, dir: string, entries: readonly TreeEntry[]): TreeModel {
  return applyDirListing(expandDir(model, dir).model, dir, entries);
}

describe("ツリーの読み込みと行(FR-013)", () => {
  it("作った直後はルートの読み込みを待ち_行は無い", () => {
    const model = createTreeModel();

    expect(isLoaded(model, "")).toBe(false);
    expect(model.loading.has("")).toBe(true);
    expect(flattenRows(model)).toEqual([]);
  });

  it("ルートを読み込むと直下の行が深さ 0 で並ぶ", () => {
    const rows = flattenRows(withRoot());

    expect(rows).toEqual([
      { path: "src", name: "src", kind: "directory", depth: 0, expandable: true, expanded: false, loading: false },
      { path: "docs", name: "docs", kind: "directory", depth: 0, expandable: true, expanded: false, loading: false },
      {
        path: "README.md",
        name: "README.md",
        kind: "file",
        depth: 0,
        expandable: false,
        expanded: false,
        loading: false,
      },
    ]);
    expect(withRoot().loading.has("")).toBe(false);
  });

  it("フォルダを展開すると読み込みが要ると返し_読み込むと子の行が続く", () => {
    const expanded = expandDir(withRoot(), "src");

    expect(expanded.needsLoad).toBe(true);
    const loadingRow = flattenRows(expanded.model).find((row) => row.path === "src");
    expect(loadingRow).toMatchObject({ expanded: true, loading: true });

    const loaded = applyDirListing(expanded.model, "src", [folder("lib"), file("main.ts")]);
    expect(outline(flattenRows(loaded))).toEqual(["src", "  lib", "  main.ts", "docs", "README.md"]);
    expect(flattenRows(loaded)[2]).toMatchObject({ path: "src/main.ts", depth: 1, loading: false });
  });

  it("読み込み済みのフォルダを開くときは読み込みが要らない", () => {
    const loaded = collapseDir(open(withRoot(), "src", [file("a.ts")]), "src");

    expect(expandDir(loaded, "src").needsLoad).toBe(false);
  });

  it("読み込み中のフォルダをもう一度開いても読み込みを重ねて求めない", () => {
    const once = expandDir(withRoot(), "src").model;
    const collapsed = collapseDir(once, "src");

    expect(expandDir(collapsed, "src").needsLoad).toBe(false);
  });

  it("フォルダへのリンクとファイルと知らない項目は展開しない", () => {
    const model = applyDirListing(createTreeModel(), "", [
      { name: "linked", kind: "directoryLink" },
      file("a.ts"),
    ]);

    expect(flattenRows(model)[0]).toMatchObject({ kind: "directoryLink", expandable: false });
    expect(expandDir(model, "linked")).toEqual({ model, needsLoad: false });
    expect(expandDir(model, "a.ts")).toEqual({ model, needsLoad: false });
    expect(expandDir(model, "missing")).toEqual({ model, needsLoad: false });
    expect(expandDir(model, "")).toEqual({ model, needsLoad: false });
  });

  it("開閉の切り替えで開いて閉じる", () => {
    const opened = toggleDir(withRoot(), "src");
    expect(opened.needsLoad).toBe(true);
    expect(opened.model.expanded.has("src")).toBe(true);

    const closed = toggleDir(opened.model, "src");
    expect(closed.needsLoad).toBe(false);
    expect(closed.model.expanded.has("src")).toBe(false);
  });
});

describe("折りたたみ", () => {
  it("折りたたむと子の行が消え_もう一度開くと前の展開も戻る", () => {
    let model = open(withRoot(), "src", [folder("lib"), file("main.ts")]);
    model = open(model, "src/lib", [file("util.ts")]);
    expect(outline(flattenRows(model))).toEqual(["src", "  lib", "    util.ts", "  main.ts", "docs", "README.md"]);

    model = collapseDir(model, "src");
    expect(outline(flattenRows(model))).toEqual(["src", "docs", "README.md"]);

    const reopened = expandDir(model, "src");
    expect(reopened.needsLoad).toBe(false);
    expect(outline(flattenRows(reopened.model))).toEqual([
      "src",
      "  lib",
      "    util.ts",
      "  main.ts",
      "docs",
      "README.md",
    ]);
  });

  it("折りたたんで隠れた行を選んでいたら折りたたんだフォルダを選ぶ", () => {
    let model = open(withRoot(), "src", [folder("lib")]);
    model = open(model, "src/lib", [file("util.ts")]);
    model = selectPath(model, "src/lib/util.ts");

    expect(collapseDir(model, "src").selected).toBe("src");
    expect(collapseDir(selectPath(model, "docs"), "src").selected).toBe("docs");
  });

  it("すべて折りたたむとルートの直下だけになる", () => {
    let model = open(withRoot(), "src", [folder("lib")]);
    model = open(model, "src/lib", [file("util.ts")]);
    model = open(model, "docs", [file("a.md")]);
    model = selectPath(model, "src/lib/util.ts");

    const collapsed = collapseAll(model);

    expect(outline(flattenRows(collapsed))).toEqual(["src", "docs", "README.md"]);
    expect(collapsed.expanded.size).toBe(0);
    expect(collapsed.selected).toBe("src");
    // 読み込み済みの中身は捨てない(開き直すとすぐ出る)
    expect(expandDir(collapsed, "src").needsLoad).toBe(false);
    expect(collapseAll(selectPath(model, "README.md")).selected).toBe("README.md");
  });
});

describe("中身の差分の適用(FR-014)", () => {
  it("中身の差分を当てると追加と削除だけが変わり選択は残る", () => {
    let model = open(withRoot(), "src", [file("a.ts"), file("b.ts")]);
    model = selectPath(model, "src/b.ts");

    model = applyDirListing(model, "src", [file("b.ts"), file("c.ts")]);

    expect(outline(flattenRows(model))).toEqual(["src", "  b.ts", "  c.ts", "docs", "README.md"]);
    expect(model.selected).toBe("src/b.ts");
  });

  it("選択していた項目が消えたら選択を外す", () => {
    let model = open(withRoot(), "src", [file("a.ts")]);
    model = selectPath(model, "src/a.ts");

    expect(applyDirListing(model, "src", []).selected).toBeNull();
  });

  it("消えたフォルダの子孫の読み込み済みの中身を捨てる", () => {
    let model = open(withRoot(), "src", [folder("lib")]);
    model = open(model, "src/lib", [folder("deep")]);
    model = open(model, "src/lib/deep", [file("x.ts")]);
    model = selectPath(model, "src/lib/deep/x.ts");

    model = applyDirListing(model, "", [folder("docs"), file("README.md")]);

    expect(isLoaded(model, "src")).toBe(false);
    expect(isLoaded(model, "src/lib")).toBe(false);
    expect(isLoaded(model, "src/lib/deep")).toBe(false);
    expect(model.expanded.has("src")).toBe(false);
    expect(model.expanded.has("src/lib/deep")).toBe(false);
    expect(model.selected).toBeNull();
    expect(loadedDirs(model)).toEqual([""]);
  });

  it("フォルダがファイルに替わったら子孫の中身を捨てる", () => {
    let model = open(withRoot(), "src", [file("a.ts")]);

    model = applyDirListing(model, "", [file("src"), folder("docs")]);

    expect(isLoaded(model, "src")).toBe(false);
    expect(model.expanded.has("src")).toBe(false);
    expect(outline(flattenRows(model))).toEqual(["src", "docs"]);
  });

  it("同じ名前の兄弟が残るフォルダは前方が同じでも捨てない", () => {
    let model = applyDirListing(createTreeModel(), "", [folder("src"), folder("src2")]);
    model = open(model, "src2", [file("a.ts")]);

    model = applyDirListing(model, "", [folder("src2")]);

    expect(isLoaded(model, "src2")).toBe(true);
    expect(outline(flattenRows(model))).toEqual(["src2", "  a.ts"]);
  });

  it("親が読み込まれていないか親から消えたフォルダの中身は捨てる(遅れて届いた結果)", () => {
    const model = withRoot();

    const stale = applyDirListing(model, "gone", [file("a.ts")]);
    expect(isLoaded(stale, "gone")).toBe(false);

    const orphan = applyDirListing(model, "src/lib", [file("a.ts")]);
    expect(isLoaded(orphan, "src/lib")).toBe(false);
  });

  it("見つからなくなったフォルダを忘れると子孫の中身と展開と選択を外す", () => {
    let model = open(withRoot(), "src", [folder("lib")]);
    model = open(model, "src/lib", [file("a.ts")]);
    model = selectPath(model, "src/lib/a.ts");

    const forgotten = forgetDir(model, "src/lib");

    expect(isLoaded(forgotten, "src/lib")).toBe(false);
    expect(forgotten.expanded.has("src/lib")).toBe(false);
    expect(forgotten.selected).toBeNull();
    expect(isLoaded(forgotten, "src")).toBe(true);
  });

  it("ルートを忘れると読み込みの待ちに戻る", () => {
    const forgotten = forgetDir(open(withRoot(), "src", [file("a.ts")]), "");

    expect(loadedDirs(forgotten)).toEqual([]);
    expect(flattenRows(forgotten)).toEqual([]);
    expect(forgotten.expanded.size).toBe(0);
  });

  it("ヘッダを選ぶとルートが選ばれ_選択は外せる", () => {
    const model = selectPath(withRoot(), "");

    expect(model.selected).toBe("");
    expect(selectPath(model, null).selected).toBeNull();
  });
});

describe("変更の受け取り(設計書 §7.1)", () => {
  function loadedTree(): TreeModel {
    let model = open(withRoot(), "src", [folder("lib")]);
    model = open(model, "src/lib", [file("a.ts")]);
    return collapseDir(model, "src");
  }

  it("読み込み済みのフォルダの変更は再読込に積む", () => {
    expect(dirsToReload(loadedTree(), { dirs: ["src/lib", "src", "src/lib"], fullReload: false })).toEqual([
      "src/lib",
      "src",
    ]);
  });

  it("読み込んでいないフォルダの変更は無視する", () => {
    expect(dirsToReload(loadedTree(), { dirs: ["docs", "src/other"], fullReload: false })).toEqual([]);
  });

  it("全部読み直しの印で読み込み済みのフォルダをすべて積む", () => {
    expect(dirsToReload(loadedTree(), { dirs: [], fullReload: true })).toEqual(["", "src", "src/lib"]);
  });

  it("読み込み済みのフォルダの一覧はプロジェクトに戻ったときの読み直しに使える(【仮定】4)", () => {
    expect(loadedDirs(loadedTree())).toEqual(["", "src", "src/lib"]);
  });
});
