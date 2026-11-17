// ツリーの表示(T43b): 仮想スクロール、開閉、ルートの行(ヘッダ)、監視の反映(tree-changed)。
//
// 偽の Rust 側(fixtures/tauriMock.ts)のフォルダごとの応答(listings)を置いて画面を開き、行の並び・開閉・選択・
// 呼ばれたコマンドを見る。外からの変更は、応答を置き換えてから tree-changed を流す(本物の Rust と同じ順)。

import { expect, type Locator, type Page, test } from "@playwright/test";

import type { AppSnapshot } from "../src/ipc/generated/AppSnapshot";
import { interfaceMetrics } from "../src/logic/appearance/interfaceMetrics";
import type { EntryKind } from "../src/ipc/generated/EntryKind";
import {
  emitEvent,
  installTauriMock,
  listed,
  listingKey,
  type MockListing,
  projectsSnapshot,
  readCallArgs,
  readRecordedCalls,
  type SampleProject,
  setMockListing,
} from "./fixtures/tauriMock";

const APP: SampleProject = { id: "p-app", name: "app", color: "green", displayPath: "~/dev/app" };
const LOG: SampleProject = { id: "p-log", name: "ribbitlog", color: "orange", displayPath: "~/dev/ribbitlog" };
const GONE: SampleProject = { id: "p-gone", name: "gone", color: "purple", displayPath: "~/dev/gone", missing: true };

type Entries = ReadonlyArray<readonly [string, EntryKind]>;

const ROOT_ENTRIES: Entries = [
  ["docs", "directory"],
  ["src", "directory"],
  ["linked", "directoryLink"],
  ["README.md", "file"],
  ["a b.ts", "file"],
];
const ROOT = listed(ROOT_ENTRIES);
const SRC = listed([
  ["lib", "directory"],
  ["main.ts", "file"],
]);
const LIB = listed([["util.ts", "file"]]);

/** APP のフォルダの応答の表。 */
function appListings(extra: Record<string, MockListing> = {}): Record<string, MockListing> {
  return {
    [listingKey(APP.id, "")]: ROOT,
    [listingKey(APP.id, "src")]: SRC,
    [listingKey(APP.id, "src/lib")]: LIB,
    ...extra,
  };
}

/** 画面のコンソールのエラーと、捕まえられなかった例外を集める。 */
function collectPageErrors(page: Page): string[] {
  const errors: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });
  page.on("pageerror", (error) => errors.push(error.message));
  return errors;
}

/** 写しとフォルダの応答を渡して画面を開き、準備完了まで待つ。 */
async function open(
  page: Page,
  snapshot: AppSnapshot = projectsSnapshot([APP]),
  listings: Record<string, MockListing> = appListings(),
): Promise<void> {
  await installTauriMock(page, { get_app_snapshot: { ok: snapshot } }, listings);
  await page.goto("/");
  await expect
    .poll(async () => (await readRecordedCalls(page)).map((call) => call.command))
    .toContain("notify_app_ready");
}

const fileTree = (page: Page) => page.getByTestId("file-tree");
const scroller = (page: Page) => page.getByTestId("tree-scroller");
const rows = (page: Page) => page.getByTestId("tree-row");
/** パスで行を指す(テストのパスは CSS の文字列として JSON の書き方で足りるものだけ)。 */
const row = (page: Page, path: string) => page.locator(`[data-testid="tree-row"][data-path=${JSON.stringify(path)}]`);
const toggle = (page: Page, path: string) => row(page, path).getByTestId("tree-row-toggle");

/** 描かれている行のパス(上から順)。 */
async function rowPaths(page: Page): Promise<string[]> {
  return rows(page).evaluateAll((elements) => elements.map((el) => el.getAttribute("data-path") ?? ""));
}

/** 行の選択の塗り(角の丸い内側の要素の地の色)。 */
async function selectionColor(locator: Locator): Promise<string> {
  return locator.locator(".tree-row__inner").evaluate((el) => getComputedStyle(el).backgroundColor);
}

