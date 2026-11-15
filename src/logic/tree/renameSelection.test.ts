import { describe, expect, it } from "vitest";

import { renameSelectionRange } from "./renameSelection";

describe("renameSelectionRange(名称変更の選択範囲。FR-026、【仮定】10)", () => {
  it("名称変更の選択範囲は最後のドットの前まで", () => {
    expect(renameSelectionRange("a.ts", "file")).toEqual({ start: 0, end: 1 });
    expect(renameSelectionRange("a.tar.gz", "file")).toEqual({ start: 0, end: 5 });
    expect(renameSelectionRange("App.test.tsx", "fileLink")).toEqual({ start: 0, end: 8 });
  });

  it("ドットで始まる名前と拡張子の無い名前とフォルダは全体を選ぶ", () => {
    expect(renameSelectionRange(".env", "file")).toEqual({ start: 0, end: 4 });
    expect(renameSelectionRange("Makefile", "file")).toEqual({ start: 0, end: 8 });
    expect(renameSelectionRange("v1.2", "directory")).toEqual({ start: 0, end: 4 });
    expect(renameSelectionRange("lib.d", "directoryLink")).toEqual({ start: 0, end: 5 });
  });

  it("ドットで始まり他にもドットがある名前は最後のドットの前まで", () => {
    expect(renameSelectionRange(".env.local", "file")).toEqual({ start: 0, end: 4 });
  });

  it("位置は入力欄の選択と同じ UTF-16 の単位で数える", () => {
    // 絵文字は UTF-16 で 2 単位
    expect(renameSelectionRange("😀メモ.md", "file")).toEqual({ start: 0, end: 4 });
  });
});
