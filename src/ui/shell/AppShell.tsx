import { APP_NAME } from "../../logic/index";

/** ウィンドウ全体の枠。M0 の時点では空ウィンドウの中身だけを描く。 */
export function AppShell() {
  return (
    <main data-testid="app-shell">
      <h1>{APP_NAME}</h1>
    </main>
  );
}
