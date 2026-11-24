import { describe, expect, it } from "vitest";

import { DEFAULT_SHORTCUT_TABLE } from "../logic/keys/defaultShortcuts";
import { PREVIEW_DELAY_MS, type PreviewTimer } from "../logic/editor/previewSchedule";
import { createFakeEditorEngine, type FakeEditorEngine, fakeBlocks } from "../test/fakeEditorEngine";
import type { EditorEngine, EditorSlot } from "./engine";
import { EditorManager, type EditorManagerOptions } from "./EditorManager";

function fakeSlot() {
  const children: unknown[] = [];
  const slot: EditorSlot = {
    appendChild(element) {
      children.push(element);
    },
    removeChild(element) {
      const index = children.indexOf(element);
      if (index >= 0) children.splice(index, 1);
    },
  };
  return { slot, children };
}

function fakeTimer() {
  let pending: { run: () => void; ms: number }[] = [];
  const timer: PreviewTimer = (run, ms) => {
    const entry = { run, ms };
    pending.push(entry);
    return () => {
      pending = pending.filter((p) => p !== entry);
    };
  };
  const flush = () => {
    const due = pending;
    pending = [];
    for (const p of due) p.run();
  };
  return { timer, flush, pending: () => pending };
}

function setup(overrides: Partial<EditorManagerOptions> = {}) {
  const engine = createFakeEditorEngine();
  const dirty: [string, boolean][] = [];
  const saves: string[] = [];
  const previews: [string, unknown][] = [];
  const errors: string[] = [];
  let loads = 0;
  const clock = fakeTimer();
  const manager = new EditorManager({
    shortcuts: () => DEFAULT_SHORTCUT_TABLE,
    loadEngine: async () => {
      loads++;
      return engine;
    },
    onDirtyChange: (id, value) => dirty.push([id, value]),
    onSave: (id) => saves.push(id),
    onPreview: (id, blocks) => previews.push([id, blocks]),
    reportError: (message) => errors.push(message),
    timer: clock.timer,
    fontSize: 15,
    vim: false,
    ...overrides,
  });
  return { engine, manager, dirty, saves, previews, errors, clock, loads: () => loads };
}

const doc = (text: string) => ({ text, lineEnding: "lf" as const, readOnly: false });

async function opened(engine: FakeEditorEngine, manager: EditorManager, id: string, text = "# a") {
  await manager.open(id, doc(text));
  const surface = engine.created.find((s) => s.id === id);
  if (surface === undefined) throw new Error(`${id} を作っていない`);
  return surface;
}

