import { describe, expect, it } from "vitest";

import { createIpc } from "../ipc/client";
import type { ConnectOutcome } from "../ipc/generated/ConnectOutcome";
import { INPUT_QUEUE_LIMIT } from "../logic/serialSender";
import { type GridView, SHOW_ALL, showOnly, visibilityChanges } from "../logic/verifyGrid";
import { createFakeIpc, type FakeHandlers } from "../test/fakeIpc";
import { createFakeTerminalEngine } from "../test/fakeTerminalEngine";
import { TerminalManager, type TerminalSlot } from "./TerminalManager";

/** 偽のエンジンと偽の通信で組み立てる。報告された失敗は errors に溜める。 */
function setup(handlers: Partial<FakeHandlers> = {}) {
  const engine = createFakeTerminalEngine();
  const fake = createFakeIpc(handlers);
  const errors: string[] = [];
  const manager = new TerminalManager(engine, createIpc(fake.transport), {
    reportError: (message) => errors.push(message),
  });
  return { engine, fake, errors, manager };
}

/** 置き場所の偽物。入れられた要素を記録する(node の環境には DOM が無い)。 */
function fakeSlot() {
  const children: unknown[] = [];
  const slot: TerminalSlot = {
    appendChild: (el) => {
      children.push(el);
      return el;
    },
    removeChild: (el) => {
      children.splice(children.indexOf(el), 1);
      return el;
    },
  };
  return { slot, children };
}

/** 溜まっている Promise の続きを流し切る。 */
const flush = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

const commands = (fake: ReturnType<typeof createFakeIpc>) => fake.calls.map((c) => c.command);

describe("TerminalManager(ID とサーフェスの対応)", () => {
  it("作った直後はサーフェスを持たない", () => {
    const { manager } = setup();

    expect(manager.get("t1")).toBeUndefined();
    expect(manager.size).toBe(0);
  });

  it("ID を渡すとエンジンでサーフェスを作り、その ID で引ける", () => {
    const { engine, manager } = setup();

    const surface = manager.getOrCreate("t1");

    expect(engine.created.map((s) => s.id)).toEqual(["t1"]);
    expect(surface).toBe(engine.created[0]);
    expect(manager.get("t1")).toBe(surface);
    expect(manager.size).toBe(1);
  });

  it("同じ ID では二重に作らず、最初のサーフェスを返す", () => {
    const { engine, manager } = setup();

    const first = manager.getOrCreate("t1");
    const second = manager.getOrCreate("t1");

    expect(second).toBe(first);
    expect(engine.created).toHaveLength(1);
  });

  it("ID ごとに別のサーフェスになる", () => {
    const { manager } = setup();

    const a = manager.getOrCreate("a");
    const b = manager.getOrCreate("b");

    expect(a).not.toBe(b);
    expect(manager.size).toBe(2);
  });

  it("破棄するとサーフェスの dispose を 1 回呼び、対応を外す。2 回目は false", () => {
    const { engine, manager } = setup();
    manager.getOrCreate("t1");

    expect(manager.dispose("t1")).toBe(true);
    expect(manager.dispose("t1")).toBe(false);

    expect(engine.created[0]?.disposeCount).toBe(1);
    expect(manager.get("t1")).toBeUndefined();
    expect(manager.size).toBe(0);
  });

  it("破棄しても他の ID のサーフェスは残る", () => {
    const { engine, manager } = setup();
    manager.getOrCreate("a");
    const b = manager.getOrCreate("b");

    manager.dispose("a");

    expect(manager.get("b")).toBe(b);
    expect(engine.created[1]?.disposeCount).toBe(0);
  });

  it("存在しない ID の破棄は何もせず false を返す", () => {
    const { engine, manager } = setup();
    manager.getOrCreate("a");

    expect(manager.dispose("missing")).toBe(false);
    expect(engine.created[0]?.disposeCount).toBe(0);
  });

  it("破棄した ID でもう一度求めると新しいサーフェスを作る", () => {
    const { engine, manager } = setup();
    const first = manager.getOrCreate("t1");
    manager.dispose("t1");

    const second = manager.getOrCreate("t1");

    expect(second).not.toBe(first);
    expect(engine.created).toHaveLength(2);
  });

  it("サーフェスの dispose が失敗しても対応は外れ、例外はそのまま伝わる", () => {
    const { engine, manager } = setup();
    engine.failDisposeOnNext = true;
    manager.getOrCreate("t1");

    expect(() => manager.dispose("t1")).toThrow("偽のサーフェス t1 の破棄に失敗した");
    expect(manager.get("t1")).toBeUndefined();
  });

  it("空文字の ID も 1 つの ID として扱う", () => {
    const { manager } = setup();

    const surface = manager.getOrCreate("");

    expect(manager.get("")).toBe(surface);
    expect(manager.dispose("")).toBe(true);
  });
});

describe("TerminalManager(置き場所)", () => {
  it("置き場所に要素を入れて寸法を合わせ、そのときの行・桁で接続する", async () => {
    const { engine, fake, manager } = setup();
    const { slot, children } = fakeSlot();
    manager.getOrCreate("t1");
    const surface = engine.created[0];
    if (surface === undefined) throw new Error("サーフェスが無い");
    surface.fitTo = { rows: 40, cols: 120 };

    manager.attach("t1", slot);
    await flush();

    expect(children).toEqual([surface.element]);
    expect(surface.fitCount).toBe(1);
    expect(fake.calls).toHaveLength(1);
    const args = fake.calls[0]?.args as { terminalId: string; rows: number; cols: number };
    expect([fake.calls[0]?.command, args.terminalId, args.rows, args.cols]).toEqual([
      "connect_terminal",
      "t1",
      40,
      120,
    ]);
  });

  it("外すと要素を置き場所から出すが、ターミナルは破棄も切断もしない。付け直しても接続し直さない", async () => {
    const { engine, fake, manager } = setup();
    const { slot, children } = fakeSlot();

    const detach = manager.attach("t1", slot);
    await flush();
    detach();
    detach();
    manager.attach("t1", slot);
    await flush();

    expect(children).toEqual([engine.created[0]?.element]);
    expect(engine.created).toHaveLength(1);
    expect(engine.created[0]?.disposeCount).toBe(0);
    expect(fake.channels[0]?.closed).toBe(false);
    expect(commands(fake)).toEqual(["connect_terminal"]);
  });

  it("別の置き場所に付け替えたあと、古い置き場所の外す関数は新しい置き場所に触れない", () => {
    const { manager } = setup();
    const a = fakeSlot();
    const b = fakeSlot();

    const detachA = manager.attach("t1", a.slot);
    manager.attach("t1", b.slot);
    detachA();

    expect(a.children).toEqual([]);
    expect(b.children).toHaveLength(1);
  });
});

