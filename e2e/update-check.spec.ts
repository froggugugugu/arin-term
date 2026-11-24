// 新しい版の確認(ADR-018)。偽の Rust 側(fixtures/tauriMock.ts)の get_update_notice の応答と update-changed で知らせを流し、
// 知らせの行・「ダウンロードのページを開く」・閉じる・設定画面の切替を確かめる。問い合わせそのもの(curl)は Rust のテストが見る。

import { expect, type Page, test } from "@playwright/test";

import type { AppSnapshot } from "../src/ipc/generated/AppSnapshot";
import {
  emitEvent,
  installTauriMock,
  readCallArgs,
  readRecordedCalls,
  sampleSnapshot,
  type TauriMockConfig,
} from "./fixtures/tauriMock";

async function open(page: Page, overrides: Partial<TauriMockConfig> = {}, snapshot: AppSnapshot = sampleSnapshot()) {
  await installTauriMock(page, { get_app_snapshot: { ok: snapshot }, ...overrides });
  await page.goto("/");
  await expect
    .poll(async () => (await readRecordedCalls(page)).map((call) => call.command))
    .toContain("notify_app_ready");
}

test("起動時の知らせに新しい版があれば_ページを開くボタンで_open_release_page_を呼び_閉じると消える", async ({ page }) => {
  await open(page, { get_update_notice: { ok: { kind: "available", version: "0.4.0" } } });

  const bar = page.getByTestId("update-notice");
  await expect(bar).toBeVisible();
  await expect(page.getByTestId("update-notice-text")).toHaveText("新しい版 v0.4.0 があります");
  await page.getByTestId("update-notice-action").click();
  await expect.poll(async () => (await readRecordedCalls(page)).filter((c) => c.command === "open_release_page").length).toBe(1);

  await bar.getByRole("button", { name: "アップデートの知らせを閉じる" }).click();
  await expect(bar).toHaveCount(0);
  await expect.poll(async () => (await readRecordedCalls(page)).filter((c) => c.command === "dismiss_update_notice").length).toBe(1);
});

test("知らせが無ければ行を出さず_メニューからの結果_最新です_は_update_changed_で出る_ボタンは無い", async ({ page }) => {
  await open(page);
  await expect(page.getByTestId("update-notice")).toHaveCount(0);

  await emitEvent(page, "update-changed", { kind: "upToDate", version: "0.3.0" });
  await expect(page.getByTestId("update-notice-text")).toHaveText("Arin v0.3.0 は最新です");
  await expect(page.getByTestId("update-notice-action")).toHaveCount(0);

  await emitEvent(page, "update-changed", { kind: "failed", reason: "GitHub に問い合わせられませんでした" });
  await expect(page.getByTestId("update-notice-text")).toHaveText(
    "新しい版を確かめられませんでした: GitHub に問い合わせられませんでした",
  );
});

test("設定画面の_起動時に新しい版を確かめる_は既定でオフ_押すと_set_check_updates_を送り写しで表示が替わる", async ({ page }) => {
  const snapshot = sampleSnapshot();
  await open(page, {}, snapshot);
  await emitEvent(page, "menu-command", "open-settings");
  await expect(page.getByTestId("settings-dialog")).toBeVisible();

  const toggle = page.getByTestId("check-updates-switch");
  await expect(toggle).toHaveAttribute("aria-checked", "false");
  await toggle.click();
  await expect.poll(() => readCallArgs(page, "set_check_updates")).toEqual([{ enabled: true }]);
  await emitEvent(page, "app-snapshot", { ...snapshot, checkUpdates: true });
  await expect(toggle).toHaveAttribute("aria-checked", "true");
});

for (const colorScheme of ["light", "dark"] as const) {
  test(`知らせの行と設定画面の画像を撮る_${colorScheme}`, async ({ page }) => {
    await page.setViewportSize({ width: 960, height: 700 });
    await page.emulateMedia({ colorScheme });
    await open(page, { get_update_notice: { ok: { kind: "available", version: "0.4.0" } } });
    await expect(page.getByTestId("update-notice")).toBeVisible();
    await page.screenshot({ path: `testreport/e2e/update-check/notice-${colorScheme}.png` });
    await emitEvent(page, "menu-command", "open-settings");
    await expect(page.getByTestId("check-updates-row")).toBeVisible();
    await page.screenshot({ path: `testreport/e2e/update-check/settings-${colorScheme}.png` });
  });
}