describe("EditorManager(エディタの部品を React の外で持つ。ADR-015 の 1)", () => {
  it("エンジンは初めて開くときに 1 回だけ読む", async () => {
    const t = setup();
    expect(t.loads()).toBe(0);
    await Promise.all([t.manager.open("e1", doc("a")), t.manager.open("e2", doc("b"))]);
    await t.manager.open("e3", doc("c"));
    expect(t.loads()).toBe(1);
    expect(t.engine.created.map((s) => s.id)).toEqual(["e1", "e2", "e3"]);
  });

  it("改行が混ざった文書は_打って取り消すと未保存でなくなる(基準は部品が揃えた文書)", async () => {
    const t = setup();
    await t.manager.open("e1", { text: "a\r\nb\nc", lineEnding: "crlf", readOnly: false });
    const surface = t.engine.created[0];
    if (surface === undefined) throw new Error("部品を作っていない");
    expect(t.manager.isDirty("e1")).toBe(false);

    surface.type(`${surface.current}X`);
    surface.undo();

    expect(t.manager.isDirty("e1")).toBe(false);
    expect(t.dirty).toEqual([
      ["e1", true],
      ["e1", false],
    ]);
  });

  it("同じ_id_を開き直しても部品を作り直さない", async () => {
    const t = setup();
    await t.manager.open("e1", doc("a"));
    await t.manager.open("e1", doc("b"));
    expect(t.engine.created).toHaveLength(1);
    expect(t.manager.text("e1")).toBe("a");
  });

  it("作るときにいまの文字の大きさ_vim_表を渡す", async () => {
    const t = setup({ fontSize: 18, vim: true });
    await t.manager.open("e1", { text: "x", lineEnding: "crlf", readOnly: true });
    const [surface] = t.engine.created;
    expect(surface?.options).toMatchObject({ text: "x", lineEnding: "crlf", readOnly: true, fontSize: 18, vim: true });
    expect(surface?.options.shortcuts()).toBe(DEFAULT_SHORTCUT_TABLE);
  });

  it("未保存は変わり目だけ知らせる", async () => {
    const t = setup();
    const surface = await opened(t.engine, t.manager, "e1", "a");
    surface.type("ab");
    surface.type("abc");
    surface.type("abcd");
    expect(t.dirty).toEqual([["e1", true]]);
    expect(t.manager.isDirty("e1")).toBe(true);
  });

  it("保存済みの文書に戻すと未保存が消える", async () => {
    const t = setup();
    const surface = await opened(t.engine, t.manager, "e1", "a");
    surface.type("ab");
    surface.undo();
    expect(t.dirty).toEqual([
      ["e1", true],
      ["e1", false],
    ]);
    expect(t.manager.isDirty("e1")).toBe(false);
  });

  it("保存できたら保存した文書を基準にする_保存中に打った分は未保存のまま", async () => {
    const t = setup();
    const surface = await opened(t.engine, t.manager, "e1", "a");
    surface.type("ab");
    const saving = t.manager.text("e1");
    surface.type("abc");
    t.manager.markSaved("e1", saving ?? "");
    expect(t.manager.isDirty("e1")).toBe(true);
    expect(t.dirty).toEqual([["e1", true]]);

    t.manager.markSaved("e1", "abc");
    expect(t.dirty).toEqual([
      ["e1", true],
      ["e1", false],
    ]);
  });

  it("読み直しは文書を差し替えて未保存を消す", async () => {
    const t = setup();
    const surface = await opened(t.engine, t.manager, "e1", "a");
    surface.type("ab");
    t.manager.replaceText("e1", "外で変えた", "crlf");
    expect(surface.setTextCalls).toEqual([{ text: "外で変えた", lineEnding: "crlf" }]);
    expect(t.manager.isDirty("e1")).toBe(false);
    expect(t.dirty.at(-1)).toEqual(["e1", false]);
  });

  it("保存のキーで保存を知らせる", async () => {
    const t = setup();
    const surface = await opened(t.engine, t.manager, "e1");
    surface.pressSave();
    expect(t.saves).toEqual(["e1"]);
  });

  it("隠しても取り消しの履歴が残る", async () => {
    const t = setup();
    const surface = await opened(t.engine, t.manager, "e1", "a");
    const first = fakeSlot();
    const detach = t.manager.attach("e1", first.slot);
    expect(first.children).toEqual([surface.element]);
    surface.type("ab");
    surface.type("abc");

    detach();
    expect(first.children).toEqual([]);
    const second = fakeSlot();
    t.manager.attach("e1", second.slot);

    expect(t.engine.created).toHaveLength(1);
    expect(surface.disposeCount).toBe(0);
    expect(second.children).toEqual([surface.element]);
    surface.undo();
    expect(t.manager.text("e1")).toBe("ab");
  });

  it("古い外す関数はほかの置き場所に移したあとでは何もしない", async () => {
    const t = setup();
    const surface = await opened(t.engine, t.manager, "e1");
    const first = fakeSlot();
    const detachFirst = t.manager.attach("e1", first.slot);
    const second = fakeSlot();
    t.manager.attach("e1", second.slot);
    detachFirst();
    expect(second.children).toEqual([surface.element]);
  });

  it("まだ作っていない_id_の置き場所は作ったら入れる", async () => {
    const t = setup();
    const { slot, children } = fakeSlot();
    t.manager.attach("e1", slot);
    expect(children).toEqual([]);
    const surface = await opened(t.engine, t.manager, "e1");
    expect(children).toEqual([surface.element]);
  });

  it("vim_の切替で部品を作り直さない", async () => {
    const t = setup();
    const surface = await opened(t.engine, t.manager, "e1");
    t.manager.setVim(true);
    t.manager.setVim(true);
    t.manager.setVim(false);
    expect(surface.vimCalls).toEqual([true, false]);
    expect(t.engine.created).toHaveLength(1);
    expect(surface.disposeCount).toBe(0);
    t.manager.setVim(true);
    await t.manager.open("e2", doc("b"));
    expect(t.engine.created[1]?.options.vim).toBe(true);
  });

  it("文字の大きさは全部の部品に当て_新しい部品はその大きさで作る", async () => {
    const t = setup();
    const a = await opened(t.engine, t.manager, "e1");
    const b = await opened(t.engine, t.manager, "e2");
    t.manager.setFontSize(20);
    t.manager.setFontSize(20);
    expect(a.fontSizeCalls).toEqual([20]);
    expect(b.fontSizeCalls).toEqual([20]);
    await t.manager.open("e3", doc("c"));
    expect(t.engine.created[2]?.options.fontSize).toBe(20);
  });

  it("読み取り専用を替えられる", async () => {
    const t = setup();
    const surface = await opened(t.engine, t.manager, "e1");
    t.manager.setReadOnly("e1", true);
    expect(surface.readOnlyCalls).toEqual([true]);
  });

  it("フォーカスと全選択はその部品へ_フォーカスのある_id_を返す", async () => {
    const t = setup();
    const surface = await opened(t.engine, t.manager, "e1");
    await opened(t.engine, t.manager, "e2");
    t.manager.attach("e1", fakeSlot().slot);
    expect(t.manager.focusedId()).toBeNull();
    t.manager.focus("e1");
    expect(t.manager.focusedId()).toBe("e1");
    expect(t.manager.selectAllFocused()).toBe(true);
    expect(surface.selectAllCount).toBe(1);
    surface.focused = false;
    expect(t.manager.selectAllFocused()).toBe(false);
  });

  it("プレビューは見えるときに解析し_続けて打つと最後から待って_1_回", async () => {
    const t = setup();
    const surface = await opened(t.engine, t.manager, "e1", "a");
    expect(t.engine.parsed).toEqual([]);
    t.manager.setPreviewVisible("e1", true);
    expect(t.engine.parsed).toEqual(["a"]);
    expect(t.manager.preview("e1")).toEqual(fakeBlocks("a"));
    expect(t.previews).toEqual([["e1", fakeBlocks("a")]]);

    surface.type("ab");
    surface.type("abc");
    expect(t.clock.pending().map((p) => p.ms)).toEqual([PREVIEW_DELAY_MS]);
    t.clock.flush();
    expect(t.engine.parsed).toEqual(["a", "abc"]);
    expect(t.manager.preview("e1")).toEqual(fakeBlocks("abc"));
  });

  it("プレビューを隠しているあいだは解析しない", async () => {
    const t = setup();
    const surface = await opened(t.engine, t.manager, "e1", "a");
    surface.type("ab");
    t.clock.flush();
    expect(t.engine.parsed).toEqual([]);
    expect(t.manager.preview("e1")).toBeNull();
  });

  it("読み直しはプレビューも作り直す", async () => {
    const t = setup();
    await opened(t.engine, t.manager, "e1", "a");
    t.manager.setPreviewVisible("e1", true);
    t.manager.replaceText("e1", "b", "lf");
    t.clock.flush();
    expect(t.engine.parsed).toEqual(["a", "b"]);
  });

  it("開く前にプレビューを見えるようにしたら開いたときに解析する", async () => {
    const t = setup();
    t.manager.setPreviewVisible("e1", true);
    expect(t.engine.parsed).toEqual([]);
    await opened(t.engine, t.manager, "e1", "a");
    expect(t.engine.parsed).toEqual(["a"]);
  });

  it("閉じると破棄して置き場所から外す_読み込み中に閉じたら作らない", async () => {
    const t = setup();
    const surface = await opened(t.engine, t.manager, "e1");
    const { slot, children } = fakeSlot();
    t.manager.attach("e1", slot);
    t.manager.close("e1");
    expect(surface.disposeCount).toBe(1);
    expect(children).toEqual([]);
    expect(t.manager.has("e1")).toBe(false);
    expect(t.manager.text("e1")).toBeNull();

    const pending = t.manager.open("e2", doc("b"));
    t.manager.close("e2");
    await pending;
    expect(t.engine.created.map((s) => s.id)).toEqual(["e1"]);
  });

  it("retain_で写しから消えたものを閉じる", async () => {
    const t = setup();
    const a = await opened(t.engine, t.manager, "e1");
    const b = await opened(t.engine, t.manager, "e2");
    t.manager.retain(new Set(["e2"]));
    expect(a.disposeCount).toBe(1);
    expect(b.disposeCount).toBe(0);
    expect(t.manager.has("e2")).toBe(true);
  });

  it("エンジンを読めなければ報告して開けない_次はもう一度読む", async () => {
    let attempts = 0;
    const engine: EditorEngine = createFakeEditorEngine();
    const t = setup({
      loadEngine: async () => {
        attempts++;
        if (attempts === 1) throw new Error("読み込めない");
        return engine;
      },
    });
    await expect(t.manager.open("e1", doc("a"))).rejects.toThrow("読み込めない");
    expect(t.errors).toEqual(["エディタの部品を読み込めませんでした: 読み込めない"]);
    await t.manager.open("e1", doc("a"));
    expect(t.manager.has("e1")).toBe(true);
  });

  it("破棄すると全部の部品を手放し以後は開かない", async () => {
    const t = setup();
    const surface = await opened(t.engine, t.manager, "e1");
    t.manager.dispose();
    expect(surface.disposeCount).toBe(1);
    await t.manager.open("e2", doc("b"));
    expect(t.engine.created).toHaveLength(1);
  });
});