/** フォルダの中身の問い合わせのパス(届いた順)。 */
async function listedPaths(page: Page, projectId: string): Promise<string[]> {
  return (await readCallArgs(page, "list_directory")).filter((a) => a.projectId === projectId).map((a) => a.path);
}

/** n 件のファイル。名前は file-000001.txt から。 */
function manyFiles(count: number): Entries {
  return Array.from({ length: count }, (_, i) => [`file-${String(i + 1).padStart(6, "0")}.txt`, "file"] as const);
}

test.describe("ツリーの表示(FR-013)", () => {
  test("選択中のプロジェクトのルートの直下が並ぶ", async ({ page }) => {
    const errors = collectPageErrors(page);
    await open(page);

    await expect.poll(() => rowPaths(page)).toEqual(["docs", "src", "linked", "README.md", "a b.ts"]);
    await expect(rows(page)).toHaveText(["docs", "src", "linked", "README.md", "a b.ts"]);
    expect(await listedPaths(page, APP.id)).toEqual([""]);
    // WAI-ARIA の tree。ヘッダがルート(1 段目)、直下は 2 段目。
    await expect(fileTree(page)).toHaveAttribute("role", "tree");
    await expect(page.getByTestId("tree-header")).toHaveText("app");
    const header = page.getByRole("treeitem", { name: "app" });
    await expect(header).toHaveAttribute("aria-level", "1");
    await expect(header).toHaveAttribute("aria-expanded", "true");
    await expect(row(page, "src")).toHaveAttribute("aria-level", "2");
    await expect(row(page, "src")).toHaveAttribute("aria-expanded", "false");
    await expect(row(page, "README.md")).not.toHaveAttribute("aria-expanded", /.*/);
    expect(errors).toEqual([]);
  });

  test("フォルダを展開すると中身を求めるコマンドが呼ばれ子が並ぶ", async ({ page }) => {
    await open(page);
    await expect(row(page, "src")).toBeVisible();

    await toggle(page, "src").click();

    await expect.poll(() => listedPaths(page, APP.id)).toEqual(["", "src"]);
    await expect
      .poll(() => rowPaths(page))
      .toEqual(["docs", "src", "src/lib", "src/main.ts", "linked", "README.md", "a b.ts"]);
    await expect(row(page, "src")).toHaveAttribute("aria-expanded", "true");
    await expect(row(page, "src/main.ts")).toHaveAttribute("aria-level", "3");
    // 子は親より 1 段ぶん右に字下げされる。
    const parentX = (await row(page, "src").locator(".tree-row__name").boundingBox())?.x ?? 0;
    const childX = (await row(page, "src/main.ts").locator(".tree-row__name").boundingBox())?.x ?? 0;
    expect(childX).toBeGreaterThan(parentX);

    // 2 段目も開ける。閉じると子は消え、開き直すと読み直さずに戻る。
    await toggle(page, "src/lib").click();
    await expect(row(page, "src/lib/util.ts")).toBeVisible();
    await toggle(page, "src").click();
    await expect.poll(() => rowPaths(page)).toEqual(["docs", "src", "linked", "README.md", "a b.ts"]);
    await toggle(page, "src").click();
    await expect(row(page, "src/lib/util.ts")).toBeVisible();
    expect(await listedPaths(page, APP.id)).toEqual(["", "src", "src/lib"]);
  });

  test("フォルダへのリンクには開閉の印が無い", async ({ page }) => {
    await open(page);
    await expect(row(page, "linked")).toBeVisible();

    await expect(toggle(page, "src")).toHaveCount(1);
    await expect(toggle(page, "linked")).toHaveCount(0);
    await expect(row(page, "linked")).not.toHaveAttribute("aria-expanded", /.*/);
    // ダブルクリックしても開かない(中身を求めない)。
    await row(page, "linked").dblclick();
    expect(await listedPaths(page, APP.id)).toEqual([""]);
  });

  test("html_のような名前もそのまま文字で出る", async ({ page }) => {
    const errors = collectPageErrors(page);
    const names = ['<img src=x onerror="window.__pwned=1">.ts', "<b>bold</b>", "&amp; café 日本語.md"];
    await open(page, projectsSnapshot([APP]), {
      [listingKey(APP.id, "")]: listed(names.map((name) => [name, "file"] as const)),
    });

    await expect(rows(page)).toHaveText(names);
    await expect(fileTree(page).locator("img, b")).toHaveCount(0);
    expect(await page.evaluate(() => (window as unknown as { __pwned?: number }).__pwned)).toBeUndefined();
    expect(errors).toEqual([]);
  });

  test("読み込み中はその旨が出て_届くと行に替わる", async ({ page }) => {
    await open(page, projectsSnapshot([APP]), appListings({ [listingKey(APP.id, "")]: "pending" }));

    await expect(page.getByTestId("tree-placeholder")).toHaveText("読み込み中…");
    await expect(fileTree(page)).toHaveAttribute("aria-busy", "true");
    await setMockListing(page, listingKey(APP.id, ""), ROOT);
    await expect(rows(page)).toHaveCount(5);
    await expect(page.getByTestId("tree-placeholder")).toHaveCount(0);

    // フォルダの読み込み中は、その行に読み込み中の印が出る。
    await setMockListing(page, listingKey(APP.id, "src"), "pending");
    await toggle(page, "src").click();
    await expect(row(page, "src")).toHaveAttribute("aria-busy", "true");
    await expect(row(page, "src").getByTestId("tree-row-spinner")).toBeVisible();
    await setMockListing(page, listingKey(APP.id, "src"), SRC);
    await expect(row(page, "src/main.ts")).toBeVisible();
    await expect(row(page, "src").getByTestId("tree-row-spinner")).toHaveCount(0);
  });

  test("空のフォルダはその旨が出る", async ({ page }) => {
    await open(page, projectsSnapshot([APP]), { [listingKey(APP.id, "")]: listed([]) });

    await expect(page.getByTestId("tree-placeholder")).toHaveText("このフォルダは空です");
    await expect(rows(page)).toHaveCount(0);
  });
});

