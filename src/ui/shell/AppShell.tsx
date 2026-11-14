import { useStore } from "zustand";

import { APP_NAME } from "../../logic/index";
import type { AppStore } from "../../state/appStore";

interface AppShellProps {
  store: AppStore;
}

/** ウィンドウ全体の枠。M0 の時点では、アプリ名と写しの版、起動の失敗だけを描く。 */
export function AppShell({ store }: AppShellProps) {
  const snapshot = useStore(store, (s) => s.snapshot);
  const startupError = useStore(store, (s) => s.startupError);

  return (
    <main data-testid="app-shell">
      <h1>{APP_NAME}</h1>
      {snapshot !== null && <p data-testid="app-version">{snapshot.appVersion}</p>}
      {startupError !== null && (
        <p role="alert" data-testid="startup-error">
          {startupError}
        </p>
      )}
    </main>
  );
}