describe("TerminalManager(入出力)", () => {
  it("出力がサーフェスに届き、取り込みが済んだ量が Rust へ返る", async () => {
    const { engine, fake, manager } = setup();
    await manager.connect("t1");

    fake.channels[0]?.send(new Uint8Array([104, 105, 10]).buffer);
    await flush();

    expect(engine.created[0]?.written.map((d) => [...d])).toEqual([[104, 105, 10]]);
    expect(fake.calls.slice(1)).toEqual([
      { command: "ack_terminal_output", args: { terminalId: "t1", bytes: 3 } },
    ]);
  });

  it("接続の応答より先に届いた出力も取り込む(Rust は接続の中で出力を流し始める)", async () => {
    let send: ((m: unknown) => void) | undefined;
    const { engine, fake, manager } = setup({
      connect_terminal: (args) => {
        send = (m) => fake.channelOf(args.onOutput)?.send(m);
        send(new Uint8Array([37, 32]).buffer);
        return { sizeApplied: true };
      },
    });

    await manager.connect("t1");
    await flush();

    expect(send).toBeDefined();
    expect(engine.created[0]?.written.map((d) => [...d])).toEqual([[37, 32]]);
    expect(commands(fake)).toEqual(["connect_terminal", "ack_terminal_output"]);
  });

  it("取り込まれなかった出力(サーフェスが 0 を返した)は処理済みを返さない", async () => {
    const { engine, fake, manager } = setup();
    await manager.connect("t1");
    const surface = engine.created[0];
    if (surface === undefined) throw new Error("サーフェスが無い");
    surface.write = () => Promise.resolve(0);

    fake.channels[0]?.send(new Uint8Array([1]).buffer);
    await flush();

    expect(commands(fake)).toEqual(["connect_terminal"]);
  });

  it("入力が write_terminal に届く", async () => {
    const { engine, fake, manager } = setup();
    await manager.connect("t1");

    engine.created[0]?.emitInput("ls\r");
    await flush();

    expect(fake.calls.slice(1)).toEqual([
      { command: "write_terminal", args: { terminalId: "t1", data: "ls\r" } },
    ]);
  });

  it("接続の応答を待つあいだの入力は、接続のあとに順に連結して 1 回で送る", async () => {
    let finish: (() => void) | undefined;
    const { engine, fake, manager } = setup({
      connect_terminal: () =>
        new Promise<ConnectOutcome>((resolve) => {
          finish = () => resolve({ sizeApplied: true });
        }),
    });

    const connecting = manager.connect("t1");
    engine.created[0]?.emitInput("a");
    engine.created[0]?.emitInput("b");
    await flush();
    expect(commands(fake)).toEqual(["connect_terminal"]);
    finish?.();
    await connecting;
    await flush();

    expect(fake.calls.slice(1).map((c) => c.args)).toEqual([{ terminalId: "t1", data: "ab" }]);
  });

  it("寸法の変更が resize_terminal に届く", async () => {
    const { engine, fake, manager } = setup();
    await manager.connect("t1");
    const surface = engine.created[0];
    if (surface === undefined) throw new Error("サーフェスが無い");

    surface.fitTo = { rows: 50, cols: 132 };
    manager.fit("t1");
    manager.fit("t1");
    await flush();

    expect(fake.calls.slice(1)).toEqual([
      { command: "resize_terminal", args: { terminalId: "t1", rows: 50, cols: 132 } },
    ]);
  });

  it("接続を待つあいだに寸法が変わったら、接続のあとに今の寸法を送る", async () => {
    let finish: (() => void) | undefined;
    const { engine, fake, manager } = setup({
      connect_terminal: () =>
        new Promise<ConnectOutcome>((resolve) => {
          finish = () => resolve({ sizeApplied: true });
        }),
    });

    const connecting = manager.connect("t1");
    const surface = engine.created[0];
    if (surface === undefined) throw new Error("サーフェスが無い");
    surface.fitTo = { rows: 10, cols: 20 };
    manager.fit("t1");
    finish?.();
    await connecting;
    await flush();

    expect(fake.calls.slice(1)).toEqual([
      { command: "resize_terminal", args: { terminalId: "t1", rows: 10, cols: 20 } },
    ]);
  });

  it("接続の前の寸法の変更と、知らない ID の fit は何も送らない", async () => {
    const { engine, fake, manager } = setup();
    manager.getOrCreate("t1");
    engine.created[0]?.emitResize(100, 30);
    manager.fit("missing");
    await flush();

    expect(fake.calls).toEqual([]);
  });
});