test.describe("選択と開閉の操作(FR-018、【仮定】1・6)", () => {
  test("ヘッダを選ぶとルートが選ばれる", async ({ page }) => {
    await open(page);
    const header = page.getByRole("treeitem", { name: "app" });
    await expect(row(page, "src")).toBeVisible();

    await header.click();
    await expect(header).toHaveAttribute("aria-selected", "true");
    await expect(rows(page).and(page.locator('[aria-selected="true"]'))).toHaveCount(0);

    await row(page, "README.md").click();
    await expect(row(page, "README.md")).toHaveAttribute("aria-selected", "true");
    await expect(header).toHaveAttribute("aria-selected", "false");
    await expect(row(page, "src")).toHaveAttribute("aria-selected", "false");
    // 開閉の印のクリックは開閉だけで、選択を変えない。
    await toggle(page, "src").click();
    await expect(row(page, "src/main.ts")).toBeVisible();
    await expect(row(page, "README.md")).toHaveAttribute("aria-selected", "true");
    // 行の無い余白をクリックすると選択が外れる。
    const box = await scroller(page).boundingBox();
    if (box === null) throw new Error("スクロールの領域が見えない");
    await page.mouse.click(box.x + 40, box.y + box.height - 10);
    await expect(row(page, "README.md")).toHaveAttribute("aria-selected", "false");
  });

  test("ファイルをダブルクリックすると開くのコマンドが呼ばれる", async ({ page }) => {
    await open(page);

    await row(page, "a b.ts").dblclick();

    await expect.poll(() => readCallArgs(page, "open_entry")).toEqual([{ projectId: APP.id, path: "a b.ts" }]);
    await expect(row(page, "a b.ts")).toHaveAttribute("aria-selected", "true");
  });

  test("フォルダをダブルクリックすると開閉する", async ({ page }) => {
    await open(page);

    await row(page, "src").dblclick();
    await expect(row(page, "src")).toHaveAttribute("aria-expanded", "true");
    await expect(row(page, "src/main.ts")).toBeVisible();

    await row(page, "src").dblclick();
    await expect(row(page, "src")).toHaveAttribute("aria-expanded", "false");
    await expect(row(page, "src/main.ts")).toHaveCount(0);
    expect(await readCallArgs(page, "open_entry")).toEqual([]);
  });

  test("tab_でツリーに入れて_フォーカスの有無で選択の見た目が変わる", async ({ page }) => {
    await open(page);
    await row(page, "README.md").click();
    await expect(fileTree(page)).toBeFocused();
    await expect(fileTree(page)).toHaveAttribute("data-focused", "true");
    const focusedColor = await selectionColor(row(page, "README.md"));

    // カードへフォーカスを移すと、選択はグレーの塗りになる。
    await page.getByTestId("project-card").first().focus();
    await expect(fileTree(page)).toHaveAttribute("data-focused", "false");
    const unfocusedColor = await selectionColor(row(page, "README.md"));
    expect(unfocusedColor).not.toBe(focusedColor);
    expect(unfocusedColor).not.toBe("rgba(0, 0, 0, 0)");

    // Tab でツリーへ戻れる(macOS の既定の Tab がボタンに止まるかどうかに依らず、数回で着く)。
    for (let i = 0; i < 6; i++) {
      await page.keyboard.press("Tab");
      if (await fileTree(page).evaluate((el) => el === document.activeElement)) break;
    }
    await expect(fileTree(page)).toBeFocused();
    await expect(fileTree(page)).toHaveAttribute("data-focused", "true");
    expect(await selectionColor(row(page, "README.md"))).toBe(focusedColor);
  });
});

