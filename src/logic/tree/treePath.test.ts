import { describe, expect, it } from "vitest";

import { isSameOrDescendant, joinPath, nameOf, parentPath } from "./treePath";

describe("ルート基準の相対パスの組み立て", () => {
  it("ルートの子は名前だけ_それより下はスラッシュでつなぐ", () => {
    expect(joinPath("", "src")).toBe("src");
    expect(joinPath("src", "a.ts")).toBe("src/a.ts");
  });

  it("親と名前に分けられ_ルートの親は無い", () => {
    expect(parentPath("src/lib/a.ts")).toBe("src/lib");
    expect(parentPath("src")).toBe("");
    expect(parentPath("")).toBeNull();
    expect(nameOf("src/lib/a.ts")).toBe("a.ts");
    expect(nameOf("src")).toBe("src");
    expect(nameOf("")).toBe("");
  });

  it("自分自身と子孫は含まれ_名前の前方が同じだけの別の項目は含まれない", () => {
    expect(isSameOrDescendant("src", "src")).toBe(true);
    expect(isSameOrDescendant("src/a.ts", "src")).toBe(true);
    expect(isSameOrDescendant("src2/a.ts", "src")).toBe(false);
    expect(isSameOrDescendant("lib", "src")).toBe(false);
    expect(isSameOrDescendant("anything/deep", "")).toBe(true);
  });
});
