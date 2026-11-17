import { describe, expect, it } from "vitest";

import { EditorManager } from "../editor/EditorManager";
import { createIpc } from "../ipc/client";
import type { AppSnapshot } from "../ipc/generated/AppSnapshot";
import type { EditorTabSnapshot } from "../ipc/generated/EditorTabSnapshot";
import type { ProjectCardSnapshot } from "../ipc/generated/ProjectCardSnapshot";
import { DEFAULT_SHORTCUT_TABLE } from "../logic/keys/defaultShortcuts";
import { createFakeEditorEngine } from "../test/fakeEditorEngine";
import { createFakeIpc, type FakeHandlers, sampleSnapshot } from "../test/fakeIpc";
import { createAppStore } from "./appStore";
import { createEditorController, syncEditors } from "./editorController";
import { createUiStore } from "./uiStore";

const card = (overrides: Partial<ProjectCardSnapshot> = {}): ProjectCardSnapshot => ({
  id: "p1",
  name: "app",
  color: "blue",
  displayPath: "~/app",
  missing: false,
  showHidden: false,
  terminalIds: [],
  activeTerminalId: null,
  editorIds: [],
  shownEditorId: null,
  ...overrides,
});

const tab = (id: string, path: string, overrides: Partial<EditorTabSnapshot> = {}): EditorTabSnapshot => ({
  id,
  projectId: "p1",
  path,
  label: path.slice(path.lastIndexOf("/") + 1),
  dirty: false,
  readOnly: null,
  ...overrides,
});

/** p1 に e1(docs/a.md)を開いて表示している写し。 */
const withEditors = (editors: EditorTabSnapshot[], shown: string | null = editors[0]?.id ?? null): AppSnapshot =>
  sampleSnapshot({
    projects: [card({ editorIds: editors.map((e) => e.id), shownEditorId: shown })],
    selectedProjectId: "p1",
    editors,
  });

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

function setup(handlers: Partial<FakeHandlers> = {}) {
  const fake = createFakeIpc({
    open_document: () => ({ kind: "opened", editorId: "e1", text: "# a\n", lineEnding: "lf", readOnly: null }),
    ...handlers,
  });
  const ipc = createIpc(fake.transport);
  const app = createAppStore();
  const ui = createUiStore();
  const engine = createFakeEditorEngine();
  const errors: string[] = [];
  const reportError = (m: string) => errors.push(m);
  // 部品の知らせはコントローラーへつなぐ(main.tsx と同じ順)
  let controller: ReturnType<typeof createEditorController> | null = null;
  const editors = new EditorManager({
    shortcuts: () => DEFAULT_SHORTCUT_TABLE,
    onDirtyChange: (id, dirty) => controller?.handleDirtyChange(id, dirty),
    onSave: (id) => void controller?.save(id),
    fontSize: 15,
    vim: false,
    loadEngine: async () => engine,
    timer: () => () => {},
    reportError,
  });
  controller = createEditorController({ ipc, app, ui, editors: () => editors, reportError });
  const surface = (index = 0) => {
    const found = engine.created[index];
    if (found === undefined) throw new Error("部品が無い");
    return found;
  };
  const commands = () => fake.calls.map((c) => c.command);
  return { fake, app, ui, editors, controller, engine, errors, surface, commands };
}

