import "./AppShell.css";

import { useStore } from "zustand";

import { APP_NAME } from "../../logic/index";
import type { AppStore } from "../../state/appStore";
import { describeTerminalStatus } from "../../state/terminalStatus";
import type { TerminalManager } from "../../terminal/TerminalManager";
import { TerminalHost } from "../terminals/TerminalHost";
import { VerificationGrid } from "../terminals/VerificationGrid";
import { RendererLabel } from "./RendererLabel";

interface AppShellProps {
  store: AppStore;
  terminals: TerminalManager;
  /** 9 枚の検証画面(起動時の指定 ARIN_VERIFY_GRID。M1 の検証用で、M4 の T49 で外す)。既定は false。 */
  verifyGrid?: boolean;
}

/**
 * ウィンドウ全体の枠。M1 では、アプリ名と写しの版、描画方式(検証用)、起動の失敗、写しにある最初のターミナル(1 枚)を描く。
 * 9 枚の検証画面の指定があるときは、ターミナルの代わりに検証画面(写しにある全部のターミナル)を描く。
 */
export function AppShell({ store, terminals, verifyGrid = false }: AppShellProps) {
  const snapshot = useStore(store, (s) => s.snapshot);
  const startupError = useStore(store, (s) => s.startupError);
  const terminal = snapshot?.terminals[0];
  const status = terminal === undefined ? null : describeTerminalStatus(terminal.status);

  return (
    <main data-testid="app-shell" className="app-shell">
      <header className="app-shell__header">
        <h1>{APP_NAME}</h1>
        {snapshot !== null && <p data-testid="app-version">{snapshot.appVersion}</p>}
        {verifyGrid ? (
          <p className="app-shell__renderer" data-testid="verify-grid-label">
            検証画面(M1)
          </p>
        ) : (
          terminal !== undefined && <RendererLabel terminals={terminals} terminalId={terminal.id} />
        )}
        {startupError !== null && (
          <p role="alert" data-testid="startup-error">
            {startupError}
          </p>
        )}
      </header>
      {verifyGrid
        ? snapshot !== null && <VerificationGrid terminals={terminals} items={snapshot.terminals} />
        : terminal !== undefined && (
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