describe("TerminalManager(境界値)", () => {
  it("二重の接続は 1 回だけ connect_terminal を呼ぶ(StrictMode の 2 回の呼び出し)", async () => {
    const { fake, manager } = setup();

    await Promise.all([manager.connect("t1"), manager.connect("t1")]);
    await manager.connect("t1");

    expect(commands(fake)).toEqual(["connect_terminal"]);
    expect(fake.channels).toHaveLength(1);
  });

  it("接続の失敗は報告し、チャネルを閉じ、入力は送らない。もう一度 connect すれば試し直す", async () => {
    let fail = true;
    const { engine, fake, errors, manager } = setup({
      connect_terminal: () => {
        if (fail) throw "シェルを起動できなかった";
        return { sizeApplied: true };
      },
    });

    await manager.connect("t1");
    engine.created[0]?.emitInput("x");
    await flush();

    expect(errors).toEqual(["ターミナル t1 に接続できなかった: シェルを起動できなかった"]);
    expect(fake.channels[0]?.closed).toBe(true);
    expect(commands(fake)).toEqual(["connect_terminal"]);

    fail = false;
    await manager.connect("t1");
    expect(commands(fake)).toEqual(["connect_terminal", "connect_terminal"]);
  });

  it("書き込みの失敗と処理済みの通知の失敗は報告する(未処理の失敗にしない)", async () => {
    const { engine, fake, errors, manager } = setup({
      write_terminal: () => {
        throw "シェルは終了している";
      },
      ack_terminal_output: () => {
        throw "ID が無い";
      },
      resize_terminal: () => {
        throw "寸法が 0";
      },
    });
    await manager.connect("t1");
    const surface = engine.created[0];
    if (surface === undefined) throw new Error("サーフェスが無い");

    surface.emitInput("a");
    fake.channels[0]?.send(new Uint8Array([1]).buffer);
    surface.emitResize(1, 1);
    await flush();

    // 処理済みの通知は取り込みが済んでから送るので、順は入れ替わりうる。
    expect([...errors].sort()).toEqual(
      [
        "ターミナル t1 への入力を送れなかった: シェルは終了している",
        "ターミナル t1 の処理済みの量を返せなかった: ID が無い",
        "ターミナル t1 の寸法を変えられなかった: 寸法が 0",
      ].sort(),
    );
  });

  it("ターミナルの破棄で購読が外れ、破棄の後に届いた出力は書き込まず、処理済みも返さない", async () => {
    const { engine, fake, manager } = setup();
    await manager.connect("t1");

    manager.dispose("t1");
    fake.channels[0]?.send(new Uint8Array([1, 2]).buffer);
    engine.created[0]?.emitInput("late");
    await flush();

    expect(fake.channels[0]?.closed).toBe(true);
    expect(engine.created[0]?.written).toEqual([]);
    expect(commands(fake)).toEqual(["connect_terminal"]);
  });

  it("接続の応答を待つあいだに破棄したら、応答のあとも入力と寸法を送らない", async () => {
    let finish: (() => void) | undefined;
    const { engine, fake, manager } = setup({
      connect_terminal: () =>
        new Promise<ConnectOutcome>((resolve) => {
          finish = () => resolve({ sizeApplied: true });
        }),
    });

    const connecting = manager.connect("t1");
    engine.created[0]?.emitInput("a");
    manager.dispose("t1");
    finish?.();
    await connecting;
    await flush();

    expect(fake.channels[0]?.closed).toBe(true);
    expect(commands(fake)).toEqual(["connect_terminal"]);
  });

  it("取り込みの途中で破棄したら処理済みを返さない", async () => {
    const { engine, fake, manager } = setup();
    await manager.connect("t1");
    const surface = engine.created[0];
    if (surface === undefined) throw new Error("サーフェスが無い");
    let settle: ((n: number) => void) | undefined;
    surface.write = () =>
      new Promise<number>((resolve) => {
        settle = resolve;
      });

    fake.channels[0]?.send(new Uint8Array([1, 2, 3]).buffer);
    manager.dispose("t1");
    settle?.(3);
    await flush();

    expect(commands(fake)).toEqual(["connect_terminal"]);
  });

  it("サーフェスの取り込みが失敗したら報告する", async () => {
    const { engine, fake, errors, manager } = setup();
    await manager.connect("t1");
    const surface = engine.created[0];
    if (surface === undefined) throw new Error("サーフェスが無い");
    surface.write = () => Promise.reject(new Error("壊れた"));

    fake.channels[0]?.send(new Uint8Array([1]).buffer);
    await flush();

    expect(errors).toEqual(["ターミナル t1 に出力を書き込めなかった: 壊れた"]);
  });

  it("報告先を渡さなければ console.error に出す", async () => {
    const engine = createFakeTerminalEngine();
    const fake = createFakeIpc({
      connect_terminal: () => {
        throw "失敗";
      },
    });
    const logged: unknown[] = [];
    const original = console.error;
    console.error = (...args: unknown[]) => logged.push(args.join(" "));
    try {
      await new TerminalManager(engine, createIpc(fake.transport)).connect("t1");
    } finally {
      console.error = original;
    }

    expect(logged).toEqual(["ターミナル t1 に接続できなかった: 失敗"]);
  });
});

/** 解決を遅らせられる偽の応答。呼ばれるたびに保留の Promise を返し、テストが古い順に成功・失敗させる。 */
function deferred() {
  const waiting: { resolve: () => void; reject: (cause: unknown) => void }[] = [];
  const handler = () =>
    new Promise<null>((resolve, reject) => waiting.push({ resolve: () => resolve(null), reject }));
  const settle = async (outcome: "ok" | { fail: unknown } = "ok") => {
    const next = waiting.shift();
    if (next === undefined) throw new Error("応答待ちのものが無い");
    if (outcome === "ok") next.resolve();
    else next.reject(outcome.fail);
    await flush();
  };
  return { handler, settle, pending: () => waiting.length };
}

const writes = (fake: ReturnType<typeof createFakeIpc>) =>
  fake.calls.filter((c) => c.command === "write_terminal").map((c) => (c.args as { data: string }).data);
const resizes = (fake: ReturnType<typeof createFakeIpc>) =>
  fake.calls
    .filter((c) => c.command === "resize_terminal")
    .map((c) => {
      const a = c.args as { rows: number; cols: number };
      return [a.rows, a.cols];
    });

