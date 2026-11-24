// Markdown のエディタの Mermaid の図と「/」コマンド(ADR-017)。
//
// 本物の mermaid と CodeMirror の補完を WebKit で動かす。偽の Rust 側は写しを送り返さないので、
// コマンドが届いたことを確かめてから、Rust が返す写し(withEditors)を流す(e2e/editor.spec.ts と同じ)。

import { expect, type Page, test } from "@playwright/test";

import {
  emitEvent,
  installTauriMock,
  listed,
  listingKey,
  projectsSnapshot,
  readCallArgs,
  readRecordedCalls,
  sampleEditorTab,
  type SampleProject,
  withEditors,
} from "./fixtures/tauriMock";

const APP: SampleProject = { id: "p-app", name: "app", color: "green", displayPath: "~/dev/app" };
const EDITOR_ID = "editor-1";
const PATH = "README.md";

const DIAGRAM = ["```mermaid", "flowchart TD", "  A[始め] --> B[終わり]", "```"].join("\n");
const BROKEN = ["```mermaid", "flowchart TD", "  A --> --> ((", "```"].join("\n");

function collectPageErrors(page: Page): string[] {
  const errors: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });
  page.on("pageerror", (error) => errors.push(error.message));
  return errors;
}

const panel = (page: Page) => page.getByTestId("editor-panel");
const content = (page: Page) => panel(page).locator(".cm-content");
const preview = (page: Page) => page.getByTestId("editor-preview");
const completions = (page: Page) => page.locator(".cm-tooltip-autocomplete");

/**
 * 候補が出てから、選ぶキーを押せるまで待つ。CodeMirror の補完は、候補が出てから interactionDelay(既定 75 ms)の
 * あいだのキーを受けない(Enter は改行に、↑↓ はカーソルの移動になる)。人の操作では起きない。
 */
async function settleCompletions(page: Page): Promise<void> {
  await expect(completions(page)).toBeVisible();
  await page.waitForTimeout(150);
}

/** 画面を開いて README.md をエディタで開く(既定は並べて表示)。 */
async function openEditor(page: Page, text: string): Promise<void> {
  await installTauriMock(
    page,
    {
      get_app_snapshot: { ok: projectsSnapshot([APP]) },
      open_document: { ok: { kind: "opened", editorId: EDITOR_ID, text, lineEnding: "lf", readOnly: null } },
    },
    { [listingKey(APP.id, "")]: listed([[PATH, "file"]]) },
  );
  await page.goto("/");
  await expect
    .poll(async () => (await readRecordedCalls(page)).map((call) => call.command))
    .toContain("notify_app_ready");
  await page.locator(`[data-testid="tree-row"][data-path=${JSON.stringify(PATH)}]`).dblclick();
  await expect.poll(() => readCallArgs(page, "open_document")).toEqual([{ projectId: APP.id, path: PATH }]);
  await emitEvent(page, "app-snapshot", withEditors(projectsSnapshot([APP]), APP.id, [sampleEditorTab(EDITOR_ID, APP.id, PATH)]));
  await expect(content(page)).toBeVisible();
}

/** 文書の末尾へカーソルを移す。 */
async function toEnd(page: Page): Promise<void> {
  await content(page).click();
  await page.keyboard.press("Meta+ArrowDown");
}

const docText = (page: Page) => content(page).evaluate((el) => (el as HTMLElement).innerText);

test.describe("Mermaid の図(ADR-017)", () => {
  test("mermaid_の塊は_blob_の_img_で出しライトとダークの_2_つを持つ", async ({ page }) => {
    const errors = collectPageErrors(page);
    await openEditor(page, `# 図\n\n${DIAGRAM}\n`);

    const figure = preview(page).getByTestId("md-diagram");
    await expect(figure).toBeVisible({ timeout: 15_000 });
    const img = figure.locator("img");
    await expect(img).toHaveAttribute("src", /^blob:/);
    await expect(img).toHaveAttribute("alt", "Mermaid の図");
    await expect(figure.locator('source[media="(prefers-color-scheme: dark)"]')).toHaveAttribute("srcset", /^blob:/);
    // 画像として読み込めている(幅がある)
    await expect.poll(() => img.evaluate((el) => (el as HTMLImageElement).naturalWidth)).toBeGreaterThan(0);
    // SVG は画面に直接入れない。リンクも作らない
    expect(await preview(page).locator("svg, a[href], script, foreignObject").count()).toBe(0);

    // 図の上に段落を足しても、図は作り直さず出したまま(コードの塊に戻らない)
    const src = await img.getAttribute("src");
    await content(page).getByText("# 図").click();
    await page.keyboard.press("Meta+ArrowRight");
    await page.keyboard.type("\n\n上に足した段落");
    await expect(preview(page).getByText("上に足した段落")).toBeVisible();
    await expect(preview(page).locator(".md-diagram-source")).toHaveCount(0);
    await expect(img).toHaveAttribute("src", src ?? "");
    expect(errors).toEqual([]);
  });

  test("壊れた文では書かれた文と理由を出し_直すと図になる", async ({ page }) => {
    await openEditor(page, `${BROKEN}\n`);

    const error = preview(page).getByTestId("md-diagram-error");
    await expect(error).toBeVisible({ timeout: 15_000 });
    await expect(error).toContainText("図を描けませんでした");
    await expect(preview(page).locator("pre.md-code")).toContainText("A --> --> ((");
    expect(await preview(page).locator("img").count()).toBe(0);

    // 壊れた行を消して正しい行に打ち直す
    await content(page).getByText("A --> --> ((").click();
    await page.keyboard.press("Meta+ArrowRight");
    await page.keyboard.press("Meta+Shift+ArrowLeft");
    await page.keyboard.type("A --> B");
    await expect(preview(page).getByTestId("md-diagram").locator("img")).toHaveAttribute("src", /^blob:/, {
      timeout: 15_000,
    });
    await expect(error).toHaveCount(0);
  });
});