describe("editorController(開く。AC-E01・E02)", () => {
  it("開くと部品を作り_フォーカスを求める", async () => {
    const t = setup();

    await t.controller.openDocument("p1", "docs/a.md");

    expect(t.fake.calls).toEqual([{ command: "open_document", args: { projectId: "p1", path: "docs/a.md" } }]);
    expect(t.editors.text("e1")).toBe("# a\n");
    expect(t.ui.getState().pendingEditorFocus).toBe("e1");
    expect(t.errors).toEqual([]);
  });

  it("読み取り専用で開くと部品を読み取り専用で作る", async () => {
    const t = setup({
      open_document: () => ({ kind: "opened", editorId: "e1", text: "x", lineEnding: "crlf", readOnly: "hard-link" }),
    });

    await t.controller.openDocument("p1", "a.md");

    expect(t.surface().options).toMatchObject({ readOnly: true, lineEnding: "crlf" });
  });

  it("開いていれば部品を作らずにフォーカスだけ求める", async () => {
    const t = setup({ open_document: () => ({ kind: "already-open", editorId: "e9" }) });

    await t.controller.openDocument("p1", "a.md");

    expect(t.engine.created).toEqual([]);
    expect(t.ui.getState().pendingEditorFocus).toBe("e9");
  });

  it("開けない理由はツリーの知らせに出す", async () => {
    const cases: [FakeHandlers["open_document"], string][] = [
      [() => ({ kind: "not-allowed", reason: "too-large" }), "1 MiB を超えるファイルはエディタで開けません"],
      [() => ({ kind: "not-allowed", reason: "link" }), "シンボリックリンクはエディタで開けません"],
      [() => ({ kind: "not-found" }), "「a.md」が見つかりません"],
      [() => ({ kind: "failed", message: "ファイルを開けませんでした" }), "ファイルを開けませんでした"],
    ];
    for (const [reply, message] of cases) {
      const t = setup({ open_document: reply });
      await t.controller.openDocument("p1", "docs/a.md");
      expect(t.ui.getState().treeNotice).toMatchObject({ message, projectId: "p1" });
      expect(t.engine.created).toEqual([]);
    }
  });

  it("部品を読み込めなければ空のタブを閉じる", async () => {
    const t = setup();
    const broken = new EditorManager({
      shortcuts: () => DEFAULT_SHORTCUT_TABLE,
      onDirtyChange: () => {},
      onSave: () => {},
      fontSize: 15,
      vim: false,
      loadEngine: async () => {
        throw new Error("読めない");
      },
      reportError: () => {},
    });
    const controller = createEditorController({
      ipc: createIpc(t.fake.transport),
      app: t.app,
      ui: t.ui,
      editors: () => broken,
      reportError: () => {},
    });

    await controller.openDocument("p1", "a.md");

    expect(t.fake.calls.at(-1)).toEqual({ command: "close_editor", args: { editorId: "e1", discard: true } });
    expect(t.ui.getState().pendingEditorFocus).toBeNull();
  });
});

