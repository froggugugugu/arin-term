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
  },
});