describe("TerminalManager(入力・寸法の送信の直列化。M-1)", () => {
  it("前の write_terminal が終わるまで次を送らず、送信中に来た入力は連結して次の 1 回で送る(順序を保つ)", async () => {
    const write = deferred();
    const { engine, fake, manager } = setup({ write_terminal: write.handler });
    await manager.connect("t1");
    const surface = engine.created[0];
    if (surface === undefined) throw new Error("サーフェスが無い");

    surface.emitInput("a");
    surface.emitInput("b");
    surface.emitInput("c");
    await flush();
    expect(writes(fake)).toEqual(["a"]);

    await write.settle();
    expect(writes(fake)).toEqual(["a", "bc"]);
    surface.emitInput("d");
    await flush();
    expect(writes(fake)).toEqual(["a", "bc"]);

    await write.settle();
    await write.settle();
    expect(writes(fake)).toEqual(["a", "bc", "d"]);
  });

  it("書き込みが失敗したら報告し、待っていた入力を次に送る", async () => {
    const write = deferred();
    const { engine, fake, errors, manager } = setup({ write_terminal: write.handler });
    await manager.connect("t1");
    const surface = engine.created[0];
    if (surface === undefined) throw new Error("サーフェスが無い");

    surface.emitInput("a");
    surface.emitInput("b");
    await flush();
    await write.settle({ fail: "書けなかった" });

    expect(errors).toEqual(["ターミナル t1 への入力を送れなかった: 書けなかった"]);
    expect(writes(fake)).toEqual(["a", "b"]);
  });

  it("寸法は送信中に変わったら、最新の 1 つだけを次に送る", async () => {
    const resize = deferred();
    const { engine, fake, manager } = setup({ resize_terminal: resize.handler });
    await manager.connect("t1");
    const surface = engine.created[0];
    if (surface === undefined) throw new Error("サーフェスが無い");

    surface.emitResize(80, 24);
    surface.emitResize(90, 30);
    surface.emitResize(100, 40);
    await flush();
    expect(resizes(fake)).toEqual([[24, 80]]);

    await resize.settle();
    await resize.settle();
    expect(resizes(fake)).toEqual([
      [24, 80],
      [40, 100],
    ]);
  });

  it("寸法の送信が失敗しても、次の寸法は送る", async () => {
    const resize = deferred();
    const { engine, fake, errors, manager } = setup({ resize_terminal: resize.handler });
    await manager.connect("t1");
    const surface = engine.created[0];
    if (surface === undefined) throw new Error("サーフェスが無い");

    surface.emitResize(80, 24);
    surface.emitResize(90, 30);
    await flush();
    await resize.settle({ fail: "変えられなかった" });

    expect(errors).toEqual(["ターミナル t1 の寸法を変えられなかった: 変えられなかった"]);
    expect(resizes(fake)).toEqual([
      [24, 80],
      [30, 90],
    ]);
  });

  it("送信中に破棄したら、応答のあとも待っていた入力と寸法を送らない", async () => {
    const write = deferred();
    const resize = deferred();
    const { engine, fake, manager } = setup({ write_terminal: write.handler, resize_terminal: resize.handler });
    await manager.connect("t1");
    const surface = engine.created[0];
    if (surface === undefined) throw new Error("サーフェスが無い");

    surface.emitInput("a");
    surface.emitInput("b");
    surface.emitResize(80, 24);
    surface.emitResize(90, 30);
    await flush();
    manager.dispose("t1");
    await write.settle();
    await resize.settle();

    expect(writes(fake)).toEqual(["a"]);
    expect(resizes(fake)).toEqual([[24, 80]]);
  });
});

describe("TerminalManager(シェルの終了のあとは送らない。S-3)", () => {
  it("終了の状態を伝えたあとの入力と寸法は送らず、報告もしない", async () => {
    const { engine, fake, errors, manager } = setup({
      write_terminal: () => {
        throw "シェルは終了している";
      },
    });
    await manager.connect("t1");
    const surface = engine.created[0];
    if (surface === undefined) throw new Error("サーフェスが無い");

    manager.setStatus("t1", { kind: "exited", code: 0, signal: null });
    for (let i = 0; i < 60; i++) surface.emitInput("x");
    surface.emitResize(100, 40);
    await flush();

    expect(commands(fake)).toEqual(["connect_terminal"]);
    expect(errors).toEqual([]);
  });

  it("終了を伝えた時点で送信待ちだった入力も捨てる", async () => {
    const write = deferred();
    const { engine, fake, manager } = setup({ write_terminal: write.handler });
    await manager.connect("t1");
    const surface = engine.created[0];
    if (surface === undefined) throw new Error("サーフェスが無い");

    surface.emitInput("a");
    surface.emitInput("b");
    await flush();
    manager.setStatus("t1", { kind: "exited", code: null, signal: "SIGHUP" });
    await write.settle();

    expect(writes(fake)).toEqual(["a"]);
  });

  it("サーフェスを作る前に伝えた終了も、作ったあとの入力に効く", async () => {
    const { engine, fake, manager } = setup();
    manager.setStatus("t1", { kind: "exited", code: 1, signal: null });
    await manager.connect("t1");

    engine.created[0]?.emitInput("x");
    await flush();

    expect(writes(fake)).toEqual([]);
  });

  it("起動の失敗のあとでも、次の接続(起動のやり直し)が成功すれば入力を送る", async () => {
    let fail = true;
    const { engine, fake, manager } = setup({
      connect_terminal: () => {
        if (fail) throw "シェルを起動できなかった";
        return { sizeApplied: true };
      },
    });
    await manager.connect("t1");
    manager.setStatus("t1", { kind: "failed", message: "シェルを起動できなかった" });

    fail = false;
    await manager.connect("t1");
    engine.created[0]?.emitInput("ls\r");
    await flush();

    expect(commands(fake)).toEqual(["connect_terminal", "connect_terminal", "write_terminal"]);
    expect(writes(fake)).toEqual(["ls\r"]);
  });

  it("実行中・起動前の状態は入力を止めない", async () => {
    const { engine, fake, manager } = setup();
    await manager.connect("t1");
    manager.setStatus("t1", { kind: "running" });
    manager.setStatus("t1", { kind: "notStarted" });

    engine.created[0]?.emitInput("a");
    await flush();

    expect(writes(fake)).toEqual(["a"]);
  });
});

describe("TerminalManager(送信待ちの入力の上限。C-1)", () => {
  it("接続を待つあいだの入力は上限までに留め、超えたら 1 回だけ報告する", async () => {
    let finish: (() => void) | undefined;
    const { engine, fake, errors, manager } = setup({
      connect_terminal: () =>
        new Promise<ConnectOutcome>((resolve) => {
          finish = () => resolve({ sizeApplied: true });
        }),
    });
    const connecting = manager.connect("t1");
    const surface = engine.created[0];
    if (surface === undefined) throw new Error("サーフェスが無い");
    const chunk = "x".repeat(INPUT_QUEUE_LIMIT / 2);

    surface.emitInput(chunk);
    surface.emitInput(chunk);
    surface.emitInput("over");
    surface.emitInput("more");
    finish?.();
    await connecting;
    await flush();

    expect(writes(fake).map((d) => d.length)).toEqual([INPUT_QUEUE_LIMIT]);
    expect(errors).toEqual([
      `ターミナル t1 の送信待ちの入力が上限(${INPUT_QUEUE_LIMIT} 文字)を超えたので、超えた分を捨てた`,
    ]);
  });

  it("送信中に溜まる入力にも同じ上限を掛ける。送信中でなければ大きな貼り付けもそのまま送る", async () => {
    const write = deferred();
    const { engine, fake, errors, manager } = setup({ write_terminal: write.handler });
    await manager.connect("t1");
    const surface = engine.created[0];
    if (surface === undefined) throw new Error("サーフェスが無い");
    const big = "y".repeat(INPUT_QUEUE_LIMIT + 10);

    surface.emitInput(big);
    surface.emitInput(big);
    await flush();
    await write.settle();

    expect(writes(fake).map((d) => d.length)).toEqual([INPUT_QUEUE_LIMIT + 10, INPUT_QUEUE_LIMIT]);
    expect(errors).toHaveLength(1);
  });
});

