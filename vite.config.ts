import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

// Tauri の開発サーバーは固定のポートを前提にする(tauri.conf.json の devUrl と合わせる)。
const DEV_PORT = 1420;

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: DEV_PORT,
    strictPort: true,
  },
  build: {
    // 最低 OS は macOS 15(NFR-007)。その WebView は Safari 18 相当。
    target: "safari18",
  },
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
    // カバレッジは `npm run test:coverage`(scripts/coverage.mjs)が有効にする。
    // ここでは計測の範囲と出力だけを決める。80% の判定は src/logic だけにかけるので、
    // 判定はスクリプト側で json-summary を読んで行う(ここに thresholds は置かない)。
    coverage: {
      provider: "v8",
      // テストに触れられていないファイルも 0% として数えるため、include を明示する。
      include: ["src/**/*.{ts,tsx}"],
      exclude: ["src/**/*.test.ts", "src/**/*.d.ts", "src/ipc/generated/**"],
      reporter: ["text-summary", "html", "json-summary", "lcov"],
      reportsDirectory: "testreport/coverage/ts",
    },
  },
});
