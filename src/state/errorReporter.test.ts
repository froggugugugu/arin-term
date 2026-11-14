import { describe, expect, it } from "vitest";

import { type FailureEventTarget, installErrorReporter } from "./errorReporter";

/** 受け取り手を記録する偽のイベントの的(node の環境には window も document も無い)。 */
function fakeTarget() {
  const listeners = new Map<string, { listener: (event: unknown) => void; capture: boolean }>();
  const target: FailureEventTarget = {
    addEventListener: (type, listener, capture) => {
      listeners.set(type, { listener, capture: capture === true });
    },
    removeEventListener: (type, listener) => {
      if (listeners.get(type)?.listener === listener) listeners.delete(type);
    },
  };
  return {
    target,
    listeners,
    fire: (type: string, event: unknown) => listeners.get(type)?.listener(event),
  };
}

function setup(options: { max?: number; failReport?: boolean } = {}) {
  const win = fakeTarget();
  const doc = fakeTarget();
  const sent: string[] = [];
  const logged: unknown[][] = [];
  const reporter = installErrorReporter({
    window: win.target,
    document: doc.target,
    report: async (message) => {
      sent.push(message);
      if (options.failReport) throw "Rust に届かない";
    },
    log: (...args) => logged.push(args),
    ...(options.max === undefined ? {} : { max: options.max }),
  });
  return { win, doc, sent, logged, reporter };
}

const flush = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

describe("installErrorReporter(画面の失敗を Rust へ知らせる)", () => {
  it("CSP 違反は document で、例外は window の捕捉の段階で、Promise の失敗は window で拾う", () => {
    const { win, doc } = setup();

    expect(doc.listeners.has("securitypolicyviolation")).toBe(true);
    expect(win.listeners.get("error")?.capture).toBe(true);
    expect(win.listeners.has("unhandledrejection")).toBe(true);
  });

  it("拾った失敗を 1 行にして Rust へ送り、ログにも出す", async () => {
    const { win, doc, sent, logged } = setup();

    doc.fire("securitypolicyviolation", { effectiveDirective: "style-src-elem", blockedURI: "inline" });
    win.fire("error", { message: "boom" });
    win.fire("unhandledrejection", { reason: "失敗" });
    await flush();

    expect(sent).toEqual([
      "CSP 違反: style-src-elem が inline を止めた",
      "未処理の例外: boom",
      "未処理の Promise の失敗: 失敗",
    ]);
    expect(logged.map((args) => args[0])).toEqual(sent);
  });

  it("呼び出し側からの報告(ターミナルの接続の失敗など)も同じ道で送る", async () => {
    const { reporter, sent } = setup();

    reporter.report("ターミナル t1 に接続できなかった: x");
    await flush();

    expect(sent).toEqual(["ターミナル t1 に接続できなかった: x"]);
  });

  it("上限を超えた報告は、上限に達したことを 1 回だけ知らせて、あとは送らない", async () => {
    const { reporter, sent, logged } = setup({ max: 1 });

    reporter.report("a");
    reporter.report("b");
    reporter.report("c");
    await flush();

    expect(sent).toEqual(["a", "b(画面の失敗の報告が上限の 1 件に達したので、以後は送らない)"]);
    expect(logged).toHaveLength(3);
  });

  it("送るのに失敗しても投げ直さず(繰り返しの報告にしない)、ログに残す", async () => {
    const { reporter, logged } = setup({ failReport: true });

    reporter.report("a");
    await flush();

    expect(logged.map((args) => args[0])).toEqual(["a", "画面の失敗を Rust へ送れなかった: Rust に届かない"]);
  });

  it("外すと受け取り手を外し、以後は拾わない", async () => {
    const { win, doc, reporter, sent } = setup();

    reporter.uninstall();
    win.fire("error", { message: "boom" });
    await flush();

    expect(win.listeners.size).toBe(0);
    expect(doc.listeners.size).toBe(0);
    expect(sent).toEqual([]);
  });
});
