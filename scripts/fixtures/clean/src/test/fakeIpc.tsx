// テストの補助(src/test/)は製品に入らないので、検査から外す。
import { mockIPC } from "@tauri-apps/api/mocks";

export function installFakeIpc(): void {
  mockIPC(() => "pong");
}
