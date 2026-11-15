import { describe, expect, it } from "vitest";

import type { ClipboardFocus, ClipboardKind } from "../../logic/keys/clipboard";
import type { ClipboardText } from "../../terminal/TerminalManager";
import {
  type ClipboardEventTarget,
  installClipboardRouting,
  type TreeClipboardAction,
  type TreeClipboardTarget,
} from "./clipboardRouting";

type Listener = (event: ClipboardEvent) => void;

/** 文書の偽物。付けた受け取り手を種類ごとに持つ(node の環境には DOM が無い)。 */
function fakeDocument() {
  const listeners = new Map<string, { listener: Listener; capture: boolean }[]>();
  const target = {
    addEventListener(type: string, listener: Listener, options: AddEventListenerOptions) {
      listeners.set(type, [...(listeners.get(type) ?? []), { listener, capture: options.capture === true }]);
    },
    removeEventListener(type: string, listener: Listener) {
      listeners.set(
        type,
        (listeners.get(type) ?? []).filter((l) => l.listener !== listener),
      );
    },
  } as unknown as ClipboardEventTarget;
  const dispatch = (type: ClipboardKind, clipboardData: ClipboardText | null) => {
    const event = {
      type,
      clipboardData,
      defaultPrevented: false,
      propagationStopped: false,
      preventDefault() {
        event.defaultPrevented = true;
      },
      stopPropagation() {
        event.propagationStopped = true;
      },
    };
    for (const { listener } of listeners.get(type) ?? []) listener(event as unknown as ClipboardEvent);
    return event;
  };
  return { target, listeners, dispatch };
}

const data: ClipboardText = { getData: () => "", setData: () => {} };

describe("installClipboardRouting(コピー・ペーストのイベントの入口)", () => {
  it("copy と paste を捕捉の段階で受ける", () => {
    const doc = fakeDocument();
    installClipboardRouting(doc.target, { handleClipboard: () => false });

    expect(doc.listeners.get("copy")?.map((l) => l.capture)).toEqual([true]);
    expect(doc.listeners.get("paste")?.map((l) => l.capture)).toEqual([true]);
  });

  it("イベントの種類と clipboardData をそのまま渡し、処理されたら既定の動作と伝わりを止める", () => {
    const doc = fakeDocument();
    const seen: [ClipboardKind, ClipboardText][] = [];
    installClipboardRouting(doc.target, {
      handleClipboard: (kind, d) => {
        seen.push([kind, d]);
        return true;
      },
    });

    const copy = doc.dispatch("copy", data);
    const paste = doc.dispatch("paste", data);

    expect(seen).toEqual([
      ["copy", data],
      ["paste", data],
    ]);
    expect([copy.defaultPrevented, copy.propagationStopped]).toEqual([true, true]);
    expect([paste.defaultPrevented, paste.propagationStopped]).toEqual([true, true]);
  });

  it("処理されなければ何も止めない(既定の動作と xterm.js に任せる)", () => {
    const doc = fakeDocument();
    installClipboardRouting(doc.target, { handleClipboard: () => false });

    const copy = doc.dispatch("copy", data);

    expect([copy.defaultPrevented, copy.propagationStopped]).toEqual([false, false]);
  });

  it("clipboardData の無いイベントは振り分けない", () => {
    const doc = fakeDocument();
    let calls = 0;
    installClipboardRouting(doc.target, {
      handleClipboard: () => {
        calls += 1;
        return true;
      },
    });

    const paste = doc.dispatch("paste", null);

    expect(calls).toBe(0);
    expect(paste.defaultPrevented).toBe(false);
  });

  it("外す関数で受け取りを外す", () => {
    const doc = fakeDocument();
    const uninstall = installClipboardRouting(doc.target, { handleClipboard: () => true });

    uninstall();

    expect(doc.listeners.get("copy")).toEqual([]);
    expect(doc.listeners.get("cut")).toEqual([]);
    expect(doc.listeners.get("paste")).toEqual([]);
  });

  it("切り取りも捕捉の段階で受ける(ターミナルでは何もしない)", () => {
    const doc = fakeDocument();
    const seen: ClipboardKind[] = [];
    installClipboardRouting(doc.target, {
      handleClipboard: (kind) => {
        seen.push(kind);
        return false;
      },
    });

    const cut = doc.dispatch("cut", data);

    expect(doc.listeners.get("cut")?.map((l) => l.capture)).toEqual([true]);
    expect(seen).toEqual(["cut"]);
    expect(cut.defaultPrevented).toBe(false);
  });
});

