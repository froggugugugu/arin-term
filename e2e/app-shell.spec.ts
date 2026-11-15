// 起動の画面の自動テスト(T11。設計書 §10.3、§11 の画面側の表)。
//
// 偽の Rust 側(fixtures/tauriMock.ts)を差し込んでから画面を開き、
// 起動の手順(起動時の指定の取得 → 写しの取得 → 描画 → 準備完了の通知)が画面と通信の両方で期待どおりかを見る。

import { expect, test, type Page } from "@playwright/test";

import { installTauriMock, readRecordedCalls, sampleSnapshot, sampleTerminal } from "./fixtures/tauriMock";

/** 画面のコンソールのエラーと、捕まえられなかった例外を集める。 */
function collectPageErrors(page: Page): string[] {
  const errors: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });
  page.on("pageerror", (error) => errors.push(error.message));
  return errors;
}

/**
 * 画面の側に溜まっている仕事を流し切る。
 * 準備完了の通知が「ちょうど 1 回」であることを見るため、遅れて届く 2 回目があれば届く余地を作ってから数える。
 */
async function settle(page: Page): Promise<void> {
  await page.evaluate(
    () =>
      new Promise<void>((resolve) => {
        requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(resolve, 0)));
      }),
  );
}

test("画面を開くと AppShell が表示され、写しの取得のあと準備完了がちょうど 1 回届く", async ({ page }) => {
  const errors = collectPageErrors(page);
  await installTauriMock(page);

  await page.goto("/");

  await expect(page.getByTestId("app-shell")).toBeVisible();
  await expect
    .poll(async () => (await readRecordedCalls(page)).map((call) => call.command))
    .toContain("notify_app_ready");
  await settle(page);

  // 本物の経路(@tauri-apps/api の invoke)を通るので、引数の無いコマンドは {} で届く。
  // 起動時の指定(T21)を取ってからエンジンを作り、そのあとで写しを取る。
  expect(await readRecordedCalls(page)).toEqual([
    { command: "get_launch_options", args: {} },
    { command: "get_app_snapshot", args: {} },
    { command: "notify_app_ready", args: {} },
  ]);
  await expect(page.getByRole("alert")).toHaveCount(0);
  expect(errors).toEqual([]);
});

test("偽の Rust 側が返した写しのアプリの版が画面に出る", async ({ page }) => {
  const errors = collectPageErrors(page);
  await installTauriMock(page, {
    get_app_snapshot: { ok: sampleSnapshot({ appVersion: "9.8.7-e2e" }) },
  });

  await page.goto("/");

  await expect(page.getByTestId("app-version")).toHaveText("9.8.7-e2e");
  expect(errors).toEqual([]);
});

test("写しの取得が失敗すると、失敗が alert で表示され、準備完了は通知されない", async ({ page }) => {
  const errors = collectPageErrors(page);
  // Tauri のコマンドの失敗は文字列で届く。
  await installTauriMock(page, { get_app_snapshot: { error: "偽の Rust 側の失敗" } });

  await page.goto("/");

  await expect(page.getByRole("alert")).toHaveText("状態の写しを取得できなかった: 偽の Rust 側の失敗");
  await settle(page);

  expect(await readRecordedCalls(page)).toEqual([
    { command: "get_launch_options", args: {} },
    { command: "get_app_snapshot", args: {} },
  ]);
  await expect(page.getByTestId("app-version")).toHaveCount(0);
  // 失敗はログにも残す(startApp.ts)。
  expect(errors).toEqual([expect.stringContaining("状態の写しを取得できなかった: 偽の Rust 側の失敗")]);
});

test("起動時の指定を取得できなくても既定で起動し、準備完了を通知し、診断に 1 行だけ出す", async ({ page }) => {
  await installTauriMock(page, {
    get_launch_options: { error: "command get_launch_options not found" },
    get_app_snapshot: { ok: sampleSnapshot({ terminals: [sampleTerminal("t-1", { kind: "running" })] }) },
  });

  await page.goto("/");

  await expect(page.getByTestId("terminal-renderer")).toHaveText("dom");
  await expect
    .poll(async () => (await readRecordedCalls(page)).map((call) => call.command))
    .toContain("notify_app_ready");
  const reports = (await readRecordedCalls(page)).filter((call) => call.command === "report_frontend_error");
  expect(reports.map((call) => call.args)).toEqual([
    {
      message:
        "起動時の指定を取得できなかったので、既定(dom・unicode11)で起動する: command get_launch_options not found",
    },
  ]);
});
