import { defineConfig, devices } from "@playwright/test";

// 画面の自動テスト(ブラウザ方式。設計書 ARCH_multi-terminal.md §10.3、ADR-006)。
//
// Tauri を起動せず、Vite の開発サーバーが配る画面を Playwright の WebKit で開く。
// Rust との通信は e2e/fixtures/tauriMock.ts が偽物に差し替える(差し替えはテストの側だけで行い、
// 製品のソースには差し込み口を置かない)。実アプリは `npm run smoke` で確かめる。
//
// 出力はすべて testreport/e2e/ の下に置く(testreport/ は .gitignore 済み)。
// HTML のレポートは書き出すたびに出力先を空にするので、トレースやスクリーンショット(outputDir)とは
// 別のフォルダにする(同じフォルダや入れ子にすると、Playwright が衝突を警告し、生成物が消える)。

// Tauri の開発サーバーと同じ固定のポート(vite.config.ts の strictPort、tauri.conf.json の devUrl)。
const BASE_URL = "http://localhost:1420";

// 性能の計測の spec(e2e/perf-*.spec.ts。T49b)は `npm run e2e` では動かさない。`npm run perf`(scripts/perf/run.mjs)が
// ARIN_PERF_E2E=1 を付けて、その spec だけを 1 ワーカーで動かす(重いので、ほかの画面の自動テストと並べない)。
const PERF_E2E = process.env.ARIN_PERF_E2E === "1";

export default defineConfig({
  testDir: "./e2e",
  testIgnore: PERF_E2E ? [] : ["**/perf-*.spec.ts"],
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: [["list"], ["html", { outputFolder: "testreport/e2e/report", open: "never" }]],
  outputDir: "testreport/e2e/results",

  use: {
    baseURL: BASE_URL,
    // 失敗したテストのときだけ、手がかりを残す。
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    video: "off",
  },

  webServer: {
    command: "npm run dev:web",
    url: BASE_URL,
    // 既に動いている開発サーバーは使い回さない。別のブランチや作業ツリーのサーバーが 1420 番にいても、
    // 気づかずにそちらを試してしまわないようにするため(ポートが埋まっていれば Vite の strictPort で失敗する)。
    // テストが終わると Playwright がこのサーバーを止める。
    reuseExistingServer: false,
    timeout: 60_000,
  },

  // ブラウザは WebKit だけ(WKWebView と同じ系統のエンジン。§10.3)。
  projects: [
    {
      name: "webkit",
      use: { ...devices["Desktop Safari"] },
    },
  ],
});
