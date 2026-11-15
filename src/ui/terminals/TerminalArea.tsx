import "./TerminalArea.css";

import { type ReactNode, useEffect, useId, useRef } from "react";
import { useStore } from "zustand";

import type { Actions } from "../../state/actions";
import type { AppStore } from "../../state/appStore";
import { projectTabs, selectedProject, shownTerminal } from "../../state/selectors";
import { describeTerminalStatus } from "../../state/terminalStatus";
import type { TerminalManager } from "../../terminal/TerminalManager";
import { TabBar, tabDomId } from "./TabBar";
import { TerminalHost } from "./TerminalHost";

interface TerminalAreaProps {
  store: AppStore;
  actions: Pick<Actions, "newTerminal" | "selectTerminal" | "closeTerminal">;
  terminals: TerminalManager;
}

/**
 * 右ペインのターミナル領域(設計書 §8「タブ表示」「タブ 0 枚」、§9.1 の Container)。
 * 選択中のプロジェクトのタブバーと、表示中のタブのターミナルの置き場所を 1 つだけ描く。
 *
 * - 他のタブ・他のプロジェクトのターミナルは描かない。TerminalManager が写しのたびに隠したまま出力の取り込みを続け
 *   (FR-042。src/state/terminalSync.ts の present)、表示に変わったらこの置き場所へ要素を移す(接続は作り直さない)
 * - 表示中のタブが変わったら、そのターミナルに入力のフォーカスを移す(タブのクリック、⌘T・⌘W、プロジェクトの切替)
 * - タブ 0 枚(C-21)は「ターミナルがありません」と「新規ターミナル(⌘T)」。見つからないプロジェクトでは新しいタブを開けない
 * - プロジェクトが 0 件・選択が無い(一覧に無い)ときは、案内だけを出す(フォルダの追加は左ペインのボタン)
 */
export function TerminalArea({ store, actions, terminals }: TerminalAreaProps) {
  const snapshot = useStore(store, (s) => s.snapshot);
  const idBase = useId();
  const panelId = `${idBase}-panel`;
  const project = selectedProject(snapshot);
  const terminal = shownTerminal(snapshot);
  const shownId = terminal?.id ?? null;
  const previousShownId = useRef(shownId);
  const newTerminalButton = useRef<HTMLButtonElement>(null);

  // 表示中のターミナルが変わったら、置き場所への要素の移し替え(子の TerminalHost の effect)が済んだあと、
  // 次の描画のフレームでフォーカスを移す。WebKit は、フォーカスのあった要素(前のターミナルの入力欄)を外すと、
  // 次の描画の更新でフォーカスを外し直す(blur のイベントは出ない)ので、同じ処理の中で移したフォーカスは消える。
  // 表示中が無くなった(最後の 1 枚を閉じた)ときは、「新規ターミナル」のボタンへ移す(body に落とさない)。
  useEffect(() => {
    const previous = previousShownId.current;
    previousShownId.current = shownId;
    if (shownId === null && previous === null) return;
    const frame = requestAnimationFrame(() => {
      if (shownId === null) newTerminalButton.current?.focus();
      else terminals.focus(shownId);
    });
    return () => cancelAnimationFrame(frame);
  }, [terminals, shownId]);

  if (project === null) {
    const none = (snapshot?.projects.length ?? 0) === 0;
    return (
      <div className="terminal-area">
        {none ? (
          <EmptyState title="プロジェクトがありません">
            <p>左の「フォルダを追加」でフォルダを登録すると、そのフォルダでターミナルが開きます。</p>
          </EmptyState>
        ) : (
          <EmptyState title="プロジェクトを選んでください">
            <p>左の一覧からプロジェクトを選ぶと、そのプロジェクトのターミナルが出ます。</p>
          </EmptyState>
        )}
      </div>
    );
  }

  const tabs = projectTabs(snapshot, project.id);
  const status = terminal === null ? null : describeTerminalStatus(terminal.status);
  const canAdd = !project.missing;

  return (
    <div className="terminal-area">
      <TabBar
        tabs={tabs}
        activeId={shownId}
        projectName={project.name}
        color={project.color}
        idBase={idBase}
        panelId={panelId}
        canAdd={canAdd}
        onSelect={(id) => void actions.selectTerminal(id)}
        onClose={(id) => void actions.closeTerminal(id)}
        onNew={() => void actions.newTerminal()}
      />
      {terminal === null ? (
        <EmptyState title="ターミナルがありません">
          {canAdd ? (
            <button
              ref={newTerminalButton}
              type="button"
              className="button button--primary"
              onClick={() => void actions.newTerminal()}
            >
              新規ターミナル(⌘T)
            </button>
          ) : (
            <p>フォルダが見つからないため、新しいターミナルを開けません。左の「再リンク…」でつなぎ直してください。</p>
          )}
        </EmptyState>
      ) : (
        <div
          role="tabpanel"
          id={panelId}
          aria-labelledby={tabDomId(idBase, terminal.id)}
          className="terminal-area__panel"
        >
          {status !== null && (
            <p role="status" className="terminal-status" data-testid="terminal-status">
              {status}
            </p>
          )}
          {/* ターミナルごとに置き場所を作り直す(要素の付け外しは TerminalManager が行う)。 */}
          <TerminalHost key={terminal.id} manager={terminals} terminalId={terminal.id} />
        </div>
      )}
    </div>
  );
}

function EmptyState({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="terminals-empty" data-testid="terminals-empty">
      <p className="terminals-empty__title">{title}</p>
      <div className="terminals-empty__body">{children}</div>
    </div>
  );
}