test.describe("「/」コマンド(ADR-017)", () => {
  test("行の先頭の_slash_で候補が出て_見出し_2_を選ぶと_##_になる", async ({ page }) => {
    await openEditor(page, "本文\n");
    await toEnd(page);

    await page.keyboard.type("/");
    await expect(completions(page)).toBeVisible();
    await expect(completions(page).getByRole("option")).toHaveCount(12);

    await page.keyboard.type("h2");
    await expect(completions(page).getByRole("option")).toHaveText([/見出し 2/]);
    await settleCompletions(page);
    await page.keyboard.press("Enter");
    await expect(completions(page)).toHaveCount(0);
    await page.keyboard.type("題");
    expect(await docText(page)).toContain("## 題");
    expect(await docText(page)).not.toContain("/h2");
  });

  test("表と_mermaid_の雛形を差し込みプレビューに出る", async ({ page }) => {
    await openEditor(page, "");
    await content(page).click();

    await page.keyboard.type("/table");
    await expect(completions(page).getByRole("option")).toHaveText([/表/]);
    await settleCompletions(page);
    await page.keyboard.press("Enter");
    // カーソルは最初の列名の前
    await page.keyboard.type("名前");
    await expect(preview(page).locator("table th").first()).toHaveText("名前列 1");

    await page.keyboard.press("Meta+ArrowDown");
    await page.keyboard.press("Enter");
    await page.keyboard.press("Enter");
    await page.keyboard.type("/mermaid");
    await expect(completions(page).getByRole("option")).toHaveText([/フローチャート/, /シーケンス図/]);
    await settleCompletions(page);
    await page.keyboard.press("Enter");
    await expect(preview(page).getByTestId("md-diagram").locator("img")).toHaveAttribute("src", /^blob:/, {
      timeout: 15_000,
    });
  });

  test("文の途中の_slash_とコードの塊の中では候補を出さない_Esc_で閉じる", async ({ page }) => {
    await openEditor(page, "```\ncode\n```\n");

    // コードの塊の中
    await content(page).getByText("code", { exact: true }).click();
    await page.keyboard.press("Meta+ArrowRight");
    await page.keyboard.type(" /");
    await page.waitForTimeout(300);
    await expect(completions(page)).toHaveCount(0);

    // パスのような a/b
    await page.keyboard.press("Meta+ArrowDown");
    await page.keyboard.type("a/b");
    await page.waitForTimeout(300);
    await expect(completions(page)).toHaveCount(0);

    // 空白のあとなら出て、Esc で閉じる(文字はそのまま)
    await page.keyboard.type(" /");
    await settleCompletions(page);
    await page.keyboard.press("Escape");
    await expect(completions(page)).toHaveCount(0);
    expect(await docText(page)).toContain("a/b /");
  });
});

test.describe("「/」コマンドと Vim(ADR-017)", () => {
  test("vim_の挿入モードでも候補を矢印で選び_Enter_で差し込める_ノーマルモードの_slash_は検索", async ({ page }) => {
    await openEditor(page, "本文\n");
    // Rust が返す写し(Vim を有効にした)を流す
    const vimOn = withEditors({ ...projectsSnapshot([APP]), editor: { vim: true } }, APP.id, [
      sampleEditorTab(EDITOR_ID, APP.id, PATH),
    ]);
    await emitEvent(page, "app-snapshot", vimOn);
    await expect(panel(page).locator(".cm-vimMode, .cm-fat-cursor").first()).toBeAttached();
    await content(page).click();

    // ノーマルモードの「/」は Vim の検索で、候補は出ない
    await page.keyboard.press("Escape");
    await page.keyboard.type("/");
    await page.waitForTimeout(300);
    await expect(completions(page)).toHaveCount(0);
    await page.keyboard.press("Escape");

    // 末尾の行で挿入モードにして「/」
    await page.keyboard.press("G");
    await page.keyboard.press("o");
    await page.keyboard.type("/見出し");
    await expect(completions(page).getByRole("option")).toHaveText([/見出し 1/, /見出し 2/, /見出し 3/]);
    await settleCompletions(page);
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Enter");
    await expect(completions(page)).toHaveCount(0);
    await page.keyboard.type("題");
    expect(await docText(page)).toContain("### 題");
  });
});

test.describe("見た目(ADR-017)", () => {
  for (const colorScheme of ["light", "dark"] as const) {
    test(`図と候補の画像を撮る_${colorScheme}`, async ({ page }) => {
      await page.emulateMedia({ colorScheme });
      await openEditor(page, `# 図\n\n${DIAGRAM}\n\n`);
      await expect(preview(page).getByTestId("md-diagram").locator("img")).toHaveAttribute("src", /^blob:/, {
        timeout: 15_000,
      });
      await toEnd(page);
      await page.keyboard.type("/");
      await expect(completions(page)).toBeVisible();
      const configFile = test.info().config.configFile ?? "";
      await page.screenshot({
        path: `${configFile.slice(0, configFile.lastIndexOf("/"))}/testreport/e2e/mermaid-slash/editor-${colorScheme}.png`,
      });
    });
  }
});