describe("editorController(未保存と保存。AC-E04・E05)", () => {
  it("打つと未保存を変わり目だけ送り_保存すると_save_document_を呼ぶ", async () => {
    const t = setup();
    await t.controller.openDocument("p1", "a.md");

    t.surface().type("# b\n");
    t.surface().type("# bc\n");
    await flush();
    t.surface().pressSave();
    await flush();

    expect(t.fake.calls.slice(1)).toEqual([
      { command: "set_document_dirty", args: { editorId: "e1", dirty: true } },
      { command: "save_document", args: { editorId: "e1", text: "# bc\n", mode: "normal" } },
      // 保存した文書を基準にし直したので未保存が消える
      { command: "set_document_dirty", args: { editorId: "e1", dirty: false } },
    ]);
    expect(t.editors.isDirty("e1")).toBe(false);
  });

  it("保存のあいだに打った分は未保存のまま送り直す", async () => {
    const pending: { finish?: () => void } = {};
    const t = setup({
      save_document: () =>
        new Promise((resolve) => {
          pending.finish = () => resolve({ kind: "saved" });
        }),
    });
    await t.controller.openDocument("p1", "a.md");
    t.surface().type("1");
    const saving = t.controller.save("e1");
    // 保存の途中で押しても 2 回は送らない
    await t.controller.save("e1");
    t.surface().type("12");
    await flush();
    pending.finish?.();
    await saving;

    expect(t.commands().filter((c) => c === "save_document")).toHaveLength(1);
    expect(t.fake.calls.at(-1)).toEqual({ command: "set_document_dirty", args: { editorId: "e1", dirty: true } });
    expect(t.editors.isDirty("e1")).toBe(true);
  });

  it("changed_outside_で衝突の確認を出し_上書き保存は_mode_overwrite_で呼ぶ", async () => {
    const replies = [{ kind: "changed-outside" } as const, { kind: "saved" } as const];
    const t = setup({ save_document: () => replies.shift() ?? { kind: "saved" } });
    await t.controller.openDocument("p1", "a.md");
    t.surface().type("x");

    await t.controller.save("e1");
    expect(t.ui.getState().saveConflict).toEqual({ editorId: "e1", kind: "changed" });

    await t.controller.resolveSaveConflict("overwrite");

    expect(t.ui.getState().saveConflict).toBeNull();
    const saves = t.fake.calls.filter((c) => c.command === "save_document").map((c) => c.args);
    expect(saves).toEqual([
      { editorId: "e1", text: "x", mode: "normal" },
      { editorId: "e1", text: "x", mode: "overwrite" },
    ]);
  });

  it("衝突の確認のキャンセルでは何も呼ばずエディタにフォーカスを戻す", async () => {
    const t = setup({ save_document: () => ({ kind: "changed-outside" }) });
    await t.controller.openDocument("p1", "a.md");
    t.ui.getState().consumeEditorFocus("e1");
    await t.controller.save("e1");
    const before = t.fake.calls.length;

    await t.controller.resolveSaveConflict("cancel");

    expect(t.fake.calls.length).toBe(before);
    expect(t.ui.getState().pendingEditorFocus).toBe("e1");
  });

  it("読み直すは未保存でも_外の版が手元と同じでも本文を受け取って置き換え_当てた版を伝える", async () => {
    const t = setup({
      save_document: () => ({ kind: "changed-outside" }),
      check_document: () => ({ kind: "changed", text: "外の文書", lineEnding: "lf", token: 4 }),
    });
    await t.controller.openDocument("p1", "a.md");
    t.surface().type("打った");
    await t.controller.save("e1");

    await t.controller.resolveSaveConflict("reload");

    // always: 外の版が手元と同じ(unchanged になる)ときも本文を受け取る
    expect(t.fake.calls.filter((c) => c.command === "check_document").map((c) => c.args)).toEqual([
      { editorId: "e1", withText: "always" },
    ]);
    expect(t.editors.text("e1")).toBe("外の文書");
    expect(t.editors.isDirty("e1")).toBe(false);
    expect(t.commands().slice(-2)).toEqual(["set_document_dirty", "adopt_document"]);
    expect(t.fake.calls.at(-1)).toEqual({ command: "adopt_document", args: { editorId: "e1", token: 4 } });
  });

  it("消えていたら作るか聞き_作るは_mode_create_で呼ぶ", async () => {
    const replies = [{ kind: "missing" } as const, { kind: "saved" } as const];
    const t = setup({ save_document: () => replies.shift() ?? { kind: "saved" } });
    await t.controller.openDocument("p1", "a.md");

    await t.controller.save("e1");
    expect(t.ui.getState().saveConflict).toEqual({ editorId: "e1", kind: "missing" });
    await t.controller.resolveSaveConflict("create");

    expect(t.fake.calls.filter((c) => c.command === "save_document").map((c) => c.args)).toMatchObject([
      { mode: "normal" },
      { mode: "create" },
    ]);
    // 保存で印が変わらなかったので、今の印を 1 回送る(Rust は保存で印を替えない)
    expect(t.fake.calls.at(-1)).toEqual({ command: "set_document_dirty", args: { editorId: "e1", dirty: false } });
  });

  it("保存できない理由はエディタの帯に出す", async () => {
    const cases: [FakeHandlers["save_document"], string][] = [
      [() => ({ kind: "read-only" }), "読み取り専用のため保存できません"],
      [() => ({ kind: "too-large" }), "1 MiB を超えるため保存できません"],
      [() => ({ kind: "failed", message: "保存できませんでした" }), "保存できませんでした"],
    ];
    for (const [reply, message] of cases) {
      const t = setup({ save_document: reply });
      await t.controller.openDocument("p1", "a.md");
      await t.controller.save("e1");
      expect(t.ui.getState().editorBanners.get("e1")).toEqual({ kind: "failed", message });
    }
  });
});

describe("editorController(閉じる。AC-E07)", () => {
  it("未保存のタブの閉じるで確認を開き_保存しないで_discard_を真にして閉じる", async () => {
    const replies = [{ kind: "needs-confirmation" } as const, { kind: "closed" } as const];
    const t = setup({ close_editor: () => replies.shift() ?? { kind: "closed" } });

    await t.controller.requestClose("e1");
    expect(t.ui.getState().unsavedClose).toEqual({ editorId: "e1" });
    await t.controller.resolveUnsavedClose("discard");

    expect(t.ui.getState().unsavedClose).toBeNull();
    expect(t.fake.calls.map((c) => c.args)).toEqual([
      { editorId: "e1", discard: false },
      { editorId: "e1", discard: true },
    ]);
  });

  it("editor_close_requested_で閉じる確認を出す", () => {
    const t = setup();

    t.controller.handleCloseRequested("e2");

    expect(t.ui.getState().unsavedClose).toEqual({ editorId: "e2" });
  });

  it("閉じる確認の保存は保存できたら閉じ_衝突を挟んでも上書き保存のあと閉じる", async () => {
    const replies = [{ kind: "changed-outside" } as const, { kind: "saved" } as const];
    const t = setup({ save_document: () => replies.shift() ?? { kind: "saved" } });
    await t.controller.openDocument("p1", "a.md");
    t.surface().type("x");
    t.ui.getState().openUnsavedClose("e1");

    await t.controller.resolveUnsavedClose("save");
    expect(t.commands()).not.toContain("close_editor");
    await t.controller.resolveSaveConflict("overwrite");

    expect(t.fake.calls.at(-1)).toEqual({ command: "close_editor", args: { editorId: "e1", discard: false } });
  });

  it("閉じる確認のキャンセルは何も呼ばずエディタにフォーカスを戻す", async () => {
    const t = setup();
    t.ui.getState().openUnsavedClose("e1");

    await t.controller.resolveUnsavedClose("cancel");

    expect(t.fake.calls).toEqual([]);
    expect(t.ui.getState().pendingEditorFocus).toBe("e1");
  });
});