test.describe("監視の反映(FR-014、AC-T02)", () => {
  test("tree-changed_で_src_に_new.txt_が現れる", async ({ page }) => {
    await open(page);
    await toggle(page, "src").click();
    await expect(row(page, "src/main.ts")).toBeVisible();

    await setMockListing(
      page,
      listingKey(APP.id, "src"),
      listed([
        ["lib", "directory"],
        ["main.ts", "file"],
        ["new.txt", "file"],
      ]),
    );
    await emitEvent(page, "tree-changed", { projectId: APP.id, dirs: ["src"], fullReload: false });

    await expect(row(page, "src/new.txt")).toBeVisible({ timeout: 2_000 });
    await expect(row(page, "src/new.txt")).toHaveAttribute("aria-level", "3");
  });

  test("削除と改名も反映される", async ({ page }) => {
    await open(page);
    await expect(row(page, "README.md")).toBeVisible();

    // README.md を消し、a b.ts を c.ts に改名した。
    await setMockListing(
      page,
      listingKey(APP.id, ""),
      listed([
        ["docs", "directory"],
        ["src", "directory"],
        ["linked", "directoryLink"],
        ["c.ts", "file"],
      ]),
    );
    await emitEvent(page, "tree-changed", { projectId: APP.id, dirs: [""], fullReload: false });

    await expect.poll(() => rowPaths(page)).toEqual(["docs", "src", "linked", "c.ts"]);
  });

  test("隠しファイルを表示の切替でコマンドが呼ばれ返った項目が出る", async ({ page }) => {
    await open(page);
    const button = page.getByRole("button", { name: "隠しファイルを表示" });
    await expect(button).toHaveAttribute("aria-pressed", "false");
    await expect(row(page, ".env")).toHaveCount(0);

    // 切替のあとは隠しファイルを含めて返す(本物の Rust は保存した設定で除外を変える)。
    await setMockListing(page, listingKey(APP.id, ""), listed([[".env", "file"], ...ROOT_ENTRIES]));
    await button.click();

    await expect.poll(() => readCallArgs(page, "set_show_hidden")).toEqual([{ projectId: APP.id, showHidden: true }]);
    await expect(row(page, ".env")).toBeVisible();
    // 押した状態は写しの値で出す。
    const shown = projectsSnapshot([APP]);
    await emitEvent(page, "app-snapshot", {
      ...shown,
      projects: shown.projects.map((p) => ({ ...p, showHidden: true })),
    });
    await expect(button).toHaveAttribute("aria-pressed", "true");
  });

  test("すべて折りたたむで開いたフォルダが閉じる", async ({ page }) => {
    await open(page);
    await toggle(page, "src").click();
    await toggle(page, "src/lib").click();
    await expect(row(page, "src/lib/util.ts")).toBeVisible();

    await page.getByRole("button", { name: "すべて折りたたむ" }).click();

    await expect.poll(() => rowPaths(page)).toEqual(["docs", "src", "linked", "README.md", "a b.ts"]);
  });
});

