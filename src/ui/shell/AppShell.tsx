import "./AppShell.css";

import { useStore } from "zustand";

import { APP_NAME } from "../../logic/index";
import type { AppStore } from "../../state/appStore";
import { describeTerminalStatus } from "../../state/terminalStatus";
import type { TerminalManager } from "../../terminal/TerminalManager";
import { TerminalHost } from "../terminals/TerminalHost";

interface AppShellProps {
  store: AppStore;
  terminals: TerminalManager;
}

/** ウィンドウ全体の枠。M1 では、アプリ名と写しの版、起動の失敗、写しにある最初のターミナル(1 枚)を描く。 */
export function AppShell({ store, terminals }: AppShellProps) {
  const snapshot = useStore(store, (s) => s.snapshot);
  const startupError = useStore(store, (s) => s.startupError);
  const terminal = snapshot?.terminals[0];
  const status = terminal === undefined ? null : describeTerminalStatus(terminal.status);

  return (
    <main data-testid="app-shell" className="app-shell">
      <header className="app-shell__header">
        <h1>{APP_NAME}</h1>
        {snapshot !== null && <p data-testid="app-version">{snapshot.appVersion}</p>}
        {startupError !== null && (
          <p role="alert" data-testid="startup-error">
            {startupError}
          </p>
        )}
      </header>
      {terminal !== undefined && (
        <section className="app-shell__terminal" aria-label="ターミナル">
          {status !== null && (
            <p role="status" className="terminal-status" data-testid="terminal-status">
              {status}
            </p>
          )}
          <TerminalHost manager={terminals} terminalId={terminal.id} />
        </section>
      )}
    </main>
  );
}