describe("editorController(外での変更。AC-E06)", () => {
  it("tree_changed_で親フォルダが変わった開いている文書を確かめる", async () => {
    const t = setup({ check_document: () => ({ kind: "changed", text: "新しい", lineEnding: "lf", token: 1 }) });
    await t.controller.openDocument("p1", "docs/a.md");
    t.app.getState().replaceSnapshot(withEditors([tab("e1", "docs/a.md")]));

    t.controller.handleTreeChange({ projectId: "p1", dirs: ["src"], fullReload: false });
    t.controller.handleTreeChange({ projectId: "p2", dirs: ["docs"], fullReload: false });
    await flush();
    expect(t.commands()).not.toContain("check_document");

    t.controller.handleTreeChange({ projectId: "p1", dirs: ["docs"], fullReload: false });
    await flush();

    expect(t.fake.calls.slice(-2)).toEqual([
      { command: "check_document", args: { editorId: "e1", withText: "if-changed" } },
      { command: "adopt_document", args: { editorId: "e1", token: 1 } },
    ]);
    expect(t.editors.text("e1")).toBe("新しい");
  });

  it("全部読み直しでも確かめる", async () => {
    const t = setup();
    await t.controller.openDocument("p1", "a.md");
    t.app.getState().replaceSnapshot(withEditors([tab("e1", "a.md")]));

    t.controller.handleTreeChange({ projectId: "p1", dirs: [], fullReload: true });
    await flush();

    expect(t.commands()).toContain("check_document");
  });

  it("未保存なら読み直さず外の変更の印を付ける", async () => {
    const t = setup({ check_document: () => ({ kind: "changed", text: null, lineEnding: "lf", token: null }) });
    await t.controller.openDocument("p1", "a.md");
    t.surface().type("打った");

    await t.controller.check("e1");

    expect(t.fake.calls.at(-1)).toEqual({ command: "check_document", args: { editorId: "e1", withText: "never" } });
    expect(t.editors.text("e1")).toBe("打った");
    expect(t.ui.getState().editorBanners.get("e1")).toEqual({ kind: "changed" });
  });

  it("消えていれば見つからない印_変わっていなければ印を消す", async () => {
    const replies = [{ kind: "missing" } as const, { kind: "unchanged" } as const];
    const t = setup({ check_document: () => replies.shift() ?? { kind: "unchanged" } });
    await t.controller.openDocument("p1", "a.md");

    await t.controller.check("e1");
    expect(t.ui.getState().editorBanners.get("e1")).toEqual({ kind: "missing" });
    await t.controller.check("e1");
    expect(t.ui.getState().editorBanners.has("e1")).toBe(false);
  });

  it("前面に戻ったときは写しの文書をすべて確かめる", async () => {
    const t = setup();
    await t.controller.openDocument("p1", "a.md");
    t.app.getState().replaceSnapshot(withEditors([tab("e1", "a.md"), tab("e2", "b.md")]));

    t.controller.checkAll();
    await flush();

    // 部品のある e1 だけ(e2 は部品が無いので確かめない)
    expect(t.fake.calls.filter((c) => c.command === "check_document").map((c) => c.args)).toEqual([
      { editorId: "e1", withText: "if-changed" },
    ]);
  });

  it("応答を待つあいだに打ち始めたら本文を捨て_版を当てたと伝えない", async () => {
    const reply: { finish?: () => void } = {};
    const t = setup({
      check_document: () =>
        new Promise((resolve) => {
          reply.finish = () => resolve({ kind: "changed", text: "外の文書", lineEnding: "lf", token: 3 });
        }),
    });
    await t.controller.openDocument("p1", "a.md");

    const checking = t.controller.check("e1");
    await flush();
    t.surface().type("打った");
    reply.finish?.();
    await checking;

    expect(t.editors.text("e1")).toBe("打った");
    expect(t.commands()).not.toContain("adopt_document");
    expect(t.ui.getState().editorBanners.get("e1")).toEqual({ kind: "changed" });
  });

  it("あとから始めた確認の応答が先に届いたら_古い応答は当てない", async () => {
    const replies: ((text: string, token: number) => void)[] = [];
    const t = setup({
      check_document: () =>
        new Promise((resolve) => {
          replies.push((text, token) => resolve({ kind: "changed", text, lineEnding: "lf", token }));
        }),
    });
    await t.controller.openDocument("p1", "a.md");

    const first = t.controller.check("e1");
    const second = t.controller.check("e1");
    await flush();
    replies[1]?.("新しい版", 2);
    await second;
    replies[0]?.("古い版", 1);
    await first;

    expect(t.editors.text("e1")).toBe("新しい版");
    expect(t.fake.calls.filter((c) => c.command === "adopt_document").map((c) => c.args)).toEqual([
      { editorId: "e1", token: 2 },
    ]);
  });

  it("保存中のエディタは確かめを飛ばし_保存が終わってから_1_回確かめる", async () => {
    const pending: { finish?: () => void } = {};
    const t = setup({
      save_document: () =>
        new Promise((resolve) => {
          pending.finish = () => resolve({ kind: "saved" });
        }),
    });
    await t.controller.openDocument("p1", "docs/a.md");
    t.app.getState().replaceSnapshot(withEditors([tab("e1", "docs/a.md")]));
    t.surface().type("x");
    const saving = t.controller.save("e1");
    await flush();

    // 自分の保存の改名で届いた tree-changed(保存の応答より先)
    t.controller.handleTreeChange({ projectId: "p1", dirs: ["docs"], fullReload: false });
    t.controller.checkAll();
    await flush();
    expect(t.commands()).not.toContain("check_document");

    pending.finish?.();
    await saving;
    await flush();

    expect(t.fake.calls.filter((c) => c.command === "check_document").map((c) => c.args)).toEqual([
      { editorId: "e1", withText: "if-changed" },
    ]);
  });
});

