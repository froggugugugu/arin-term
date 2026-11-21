import { describe, expect, it } from "vitest";

import { type ContextMenuTarget, installContextMenuGuard, keepsNativeContextMenu } from "./contextMenuGuard";

/** `closest` が、渡した選択子のうち当たるものだけを返す偽の要素。 */
function fakeTarget(matches: { editable?: boolean; terminal?: boolean }): ContextMenuTarget {
  return {
    closest: (selector: string) => {
      if (selector === ".xterm") return matches.terminal === true ? {} : null;
      return matches.editable === true ? {} : null;
    },
  };
}

function fakeDocument() {
  let listener: ((event: Event) => void) | null = null;
  return {
    doc: {
      addEventListener: (type: string, fn: (event: Event) => void) => {
        if (type === "contextmenu") listener = fn;
      },
      removeEventListener: (type: string) => {
        if (type === "contextmenu") listener = null;
      },
    } as unknown as Pick<Document, "addEventListener" | "removeEventListener">,
    fire(target: unknown): boolean {
      let prevented = false;
      listener?.({ target, preventDefault: () => (prevented = true) } as unknown as Event);
      return prevented;
    },
    hasListener: () => listener !== null,
  };
}

describe("keepsNativeContextMenu(標準の右クリックのメニューを残すか)", () => {
  it("文字の入力欄では残す", () => {
    expect(keepsNativeContextMenu(fakeTarget({ editable: true }))).toBe(true);
  });

  it("ターミナルの中の入力用の textarea では止める", () => {
    expect(keepsNativeContextMenu(fakeTarget({ editable: true, terminal: true }))).toBe(false);
  });

  it("入力欄でない要素と、要素でない対象では止める", () => {
    expect(keepsNativeContextMenu(fakeTarget({}))).toBe(false);
    expect(keepsNativeContextMenu(null)).toBe(false);
  });
});

describe("installContextMenuGuard", () => {
  it("入力欄でなければ既定の動作を止め、入力欄なら止めない", () => {
    const d = fakeDocument();
    installContextMenuGuard(d.doc);
    expect(d.fire(fakeTarget({}))).toBe(true);
    expect(d.fire(fakeTarget({ editable: true }))).toBe(false);
    // 文書そのもの(closest を持たない)も止める。
    expect(d.fire({})).toBe(true);
  });

  it("外す関数で受け取りを外す", () => {
    const d = fakeDocument();
    const remove = installContextMenuGuard(d.doc);
    remove();
    expect(d.hasListener()).toBe(false);
  });
});
