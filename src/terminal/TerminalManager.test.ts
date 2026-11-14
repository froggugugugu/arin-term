import { describe, expect, it } from "vitest";

import { createFakeTerminalEngine } from "../test/fakeTerminalEngine";
import { TerminalManager } from "./TerminalManager";

describe("TerminalManager", () => {
  it("作った直後はサーフェスを持たない", () => {
    const manager = new TerminalManager(createFakeTerminalEngine());

    expect(manager.get("t1")).toBeUndefined();
    expect(manager.size).toBe(0);
  });

  it("ID を渡すとエンジンでサーフェスを作り、その ID で引ける", () => {
    const engine = createFakeTerminalEngine();
    const manager = new TerminalManager(engine);

    const surface = manager.getOrCreate("t1");

    expect(engine.created.map((s) => s.id)).toEqual(["t1"]);
    expect(surface).toBe(engine.created[0]);
    expect(manager.get("t1")).toBe(surface);
    expect(manager.size).toBe(1);
  });

  it("同じ ID では二重に作らず、最初のサーフェスを返す", () => {
    const engine = createFakeTerminalEngine();
    const manager = new TerminalManager(engine);

    const first = manager.getOrCreate("t1");
    const second = manager.getOrCreate("t1");

    expect(second).toBe(first);
    expect(engine.created).toHaveLength(1);
    expect(manager.size).toBe(1);
  });

  it("ID ごとに別のサーフェスになる", () => {
    const engine = createFakeTerminalEngine();
    const manager = new TerminalManager(engine);

    const a = manager.getOrCreate("a");
    const b = manager.getOrCreate("b");

    expect(a).not.toBe(b);
    expect(manager.get("a")).toBe(a);
    expect(manager.get("b")).toBe(b);
    expect(manager.size).toBe(2);
  });

  it("破棄するとサーフェスの dispose を 1 回呼び、対応を外す", () => {
    const engine = createFakeTerminalEngine();
    const manager = new TerminalManager(engine);
    const surface = manager.getOrCreate("t1");

    expect(manager.dispose("t1")).toBe(true);

    expect(engine.created[0]?.disposeCount).toBe(1);
    expect(manager.get("t1")).toBeUndefined();
    expect(manager.size).toBe(0);
    expect(surface).toBe(engine.created[0]);
  });

  it("破棄しても他の ID のサーフェスは残る", () => {
    const engine = createFakeTerminalEngine();
    const manager = new TerminalManager(engine);
    manager.getOrCreate("a");
    const b = manager.getOrCreate("b");

    manager.dispose("a");

    expect(manager.get("b")).toBe(b);
    expect(engine.created[1]?.disposeCount).toBe(0);
  });

  it("存在しない ID の破棄は何もせず false を返す", () => {
    const engine = createFakeTerminalEngine();
    const manager = new TerminalManager(engine);
    manager.getOrCreate("a");

    expect(manager.dispose("missing")).toBe(false);

    expect(engine.created[0]?.disposeCount).toBe(0);
    expect(manager.size).toBe(1);
  });

  it("同じ ID を 2 回破棄しても dispose は 1 回だけで、2 回目は false", () => {
    const engine = createFakeTerminalEngine();
    const manager = new TerminalManager(engine);
    manager.getOrCreate("t1");

    expect(manager.dispose("t1")).toBe(true);
    expect(manager.dispose("t1")).toBe(false);

    expect(engine.created[0]?.disposeCount).toBe(1);
  });

  it("破棄した ID でもう一度求めると新しいサーフェスを作る", () => {
    const engine = createFakeTerminalEngine();
    const manager = new TerminalManager(engine);
    const first = manager.getOrCreate("t1");
    manager.dispose("t1");

    const second = manager.getOrCreate("t1");

    expect(second).not.toBe(first);
    expect(engine.created).toHaveLength(2);
    expect(manager.get("t1")).toBe(second);
  });

  it("サーフェスの dispose が失敗しても対応は外れ、例外はそのまま伝わる", () => {
    const engine = createFakeTerminalEngine();
    const manager = new TerminalManager(engine);
    engine.failDisposeOnNext = true;
    manager.getOrCreate("t1");

    expect(() => manager.dispose("t1")).toThrow("偽のサーフェス t1 の破棄に失敗した");

    expect(manager.get("t1")).toBeUndefined();
    expect(manager.size).toBe(0);
  });

  it("空文字の ID も 1 つの ID として扱う", () => {
    const engine = createFakeTerminalEngine();
    const manager = new TerminalManager(engine);

    const surface = manager.getOrCreate("");

    expect(manager.get("")).toBe(surface);
    expect(manager.dispose("")).toBe(true);
    expect(manager.get("")).toBeUndefined();
  });

  it("マネージャーごとに対応は別になる", () => {
    const engine = createFakeTerminalEngine();
    const a = new TerminalManager(engine);
    const b = new TerminalManager(engine);

    a.getOrCreate("t1");

    expect(b.get("t1")).toBeUndefined();
  });
});
