import { defineConfig, devices } from "@playwright/test";

// README と紹介ページ(.github/pages/index.html)の画像を撮る専用の設定。
//
// ここのファイルは `*.visual.ts` にし、ルートの playwright.config.ts(`npm run e2e`)の既定の対象
// (名前に `.spec.` か `.test.` を含むもの)に入れない。通常の画面の自動テストには混ざらない。
//
// - `npm run screenshots`: アプリの画面を撮って docs/media/ に書く(media.visual.ts)
// - `npm run screenshots:pages`: 紹介ページを開いて崩れを確かめ、画像を testreport/pages/ に撮る(pages.visual.ts)
//
// 画面は `npm run e2e` と同じく Vite の画面を WebKit で開き、Rust との通信は e2e/fixtures/tauriMock.ts が差し替える。
// ブラウザには窓の向こうが無いので、実アプリのすりガラスの透け感は写らない。

const BASE_URL = "http://localhost:1420";

export default defineConfig({
  testDir: "./",
  testMatch: /.*\.visual\.ts$/,
  fullyParallel: false,
  workers: 1,
  reporter: [["list"]],
  outputDir: "../../testreport/screenshots/results",

  use: {
    baseURL: BASE_URL,
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    video: "off",
  },

  // ルートの設定と同じ開発サーバー(1420 番。使い回さない理由はルートの設定を参照)。
  // 紹介ページだけを撮るときも立ち上がる(Playwright の webServer はテストの選び方によらず起動する)。
  webServer: {
    command: "npm run dev:web",
    url: BASE_URL,
    reuseExistingServer: false,
    timeout: 60_000,
  },

  projects: [
    {
      name: "webkit",
      use: { ...devices["Desktop Safari"] },
    },
  ],
});