test.describe("仮想スクロール(NFR-005、ADR-007 の (c))", () => {
  /** 見える行の数(スクロールの領域の高さを行の高さで割った数)。 */
  async function visibleRowCount(page: Page): Promise<number> {
    const { height, rowHeight } = await scroller(page).evaluate((el) => ({
      height: el.clientHeight,
      rowHeight: el.querySelector('[data-testid="tree-row"]')?.getBoundingClientRect().height ?? 0,
    }));
    expect(rowHeight).toBe(22);
    return Math.ceil(height / rowHeight);
  }

  for (const count of [10_000, 100_000]) {
    test(`${count.toLocaleString("en-US")}_行のフォルダを展開しても描かれる行は見える数だけ`, async ({ page }) => {
      const errors = collectPageErrors(page);
      await open(page, projectsSnapshot([APP]), appListings({ [listingKey(APP.id, "big")]: listed(manyFiles(count)) }));
      await setMockListing(page, listingKey(APP.id, ""), listed([
        ["big", "directory"],
        ["README.md", "file"],
      ]));
      await emitEvent(page, "tree-changed", { projectId: APP.id, dirs: [""], fullReload: false });
      await expect(row(page, "big")).toBeVisible();

      await toggle(page, "big").click();
      await expect(row(page, "big/file-000001.txt")).toBeVisible();

      const visible = await visibleRowCount(page);
      const drawn = await rows(page).count();
      // 見える数 + 上下の余分(6 行ずつ)+ 端数の 1 行まで。
      expect(drawn).toBeGreaterThanOrEqual(visible);
      expect(drawn).toBeLessThanOrEqual(visible + 13);
      // スクロールできる高さは全部の行ぶん。
      const scrollHeight = await scroller(page).evaluate((el) => el.scrollHeight);
      expect(scrollHeight).toBe((count + 2) * 22);
      expect(errors).toEqual([]);
    });
  }

  test("スクロールすると見える行が入れ替わり_位置が飛ばない", async ({ page }) => {
    await open(page, projectsSnapshot([APP]), appListings({ [listingKey(APP.id, "big")]: listed(manyFiles(10_000)) }));
    await setMockListing(page, listingKey(APP.id, ""), listed([["big", "directory"]]));
    await emitEvent(page, "tree-changed", { projectId: APP.id, dirs: [""], fullReload: false });
    await row(page, "big").dblclick();
    await expect(row(page, "big/file-000001.txt")).toBeVisible();

    // 5,000 行目(big が 0 行目なので file-005000.txt)を上端にする。
    const target = 5_000 * 22;
    await scroller(page).evaluate((el, top) => {
      el.scrollTop = top;
    }, target);

    await expect(row(page, "big/file-005000.txt")).toBeVisible();
    await expect(row(page, "big/file-000001.txt")).toHaveCount(0);
    await expect(row(page, "big")).toHaveCount(0);
    // 上端の行はスクロールの領域の上端にある。
    const top = await scroller(page).evaluate((el) => el.getBoundingClientRect().top);
    const rowTop = (await row(page, "big/file-005000.txt").boundingBox())?.y ?? -1;
    expect(Math.round(rowTop - top)).toBe(0);
    const drawn = await rows(page).count();
    expect(drawn).toBeLessThan(60);

    // 外からの変更(フォルダの末尾に 1 件増える)が届いても、位置は変わらない。
    await setMockListing(page, listingKey(APP.id, "big"), listed([...manyFiles(10_000), ["zzz.txt", "file"]]));
    await emitEvent(page, "tree-changed", { projectId: APP.id, dirs: ["big"], fullReload: false });
    await expect.poll(() => scroller(page).evaluate((el) => el.scrollHeight)).toBe(10_002 * 22);
    expect(await scroller(page).evaluate((el) => el.scrollTop)).toBe(target);
    expect(Math.round(((await row(page, "big/file-005000.txt").boundingBox())?.y ?? -1) - top)).toBe(0);

    // 少しずつ下へスクロールしても、行は 1 行ずつ詰まって並ぶ(隙間も重なりも無い)。
    await scroller(page).evaluate((el) => {
      el.scrollTop += 37;
    });
    await expect(row(page, "big/file-005001.txt")).toBeVisible();
    // スクロールの直後は、窓の描き直しが済む前の並びが一瞬見えることがあるので、落ち着いた並びを待って確かめる。
    const gaps = () =>
      rows(page).evaluateAll((elements) => {
        const tops = elements.map((el) => el.getBoundingClientRect().top);
        return [...new Set(tops.slice(1).map((t, i) => Math.round(t - (tops[i] ?? 0))))];
      });
    await expect.poll(gaps).toEqual([22]);
  });
});