describe("syncEditors(写しへの追従)", () => {
  it("消えたタブの部品を破棄し_文字の大きさ_vim_読み取り専用を当て_表示に戻したタブを確かめる", async () => {
    const t = setup();
    await t.controller.openDocument("p1", "a.md");
    t.ui.getState().setEditorView("e1", "edit");
    t.app.getState().replaceSnapshot(withEditors([tab("e1", "a.md")]));
    syncEditors({ app: t.app, ui: t.ui, editors: t.editors, controller: t.controller });
    const checks = () => t.fake.calls.filter((c) => c.command === "check_document").length;
    expect(checks()).toBe(0);

    const base = withEditors([tab("e1", "a.md", { readOnly: "not-writable" })], null);
    t.app.getState().replaceSnapshot({
      ...base,
      fontSizes: { ...base.fontSizes, terminal: { ...base.fontSizes.terminal, value: 18 } },
      editor: { vim: true },
    });
    expect(t.surface().fontSizeCalls).toEqual([18]);
    expect(t.surface().vim).toBe(true);
    expect(t.surface().readOnly).toBe(true);

    // ターミナルのタブから e1 に戻した
    t.app.getState().replaceSnapshot(withEditors([tab("e1", "a.md")], "e1"));
    await flush();
    expect(checks()).toBe(1);

    t.app.getState().replaceSnapshot(withEditors([]));
    expect(t.editors.has("e1")).toBe(false);
    expect(t.surface().disposeCount).toBe(1);
    expect(t.ui.getState().editorViews.size).toBe(0);
  });
});

describe("editorController(Vim と表示の切替)", () => {
  it("vim_の切替は_set_editor_vim_を呼び_表示の切替は_uiStore_に持つ", async () => {
    const t = setup();

    await t.controller.setVim(true);
    t.controller.setView("e1", "preview");

    expect(t.fake.calls).toEqual([{ command: "set_editor_vim", args: { enabled: true } }]);
    expect(t.ui.getState().editorViews.get("e1")).toBe("preview");
  });
});