/** 画面のイベントの clipboardData の偽物(DataTransfer の text/plain だけ)。 */
function fakeClipboard(initial: Record<string, string> = {}) {
  const items = new Map(Object.entries(initial));
  return {
    items,
    getData: (format: string) => items.get(format) ?? "",
    setData: (format: string, data: string) => {
      items.set(format, data);
    },
  };
}

describe("TerminalManager(表示と非表示・描画方式。T21、設計書 §2.2)", () => {
  it("隠したターミナルにも出力が届き続け、取り込みが済んだ量が Rust へ返る。接続も切らない", async () => {
    const { engine, fake, manager } = setup();
    const { slot } = fakeSlot();
    manager.attach("t1", slot);
    await flush();
    const surface = engine.created[0];
    if (surface === undefined) throw new Error("サーフェスが無い");

    manager.setVisible("t1", false);
    fake.channels[0]?.send(new Uint8Array([121, 10]).buffer);
    fake.channels[0]?.send(new Uint8Array([121, 10, 121, 10]).buffer);
    await flush();

    expect(surface.visible).toBe(false);
    expect(surface.written.map((d) => d.byteLength)).toEqual([2, 4]);
    expect(fake.calls.filter((c) => c.command === "ack_terminal_output").map((c) => c.args)).toEqual([
      { terminalId: "t1", bytes: 2 },
      { terminalId: "t1", bytes: 4 },
    ]);
    expect(fake.channels[0]?.closed).toBe(false);
    expect(manager.get("t1")).toBe(surface);
  });

  it("隠すときは寸法を合わせず、表示に戻すと寸法を合わせ直す", () => {
    const { engine, manager } = setup();
    manager.getOrCreate("t1");
    const surface = engine.created[0];
    if (surface === undefined) throw new Error("サーフェスが無い");

    manager.setVisible("t1", false);
    expect(surface.visibleCalls).toEqual([false]);
    expect(surface.fitCount).toBe(0);

    manager.setVisible("t1", true);
    expect(surface.visibleCalls).toEqual([false, true]);
    expect(surface.visible).toBe(true);
    expect(surface.fitCount).toBe(1);
  });

  it("持っていない ID の表示の切り替えは何もしない(サーフェスを作らない)", () => {
    const { engine, manager } = setup();

    manager.setVisible("none", false);

    expect(engine.created).toEqual([]);
  });

  it("描画方式を引け(無ければ作らず undefined)、変わったら受け取り手に届き、外したら届かない", () => {
    const { engine, manager } = setup();
    const seen: string[] = [];

    expect(manager.rendererOf("t1")).toBeUndefined();
    expect(engine.created).toHaveLength(0);
    const off = manager.onRendererChange("t1", (renderer) => seen.push(renderer));
    const surface = engine.created[0];
    if (surface === undefined) throw new Error("サーフェスが無い");
    expect(engine.created).toHaveLength(1);
    expect(manager.rendererOf("t1")).toBe("dom");

    surface.rendererKind = "webgl";
    expect(manager.rendererOf("t1")).toBe("webgl");
    surface.emitRendererChange("dom");
    off();
    surface.emitRendererChange("webgl");

    expect(seen).toEqual(["dom"]);
  });
});

describe("TerminalManager(コピー・ペーストの振り分け。FR-043、設計書 §9.1)", () => {
  it("フォーカスのあるターミナルに選択があれば、コピーで選択の文字列を text/plain に書き、処理したと返す", () => {
    const { engine, manager } = setup();
    manager.getOrCreate("t1");
    manager.getOrCreate("t2");
    const [first, second] = engine.created;
    if (!first || !second) throw new Error("サーフェスが作られていない");
    first.selection = "選ばれていない";
    second.focused = true;
    second.selection = "hello 日本語";
    const clipboard = fakeClipboard();

    expect(manager.handleClipboard("copy", clipboard)).toBe(true);
    expect(clipboard.items.get("text/plain")).toBe("hello 日本語");
  });

  it("フォーカスのあるターミナルに選択が無ければ、コピーは何もせず、処理しなかったと返す", () => {
    const { engine, manager } = setup();
    manager.getOrCreate("t1");
    const surface = engine.created[0];
    if (!surface) throw new Error("サーフェスが作られていない");
    surface.focused = true;
    const clipboard = fakeClipboard({ "text/plain": "前の中身" });

    expect(manager.handleClipboard("copy", clipboard)).toBe(false);
    expect(clipboard.items.get("text/plain")).toBe("前の中身");
  });

  it("ペーストはフォーカスのあるターミナルに text/plain を貼り付け、write_terminal に届く", async () => {
    const { engine, fake, manager } = setup();
    await manager.connect("t1");
    await manager.connect("t2");
    const [first, second] = engine.created;
    if (!first || !second) throw new Error("サーフェスが作られていない");
    second.focused = true;

    expect(manager.handleClipboard("paste", fakeClipboard({ "text/plain": "echo hi" }))).toBe(true);
    await flush();

    expect(first.pasted).toEqual([]);
    expect(second.pasted).toEqual(["echo hi"]);
    expect(fake.calls.filter((c) => c.command === "write_terminal")).toEqual([
      { command: "write_terminal", args: { terminalId: "t2", data: "echo hi" } },
    ]);
  });

  it("ペーストする文字列が無い(画像だけなど)ときは、貼り付けないが、処理したと返す(既定の貼り付けもさせない)", () => {
    const { engine, manager } = setup();
    manager.getOrCreate("t1");
    const surface = engine.created[0];
    if (!surface) throw new Error("サーフェスが作られていない");
    surface.focused = true;

    expect(manager.handleClipboard("paste", fakeClipboard({ "image/png": "..." }))).toBe(true);
    expect(surface.pasted).toEqual([]);
  });

  it("どのターミナルにもフォーカスが無ければ、コピーもペーストも処理しない", () => {
    const { engine, manager } = setup();
    manager.getOrCreate("t1");
    const surface = engine.created[0];
    if (!surface) throw new Error("サーフェスが作られていない");
    surface.selection = "選択はあるがフォーカスが無い";
    const clipboard = fakeClipboard({ "text/plain": "x" });

    expect(manager.handleClipboard("copy", clipboard)).toBe(false);
    expect(manager.handleClipboard("paste", clipboard)).toBe(false);
    expect(clipboard.items.get("text/plain")).toBe("x");
    expect(surface.pasted).toEqual([]);
  });

  it("破棄したターミナルは振り分けの対象にならない", () => {
    const { engine, manager } = setup();
    manager.getOrCreate("t1");
    const surface = engine.created[0];
    if (!surface) throw new Error("サーフェスが作られていない");
    surface.focused = true;
    manager.dispose("t1");

    expect(manager.handleClipboard("paste", fakeClipboard({ "text/plain": "x" }))).toBe(false);
  });
});