test.describe("画面の文字の大きさとツリー(M4.6 の T69b、ADR-013 の 4、AC-F05)", () => {
  /** 画面の大きさ(写しの fontSizes.interface.value)を替えた写し。 */
  const withInterfaceSize = (value: number): AppSnapshot => {
    const base = projectsSnapshot([APP]);
    return { ...base, fontSizes: { ...base.fontSizes, interface: { ...base.fontSizes.interface, value } } };
  };

  /** 10,000 件のフォルダ big を開いた状態にする。 */
  async function openBig(page: Page): Promise<void> {
    await open(page, projectsSnapshot([APP]), appListings({ [listingKey(APP.id, "big")]: listed(manyFiles(10_000)) }));
    await setMockListing(page, listingKey(APP.id, ""), listed([["big", "directory"]]));
    await emitEvent(page, "tree-changed", { projectId: APP.id, dirs: [""], fullReload: false });
    await row(page, "big").dblclick();
    await expect(row(page, "big/file-000001.txt")).toBeVisible();
  }

  /** 描かれている行の上端どうしの間隔(落ち着くまで待って使う)。 */
  const gaps = (page: Page) =>
    rows(page).evaluateAll((elements) => {
      const tops = elements.map((el) => el.getBoundingClientRect().top);
      return [...new Set(tops.slice(1).map((t, i) => Math.round(t - (tops[i] ?? 0))))];
    });

  test("画面の大きさを変えた写しでツリーの行の高さと仮想スクロールがそろう", async ({ page }) => {
    const errors = collectPageErrors(page);
    await openBig(page);
    const iconSize = () =>
      row(page, "big/file-000001.txt")
        .locator(".tree-icon")
        .evaluate((el) => [el.getBoundingClientRect().width, el.getBoundingClientRect().height]);
    const iconBefore = await iconSize();

    for (const size of [18, 11, 13]) {
      const height = interfaceMetrics(size).treeRowHeight;
      await emitEvent(page, "app-snapshot", withInterfaceSize(size));

      // CSS の変数・行の見た目の高さ・スクロールの高さ(窓の計算)が、同じ関数の値でそろう。
      await expect
        .poll(() => page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue("--tree-row-height")))
        .toBe(`${height}px`);
      await expect.poll(() => scroller(page).evaluate((el) => el.scrollHeight)).toBe(10_001 * height);
      await expect
        .poll(() => row(page, "big/file-000001.txt").evaluate((el) => el.getBoundingClientRect().height))
        .toBe(height);
      await expect.poll(() => gaps(page)).toEqual([height]);
      expect(
        await row(page, "big/file-000001.txt").evaluate((el) => getComputedStyle(el).fontSize),
        `${size}`,
      ).toBe(`${interfaceMetrics(size).tokens["--font-size-l"]}px`);
      // アイコンは追従させない(【要確認】5 の推奨)。
      expect(await iconSize(), `${size}`).toEqual(iconBefore);

      // 5,000 行目を上端にすると、その行がスクロールの領域の上端に来る(行の高さの計算が見た目と同じ)。
      await scroller(page).evaluate((el, top) => {
        el.scrollTop = top;
      }, 5_000 * height);
      await expect(row(page, "big/file-005000.txt")).toBeVisible();
      const top = await scroller(page).evaluate((el) => el.getBoundingClientRect().top);
      await expect
        .poll(async () => Math.round(((await row(page, "big/file-005000.txt").boundingBox())?.y ?? -1) - top))
        .toBe(0);
      await expect.poll(() => gaps(page)).toEqual([height]);
      await scroller(page).evaluate((el) => {
        el.scrollTop = 0;
      });
      await expect(row(page, "big/file-000001.txt")).toBeVisible();
    }
    expect(errors).toEqual([]);
  });

  test("画面の大きさを変えても選択中の行が見える位置に来る", async ({ page }) => {
    await openBig(page);
    // 見える範囲のいちばん下の行を選ぶ(大きくすると、行が伸びて見える範囲の外へ押し出される位置)。
    const lastVisible = await scroller(page).evaluate((el) => {
      const bottom = el.getBoundingClientRect().bottom;
      const inside = [...el.querySelectorAll('[data-testid="tree-row"]')].filter(
        (r) => r.getBoundingClientRect().bottom <= bottom,
      );
      return inside.at(-1)?.getAttribute("data-path") ?? "";
    });
    expect(lastVisible).toMatch(/^big\/file-/);
    await row(page, lastVisible).click();
    await expect(row(page, lastVisible)).toHaveAttribute("aria-selected", "true");

    const shownInside = () =>
      page.evaluate((path) => {
        const scrollerElement = document.querySelector('[data-testid="tree-scroller"]');
        const rowElement = document.querySelector(`[data-testid="tree-row"][data-path="${path}"]`);
        if (scrollerElement === null || rowElement === null) return false;
        const outer = scrollerElement.getBoundingClientRect();
        const inner = rowElement.getBoundingClientRect();
        return inner.top >= outer.top - 0.5 && inner.bottom <= outer.bottom + 0.5;
      }, lastVisible);

    for (const size of [18, 11, 16]) {
      await emitEvent(page, "app-snapshot", withInterfaceSize(size));
      await expect
        .poll(() => row(page, lastVisible).evaluate((el) => el.getBoundingClientRect().height))
        .toBe(interfaceMetrics(size).treeRowHeight);
      await expect.poll(shownInside, `${size}`).toBe(true);
      await expect(row(page, lastVisible)).toHaveAttribute("aria-selected", "true");
    }
  });
});