describe("EditorManager.renderDiagram(Mermaid の図。ADR-017)", () => {
  it("描画は初めて描くときに 1 回だけ読み、結果をそのまま返す", async () => {
    let loads = 0;
    const sources: string[] = [];
    const t = setup({
      loadDiagrams: async () => {
        loads++;
        return {
          render: async (source) => {
            sources.push(source);
            return { ok: true, light: "<svg>明</svg>", dark: "<svg>暗</svg>" };
          },
        };
      },
    });
    expect(loads).toBe(0);
    const [a, b] = await Promise.all([t.manager.renderDiagram("graph TD; A-->B"), t.manager.renderDiagram("x")]);
    expect(loads).toBe(1);
    expect(a).toEqual({ ok: true, light: "<svg>明</svg>", dark: "<svg>暗</svg>" });
    expect(b.ok).toBe(true);
    expect(sources).toEqual(["graph TD; A-->B", "x"]);
  });

  it("読み込めなければ失敗を返し、次の描画でもう一度読む", async () => {
    let loads = 0;
    const t = setup({
      loadDiagrams: async () => {
        loads++;
        if (loads === 1) throw new Error("塊が無い");
        return { render: async () => ({ ok: false, message: "文法の誤り" }) };
      },
    });
    expect(await t.manager.renderDiagram("a")).toEqual({ ok: false, message: "図の部品を読み込めませんでした: 塊が無い" });
    expect(await t.manager.renderDiagram("a")).toEqual({ ok: false, message: "文法の誤り" });
    expect(loads).toBe(2);
  });
});