describe("TerminalManager(9 枚の検証画面。T22、設計書 §2.2「非表示のインスタンス」)", () => {
  const IDS = Array.from({ length: 9 }, (_, i) => `terminal-${i + 1}`);

  /** 9 枚を置き場所に入れて接続し、ID ごとのサーフェスとチャネルを返す。 */
  async function setupGrid() {
    const ctx = setup();
    for (const id of IDS) ctx.manager.attach(id, fakeSlot().slot);
    await flush();
    const surfaceOf = (id: string) => {
      const surface = ctx.engine.created.find((s) => s.id === id);
      if (surface === undefined) throw new Error(`${id} のサーフェスが無い`);
      return surface;
    };
    const channelOf = (id: string) => {
      const index = ctx.fake.calls
        .filter((c) => c.command === "connect_terminal")
        .findIndex((c) => (c.args as { terminalId: string }).terminalId === id);
      const channel = ctx.fake.channels[index];
      if (channel === undefined) throw new Error(`${id} のチャネルが無い`);
      return channel;
    };
    const argsOf = (command: string) => ctx.fake.calls.filter((c) => c.command === command).map((c) => c.args);
    const applyView = (from: GridView, to: GridView) => {
      for (const change of visibilityChanges(IDS, from, to)) ctx.manager.setVisible(change.id, change.visible);
    };
    return { ...ctx, surfaceOf, channelOf, argsOf, applyView };
  }

  it("9 枚をそれぞれの寸法で 1 回ずつ接続する", async () => {
    const { argsOf } = await setupGrid();

    const connects = argsOf("connect_terminal") as { terminalId: string; rows: number; cols: number }[];
    expect(connects.map((c) => c.terminalId)).toEqual(IDS);
    expect(connects.every((c) => c.rows > 0 && c.cols > 0)).toBe(true);
  });

  it("1 枚だけ表示では残りの 8 枚に setVisible(false) が届き、隠した 8 枚も取り込みを続けて処理済みの量を返す", async () => {
    const { surfaceOf, channelOf, argsOf, applyView, fake } = await setupGrid();

    applyView(SHOW_ALL, showOnly(IDS, "terminal-1"));
    for (const id of IDS) channelOf(id).send(new TextEncoder().encode(`out-${id}\r\n`).buffer);
    await flush();

    expect(surfaceOf("terminal-1").visibleCalls).toEqual([]);
    for (const id of IDS.slice(1)) {
      expect(surfaceOf(id).visibleCalls, id).toEqual([false]);
      expect(surfaceOf(id).visible, id).toBe(false);
    }
    for (const id of IDS) {
      expect(new TextDecoder().decode(surfaceOf(id).written[0]), id).toBe(`out-${id}\r\n`);
    }
    expect(argsOf("ack_terminal_output")).toEqual(
      IDS.map((id) => ({ terminalId: id, bytes: `out-${id}\r\n`.length })),
    );
    expect(fake.channels.every((c) => !c.closed)).toBe(true);
  });

  it("隠している間に表示中の 1 枚の寸法が変わっても、隠した 8 枚の寸法は送らず、表示に戻したときに自分の置き場所に合わせ直す", async () => {
    const { surfaceOf, argsOf, applyView, manager } = await setupGrid();
    const single = showOnly(IDS, "terminal-1");
    applyView(SHOW_ALL, single);

    // ウィンドウの大きさを変えた: 表示中の 1 枚は合わせ直す。隠した 8 枚は置き場所の大きさが 0 なので決め直さない。
    surfaceOf("terminal-1").fitTo = { rows: 60, cols: 200 };
    for (const id of IDS.slice(1)) surfaceOf(id).fitTo = { rows: 0, cols: 0 };
    for (const id of IDS) manager.fit(id);
    await flush();
    expect(argsOf("resize_terminal")).toEqual([{ terminalId: "terminal-1", rows: 60, cols: 200 }]);

    // 9 枚表示に戻す: 8 枚はそれぞれのマスの大きさに合わせ直す(変わったものだけ寸法を送る)。
    for (const id of IDS.slice(1)) surfaceOf(id).fitTo = { rows: 20, cols: 66 };
    surfaceOf("terminal-9").fitTo = surfaceOf("terminal-9").size(); // 大きさが変わらなかったマス
    applyView(single, SHOW_ALL);
    await flush();

    const resized = argsOf("resize_terminal") as { terminalId: string; rows: number; cols: number }[];
    expect(resized.slice(1)).toEqual(IDS.slice(1, 8).map((id) => ({ terminalId: id, rows: 20, cols: 66 })));
    expect(resized.every((r) => r.rows > 0 && r.cols > 0)).toBe(true);
    for (const id of IDS) expect(surfaceOf(id).visible, id).toBe(true);
  });

  it("フォーカスを指定のサーフェスへ移す。隠しているもの・持っていない ID では何もしない", async () => {
    const { surfaceOf, applyView, manager, engine } = await setupGrid();

    manager.focus("terminal-4");
    expect(surfaceOf("terminal-4").hasFocus()).toBe(true);

    applyView(SHOW_ALL, showOnly(IDS, "terminal-1"));
    manager.focus("terminal-6");
    expect(surfaceOf("terminal-6").hasFocus()).toBe(false);

    manager.focus("none");
    expect(engine.created).toHaveLength(9);
  });
});