test.describe("画面の文字の大きさの画像(M4.6 の T69b。人が目で確かめる)", () => {
  /** 画像の置き場所(testreport/ は .gitignore 済み)。 */
  const shotPath = (name: string) => {
    const configFile = test.info().config.configFile;
    if (configFile === undefined) throw new Error("Playwright の設定ファイルの場所が分からない");
    return `${configFile.slice(0, configFile.lastIndexOf("/"))}/testreport/e2e/font-sizes/${name}.png`;
  };
  const shots = [
    [11, "light"],
    [13, "light"],
    [18, "light"],
    [11, "dark"],
    [18, "dark"],
  ] as const;

  for (const [size, colorScheme] of shots) {
    test(`画面の大きさ_${size}_の画像を撮る_${colorScheme}`, async ({ page }) => {
      const errors = collectPageErrors(page);
      await page.emulateMedia({ colorScheme });
      const base = projectsSnapshot([{ ...APP, tabs: ["ターミナル 1", "npm run dev", "claude"] }, LOG, GONE]);
      const snapshot: AppSnapshot = {
        ...base,
        fontSizes: { ...base.fontSizes, interface: { ...base.fontSizes.interface, value: size } },
      };
      await open(page, snapshot);
      await toggle(page, "src").click();
      await toggle(page, "src/lib").click();
      await expect(row(page, "src/lib/util.ts")).toBeVisible();
      await row(page, "src/main.ts").click();
      await expect(row(page, "src/main.ts")).toHaveAttribute("aria-selected", "true");
      await expect
        .poll(() => page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue("--font-size-l")))
        .toBe(`${size}px`);
      await page.screenshot({ path: shotPath(`interface-${size}-${colorScheme}`), animations: "disabled" });

      // 名前の入力欄(--tree-input-height)も行の中に収まるか見る。
      await fileTree(page).focus();
      await page.keyboard.press("Enter");
      const input = page.locator("[data-tree-name-input]");
      await expect(input).toBeVisible();
      const [rowBox, inputBox] = await Promise.all([row(page, "src/main.ts").boundingBox(), input.boundingBox()]);
      expect(inputBox?.height).toBe(interfaceMetrics(size).tokens["--tree-input-height"]);
      if (rowBox === null || inputBox === null) throw new Error("行か入力欄の位置が取れない");
      // 入力欄は行の高さの中に収まり、上下の余りの差は 1px まで(中央にそろう)。
      const above = inputBox.y - rowBox.y;
      const below = rowBox.y + rowBox.height - (inputBox.y + inputBox.height);
      expect(above).toBeGreaterThanOrEqual(0);
      expect(below).toBeGreaterThanOrEqual(0);
      expect(Math.abs(above - below)).toBeLessThanOrEqual(1);
      await page.screenshot({ path: shotPath(`interface-${size}-${colorScheme}-rename`), animations: "disabled" });
      expect(errors).toEqual([]);
    });
  }
});