/** ツリーの受け口の偽物。フォーカス位置を差し替えられ、行った操作を記録する。 */
function fakeTree(focus: ClipboardFocus | null) {
  const runs: TreeClipboardAction[] = [];
  const tree: TreeClipboardTarget & { current: ClipboardFocus | null } = {
    current: focus,
    focus: () => tree.current,
    run: (action) => void runs.push(action),
  };
  return { tree, runs };
}

describe("installClipboardRouting(ツリーへの振り分け。T43c)", () => {
  it("ツリーにフォーカスがあれば_コピー_切り取り_ペーストはツリーの操作になり_ターミナルには渡さない", () => {
    const doc = fakeDocument();
    let terminalCalls = 0;
    const { tree, runs } = fakeTree({ kind: "tree", selection: "entry" });
    installClipboardRouting(
      doc.target,
      {
        handleClipboard: () => {
          terminalCalls += 1;
          return true;
        },
      },
      tree,
    );

    const events = (["copy", "cut", "paste"] as const).map((kind) => doc.dispatch(kind, data));

    expect(runs).toEqual(["tree-copy", "tree-cut", "tree-paste"]);
    expect(terminalCalls).toBe(0);
    expect(events.map((e) => [e.defaultPrevented, e.propagationStopped])).toEqual([
      [true, true],
      [true, true],
      [true, true],
    ]);
  });

  it("ツリーで操作にならないもの(ルートの切り取り・選択の無いコピー)は止めず_ターミナルにも渡さない", () => {
    const doc = fakeDocument();
    let terminalCalls = 0;
    const { tree, runs } = fakeTree({ kind: "tree", selection: "root" });
    installClipboardRouting(
      doc.target,
      {
        handleClipboard: () => {
          terminalCalls += 1;
          return true;
        },
      },
      tree,
    );

    const cut = doc.dispatch("cut", data);
    tree.current = { kind: "tree", selection: "none" };
    const copy = doc.dispatch("copy", data);
    const paste = doc.dispatch("paste", data);

    expect(runs).toEqual(["tree-paste"]);
    expect([cut.defaultPrevented, copy.defaultPrevented, paste.defaultPrevented]).toEqual([false, false, true]);
    expect(terminalCalls).toBe(0);
  });

  it("名前の入力中は何もしない(入力欄の文字の操作に任せる)", () => {
    const doc = fakeDocument();
    let terminalCalls = 0;
    const { tree, runs } = fakeTree({ kind: "name-input" });
    installClipboardRouting(
      doc.target,
      {
        handleClipboard: () => {
          terminalCalls += 1;
          return true;
        },
      },
      tree,
    );

    const events = (["copy", "cut", "paste"] as const).map((kind) => doc.dispatch(kind, data));

    expect(runs).toEqual([]);
    expect(terminalCalls).toBe(0);
    expect(events.map((e) => e.defaultPrevented)).toEqual([false, false, false]);
  });

  it("ツリーにも入力欄にもフォーカスが無ければ今までどおりターミナルへ振り分ける", () => {
    const doc = fakeDocument();
    const seen: ClipboardKind[] = [];
    const { tree, runs } = fakeTree(null);
    installClipboardRouting(
      doc.target,
      {
        handleClipboard: (kind) => {
          seen.push(kind);
          return true;
        },
      },
      tree,
    );

    const paste = doc.dispatch("paste", data);

    expect(seen).toEqual(["paste"]);
    expect(runs).toEqual([]);
    expect(paste.defaultPrevented).toBe(true);
  });
});