/** 偽のエンジンが作った ID のサーフェス(最後に作ったもの)。無ければ例外。 */
function fakeSurface(engine: ReturnType<typeof createFakeTerminalEngine>, id: string) {
  const surface = engine.created.findLast((s) => s.id === id);
  if (surface === undefined) throw new Error(`${id} のサーフェスが無い`);
  return surface;
}

describe("TerminalManager(写しとの同期。T34b、FR-042)", () => {
  it("写しから消えたターミナルは破棄され購読が外れる(exited を経ずに消えても)", async () => {
    const { engine, fake, manager } = setup();
    await manager.connect("a");
    await manager.connect("b");

    manager.retain(["b"]);
    fake.channels[0]?.send(new Uint8Array([1, 2]).buffer);
    await flush();

    expect(engine.created.find((s) => s.id === "a")?.disposeCount).toBe(1);
    expect(engine.created.find((s) => s.id === "b")?.disposeCount).toBe(0);
    expect(fake.channels[0]?.closed).toBe(true);
    expect(fake.channels[1]?.closed).toBe(false);
    expect(manager.get("a")).toBeUndefined();
    expect(commands(fake)).toEqual(["connect_terminal", "connect_terminal"]);
  });

  it("破棄に失敗したターミナルがあっても、残りの破棄を続けて報告する", () => {
    const { engine, errors, manager } = setup();
    engine.failDisposeOnNext = true;
    manager.getOrCreate("a");
    manager.getOrCreate("b");

    manager.retain([]);

    expect(manager.size).toBe(0);
    expect(engine.created[1]?.disposeCount).toBe(1);
    expect(errors).toEqual(["ターミナル a を破棄できなかった: 偽のサーフェス a の破棄に失敗した"]);
  });

  it("プロジェクトを切り替えてもサーフェスを破棄しない。前の表示中は隠し、出力は取り込み続ける", async () => {
    const { engine, fake, manager } = setup();
    manager.present("a", []);
    manager.attach("a", fakeSlot().slot);
    await flush();

    // B に切り替えた写し: B が表示中、A は隠して取り込みだけ。
    manager.present("b", ["a"]);
    manager.attach("b", fakeSlot().slot);
    await flush();
    fake.channels[0]?.send(new Uint8Array([7]).buffer);
    await flush();

    const a = engine.created.find((s) => s.id === "a");
    expect(a?.disposeCount).toBe(0);
    expect(a?.visible).toBe(false);
    expect(a?.written.map((d) => [...d])).toEqual([[7]]);
    expect(fake.calls.filter((c) => c.command === "ack_terminal_output").map((c) => c.args)).toEqual([
      { terminalId: "a", bytes: 1 },
    ]);

    // A に戻す: 同じサーフェスを表示に戻す。
    manager.present("a", ["b"]);
    expect(engine.created.filter((s) => s.id === "a")).toHaveLength(1);
    expect(a?.visible).toBe(true);
    expect(engine.created.find((s) => s.id === "b")?.visible).toBe(false);
  });

  it("隠したまま作ったサーフェスは表示中の寸法で接続する(表示中の接続を待ってからつなぐ)", async () => {
    const { engine, fake, manager } = setup();
    // 起動時: A を表示、B・C は隠したまま。A はまだ置き場所に入っていない。
    manager.present("a", ["b", "c"]);
    await flush();
    expect(commands(fake)).toEqual([]);

    manager.getOrCreate("a");
    fakeSurface(engine, "a").fitTo = { rows: 30, cols: 100 };
    manager.attach("a", fakeSlot().slot);
    await flush();

    const connects = fake.calls
      .filter((c) => c.command === "connect_terminal")
      .map((c) => c.args as { terminalId: string; rows: number; cols: number });
    expect(connects.map((c) => [c.terminalId, c.rows, c.cols])).toEqual([
      ["a", 30, 100],
      ["b", 30, 100],
      ["c", 30, 100],
    ]);
    for (const id of ["b", "c"]) {
      const surface = engine.created.find((s) => s.id === id);
      expect(surface?.visible, id).toBe(false);
      expect(surface?.size(), id).toEqual({ rows: 30, cols: 100 });
    }
    // 隠したまま作った分は、接続の前に寸法を合わせたので、寸法のコマンドを別に送らない。
    expect(commands(fake).filter((c) => c === "resize_terminal")).toEqual([]);
  });

  it("表示中が後から変わったら、隠したターミナルは最後に表示していた寸法で始める", async () => {
    const { engine, fake, manager } = setup();
    manager.getOrCreate("a");
    const surfaceA = fakeSurface(engine, "a");
    surfaceA.fitTo = { rows: 30, cols: 100 };
    manager.present("a", []);
    manager.attach("a", fakeSlot().slot);
    await flush();
    surfaceA.fitTo = { rows: 40, cols: 120 };
    manager.fit("a");
    await flush();

    // 別のプロジェクトのタブ(写しで新しく届いた)を隠したまま作る。
    manager.present("a", ["z"]);
    await flush();

    const z = fake.calls
      .filter((c) => c.command === "connect_terminal")
      .map((c) => c.args as { terminalId: string; rows: number; cols: number })
      .find((c) => c.terminalId === "z");
    expect(z).toMatchObject({ rows: 40, cols: 120 });
  });

  it("表示中が無ければエンジンの既定の寸法で接続する", async () => {
    const { engine, fake, manager } = setup();

    manager.present(null, ["b"]);
    await flush();

    expect(fake.calls.map((c) => c.args)).toEqual([
      expect.objectContaining({ terminalId: "b", rows: 24, cols: 80 }),
    ]);
    expect(engine.created[0]?.visible).toBe(false);
    expect(engine.created[0]?.resized).toEqual([]);
  });

  it("隠したターミナルの接続は 1 回だけ試す(写しが届くたびに接続し直さない)", async () => {
    const { fake, errors, manager } = setup({
      connect_terminal: () => {
        throw "失敗";
      },
    });

    manager.present(null, ["b"]);
    await flush();
    manager.present(null, ["b"]);
    await flush();

    expect(commands(fake)).toEqual(["connect_terminal"]);
    expect(errors).toEqual(["ターミナル b に接続できなかった: 失敗"]);
  });

  it("待たせていた隠したターミナルが表示中になったら、その置き場所の寸法で接続する", async () => {
    const { engine, fake, manager } = setup();
    manager.present("a", ["b"]);
    await flush();

    // A を置き場所に入れる前に B へ切り替わった。
    manager.present("b", ["a"]);
    manager.getOrCreate("b");
    fakeSurface(engine, "b").fitTo = { rows: 33, cols: 99 };
    manager.attach("b", fakeSlot().slot);
    await flush();

    const connects = fake.calls
      .filter((c) => c.command === "connect_terminal")
      .map((c) => c.args as { terminalId: string; rows: number; cols: number });
    expect(connects.map((c) => [c.terminalId, c.rows, c.cols])).toEqual([
      ["b", 33, 99],
      ["a", 33, 99],
    ]);
  });

  it("接続を待たせているうちに写しから消えたターミナルは、破棄して接続しない", async () => {
    const { engine, fake, manager } = setup();
    manager.present("a", ["b"]);
    manager.retain(["a"]);
    manager.attach("a", fakeSlot().slot);
    await flush();

    expect(fakeSurface(engine, "b").disposeCount).toBe(1);
    expect(manager.get("b")).toBeUndefined();
    expect(fake.calls.map((c) => (c.args as { terminalId: string }).terminalId)).toEqual(["a"]);
  });
});