test.describe("プロジェクトの切替と見つからない(FR-002・FR-007)", () => {
  test("プロジェクトを切り替えるとツリーが切り替わる", async ({ page }) => {
    const snapshot = projectsSnapshot([APP, LOG]);
    await open(page, snapshot, {
      ...appListings(),
      [listingKey(LOG.id, "")]: listed([
        ["Cargo.toml", "file"],
        ["crates", "directory"],
      ]),
    });
    await expect.poll(() => rowPaths(page)).toEqual(["docs", "src", "linked", "README.md", "a b.ts"]);

    await emitEvent(page, "app-snapshot", { ...snapshot, selectedProjectId: LOG.id });

    await expect(page.getByTestId("tree-header")).toHaveText("ribbitlog");
    await expect.poll(() => rowPaths(page)).toEqual(["Cargo.toml", "crates"]);
    expect(await listedPaths(page, LOG.id)).toEqual([""]);

    // 戻ると元のツリーに戻る(読み込み済みのフォルダを読み直す)。
    await emitEvent(page, "app-snapshot", snapshot);
    await expect(page.getByTestId("tree-header")).toHaveText("app");
    await expect.poll(() => rowPaths(page)).toEqual(["docs", "src", "linked", "README.md", "a b.ts"]);
  });

  test("ルートの表示用のパスが変わってもツリーの行が残る", async ({ page }) => {
    const snapshot = projectsSnapshot([APP]);
    await open(page, snapshot);
    await toggle(page, "src").click();
    await expect(row(page, "src/main.ts")).toBeVisible();

    // ルートを Finder で改名した(AC-P07): 名前と表示用のパスが変わった写しが届く。
    await emitEvent(page, "app-snapshot", {
      ...snapshot,
      projects: snapshot.projects.map((p) => ({ ...p, name: "app-renamed", displayPath: "~/dev/app-renamed" })),
    });

    await expect(page.getByTestId("tree-header")).toHaveText("app-renamed");
    await expect.poll(() => rowPaths(page)).toEqual(["docs", "src", "src/lib", "src/main.ts", "linked", "README.md", "a b.ts"]);
  });

  test("見つからないプロジェクトはツリーの代わりに案内が出る", async ({ page }) => {
    await open(page, projectsSnapshot([GONE, APP]));

    await expect(page.getByTestId("tree-header")).toHaveText("gone");
    await expect(page.getByTestId("tree-missing")).toContainText("フォルダが見つかりません");
    await expect(fileTree(page)).toHaveCount(0);
    await expect(page.getByRole("button", { name: "隠しファイルを表示" })).toHaveCount(0);
    expect(await readCallArgs(page, "list_directory")).toEqual([]);
  });

  test("プロジェクトが_0_件ならツリーは出ない", async ({ page }) => {
    await open(page, projectsSnapshot([]));

    await expect(page.getByTestId("projects-empty")).toBeVisible();
    await expect(fileTree(page)).toHaveCount(0);
    await expect(page.getByTestId("tree-header")).toHaveCount(0);
  });
});
