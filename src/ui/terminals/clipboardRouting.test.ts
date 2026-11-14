import { describe, expect, it } from "vitest";

import type { ClipboardKind } from "../../logic/keys/clipboard";
import type { ClipboardText } from "../../terminal/TerminalManager";
import { type ClipboardEventTarget, installClipboardRouting } from "./clipboardRouting";

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
    expect(doc.listeners.get("paste")).toEqual([]);
  });
});
