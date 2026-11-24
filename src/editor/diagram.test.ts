import { describe, expect, it } from "vitest";

import { isMermaidInfo } from "./diagram";

describe("isMermaidInfo(コードの塊の言語名が Mermaid か。ADR-017)", () => {
  it.each([
    ["mermaid", true],
    ["Mermaid", true],
    ["  MERMAID  ", true],
    ["mermaid title=図", true],
    ["mermaidx", false],
    ["js", false],
    ["", false],
    ["x mermaid", false],
  ])("%j は %s", (info, expected) => {
    expect(isMermaidInfo(info)).toBe(expected);
  });
});
