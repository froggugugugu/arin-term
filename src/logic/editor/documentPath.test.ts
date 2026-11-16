import { describe, expect, it } from "vitest";

import { isMarkdownPath } from "./documentPath";

describe("isMarkdownPath(エディタで開くファイル。ADR-015 の 5)", () => {
  it("拡張子が_md_と_markdown_なら大文字小文字を問わず真", () => {
    for (const path of ["a.md", "docs/README.MD", "x/y.Markdown", "note.markdown"]) {
      expect(isMarkdownPath(path)).toBe(true);
    }
  });

  it("ほかの拡張子_拡張子だけの名前_フォルダの名前の点は偽", () => {
    for (const path of ["a.ts", "a.mdx", "md", ".md", "docs.md/a.txt", "a.md.txt", ""]) {
      expect(isMarkdownPath(path)).toBe(false);
    }
  });
});