describe("TerminalManager(バイト列の入力。onBinary、M1 の見送り)", () => {
  it("onBinary_のバイト列が順に送られる(文字の値をそのままバイトにし、文字列の入力と同じ列で直列に送る)", async () => {
    const write = deferred();
    const { engine, fake, manager } = setup({ write_terminal: write.handler });
    await manager.connect("t1");
    const surface = engine.created[0];
    if (surface === undefined) throw new Error("サーフェスが無い");

    surface.emitInput("a");
    surface.emitBinary("\x1b[M\xff\x80");
    surface.emitInput("b");
    surface.emitInput("c");
    await flush();
    expect(commands(fake).slice(1)).toEqual(["write_terminal"]);
    await write.settle();
    await flush();
    await write.settle();
    await flush();

    expect(
      fake.calls.slice(1).map((c) => [c.command, (c.args as { data: unknown }).data]),
    ).toEqual([
      ["write_terminal", "a"],
      ["write_terminal_bytes", [0x1b, 0x5b, 0x4d, 0xff, 0x80]],
      ["write_terminal", "bc"],
    ]);
  });

  it("接続の前のバイト列も溜めて、接続のあとに送る。終わったシェルには送らない", async () => {
    let finish: (() => void) | undefined;
    const { engine, fake, manager } = setup({
      connect_terminal: () =>
        new Promise<ConnectOutcome>((resolve) => {
          finish = () => resolve({ sizeApplied: true });
        }),
    });
    const connecting = manager.connect("t1");
    engine.created[0]?.emitBinary("\x01\x02");
    engine.created[0]?.emitBinary("\x03");
    finish?.();
    await connecting;
    await flush();
    manager.setStatus("t1", { kind: "exited", code: 0, signal: null });
    engine.created[0]?.emitBinary("\x04");
    await flush();

    expect(fake.calls.slice(1)).toEqual([
      { command: "write_terminal_bytes", args: { terminalId: "t1", data: [1, 2, 3] } },
    ]);
  });

  it("バイト列の送信の失敗は報告する", async () => {
    const { engine, errors, manager } = setup({
      write_terminal_bytes: () => {
        throw new Error("塞がっている");
      },
    });
    await manager.connect("t1");

    engine.created[0]?.emitBinary("\x01");
    await flush();

    expect(errors).toEqual(["ターミナル t1 への入力を送れなかった: 塞がっている"]);
  });
});

describe("TerminalManager(すべてを選択。M1 の見送り、設計書 §9.1)", () => {
  it("すべてを選択はフォーカスのあるターミナルを全選択する", () => {
    const { engine, manager } = setup();
    manager.getOrCreate("a");
    manager.getOrCreate("b");
    manager.focus("b");

    expect(manager.handleMenuCommand("select-all")).toBe(true);

    expect(engine.created.map((s) => s.selectAllCount)).toEqual([0, 1]);
  });

  it("どのターミナルにもフォーカスが無ければ何もしない(画面の既定に任せる)", () => {
    const { engine, manager } = setup();
    manager.getOrCreate("a");

    expect(manager.handleMenuCommand("select-all")).toBe(false);

    expect(engine.created[0]?.selectAllCount).toBe(0);
  });
});

describe("TerminalManager(接続の応答で寸法を反映していないとき。M1 の見送り)", () => {
  it("寸法を反映していない応答なら寸法を送り直す(寸法が変わっていなくても 1 回)", async () => {
    const { fake, manager } = setup({ connect_terminal: () => ({ sizeApplied: false }) });

    await manager.connect("t1");
    await flush();

    expect(fake.calls.slice(1)).toEqual([
      { command: "resize_terminal", args: { terminalId: "t1", rows: 24, cols: 80 } },
    ]);
  });

  it("寸法を反映した応答で寸法が変わっていなければ送らない", async () => {
    const { fake, manager } = setup();

    await manager.connect("t1");
    await flush();

    expect(commands(fake)).toEqual(["connect_terminal"]);
  });

  it("寸法を反映していなくても、終わったシェルには送らない", async () => {
    const { fake, manager } = setup({ connect_terminal: () => ({ sizeApplied: false }) });
    manager.setStatus("t1", { kind: "exited", code: 0, signal: null });

    await manager.connect("t1");
    await flush();

    expect(commands(fake)).toEqual(["connect_terminal"]);
  });
});

describe("TerminalManager(バイト列にできない出力。M1 の見送り、報告だけ)", () => {
  it("バイト列にできない中身は報告する(取り込まず、処理済みも返さない)", async () => {
    const { engine, errors, fake, manager } = setup();
    await manager.connect("t1");

    fake.channels[0]?.send("文字列");
    fake.channels[0]?.send({ data: 1 });
    await flush();

    expect(engine.created[0]?.written).toEqual([]);
    expect(commands(fake)).toEqual(["connect_terminal"]);
    expect(errors).toEqual([
      "ターミナル t1 の出力をバイト列にできなかったので捨てた(string)",
      "ターミナル t1 の出力をバイト列にできなかったので捨てた(object)",
    ]);
  });
});
