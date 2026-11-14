import { describe, expect, it } from "vitest";

import { createIpc } from "../ipc/client";
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
        return null;
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

  it("接続の応答を待つあいだの入力は、接続のあとに順に送る", async () => {
    let finish: (() => void) | undefined;
    const { engine, fake, manager } = setup({
      connect_terminal: () =>
        new Promise<null>((resolve) => {
          finish = () => resolve(null);
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

    expect(fake.calls.slice(1).map((c) => c.args)).toEqual([
      { terminalId: "t1", data: "a" },
      { terminalId: "t1", data: "b" },
    ]);
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
        new Promise<null>((resolve) => {
          finish = () => resolve(null);
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
        return null;
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
        new Promise<null>((resolve) => {
          finish = () => resolve(null);
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
