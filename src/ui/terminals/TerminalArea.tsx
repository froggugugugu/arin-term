import "./TerminalArea.css";

import { type ReactNode, useEffect, useId, useRef } from "react";
import { useStore } from "zustand";

import type { Actions } from "../../state/actions";
import type { AppStore } from "../../state/appStore";
import { projectTabs, selectedProject, shownTerminal, tileView } from "../../state/selectors";
import { describeTerminalStatus } from "../../state/terminalStatus";
import type { UiStore } from "../../state/uiStore";
import type { TerminalId } from "../../terminal/engine";
import type { TerminalManager } from "../../terminal/TerminalManager";
import { TabBar, tabDomId } from "./TabBar";
import { TerminalHost } from "./TerminalHost";
import { MaximizedTile, TileGrid } from "./TileGrid";

interface TerminalAreaProps {
  store: AppStore;
  /** 表示だけの状態(ページ表示のツールチップのキーを引くショートカットの表)。 */
  ui: UiStore;
  actions: Pick<
    Actions,
    | "newTerminal"
    | "selectTerminal"
    | "closeTerminal"
    | "focusTile"
    | "nextTilePage"
    | "previousTilePage"
    | "toggleTileMaximize"
  >;
  terminals: TerminalManager;
}

/**
 * 右ペインのターミナル領域(設計書 §8「タブ表示」「タブ 0 枚」「タイル表示」、§9.1 の Container)。
 * 写しの表示モード(display.mode)で出し分ける。
 * - タブ表示: 選択中のプロジェクトのタブバーと、表示中のタブのターミナルの置き場所を 1 つだけ描く
 * - タイル表示: TileGrid(帯とページ表示、表示中のページのタイル)。0 枚なら「ターミナルがありません」だけ(【仮定】2)
 * - タイル表示の拡大中: MaximizedTile(拡大しているタイルの名前と元に戻すボタンの帯と、そのターミナル 1 枚。前提 (d))
 *
 * - 他のタブ・他のプロジェクトのターミナルは描かない。TerminalManager が写しのたびに隠したまま出力の取り込みを続け
 *   (FR-042。src/state/terminalSync.ts の present)、表示に変わったらこの置き場所へ要素を移す(接続は作り直さない)
 * - 入力のフォーカスの宛先(タブ表示は表示中のタブ、タイル表示はフォーカス中のタイル)・表示モード・拡大中のどれかが
 *   変わったら、そのターミナルに入力のフォーカスを移す(タブのクリック、⌘T・⌘W、プロジェクトの切替、タイルのクリック、
 *   ページ送り、表示モードの切替、拡大と復帰)。ただし、利用者がほかの場所で操作している最中は奪わない(mayTakeInputFocus)
 * - タブ 0 枚(C-21)は「ターミナルがありません」と「新規ターミナル(⌘T)」。見つからないプロジェクトでは新しいタブを開けない
 * - プロジェクトが 0 件・選択が無い(一覧に無い)ときは、案内だけを出す(フォルダの追加は左ペインのボタン)
 */
export function TerminalArea({ store, ui, actions, terminals }: TerminalAreaProps) {
  const snapshot = useStore(store, (s) => s.snapshot);
  const view = useStore(store, (s) => tileView(s.snapshot));
  const idBase = useId();
  const panelId = `${idBase}-panel`;
  const project = selectedProject(snapshot);
  const terminal = shownTerminal(snapshot);
  const shownId = terminal?.id ?? null;
  // 入力のフォーカスの宛先(inputFocusTarget)。
  const focusTarget = inputFocusTarget(snapshot);
  // 宛先が同じでも、表示モードの切替と拡大・復帰では置き場所が作り直され、前の置き場所の入力欄のフォーカスが外れるので移し直す。
  const focusKey = `${view.mode}|${view.maximized}|${focusTarget ?? ""}`;
  /** 前回フォーカスを移そうとしたときの目印と宛先。 */
  const previousFocus = useRef<{ key: string; target: TerminalId | null } | null>(null);
  const newTerminalButton = useRef<HTMLButtonElement>(null);

  // 宛先が変わったら、置き場所への要素の移し替え(子の TerminalHost の effect)が済んだあと、次の描画のフレームで
  // フォーカスを移す。WebKit は、フォーカスのあった要素(前のターミナルの入力欄)を外すと、次の描画の更新でフォーカスを
  // 外し直す(blur のイベントは出ない)ので、同じ処理の中で移したフォーカスは消える。
  // 宛先が無くなった(最後の 1 枚を閉じた)ときは、「新規ターミナル」のボタンへ移す(body に落とさない。タブ表示だけにある)。
  // 写しが変わっただけ(宛先・モード・拡大が同じ)なら、この effect は動かない。
  // 移す直前に、いまの写しの宛先がまだ同じかを確かめる(描画のフレームを待つあいだに新しい写しが届いていたら、古い宛先へ
  // 移さない。タイルは入力のフォーカスが入るとフォーカスのコマンドを送るので、古い宛先へ移すと Rust のフォーカス中の
  // タイルを古いほうへ戻してしまう。M4 のレビューの S2)。新しい宛先へは、新しい写しの effect が移す。
  useEffect(() => {
    const previous = previousFocus.current;
    previousFocus.current = { key: focusKey, target: focusTarget };
    if (focusTarget === null && (previous?.target ?? null) === null) return;
    if (!mayTakeInputFocus()) return;
    const frame = requestAnimationFrame(() => {
      if (hasOpenOverlay()) return;
      if (inputFocusTarget(store.getState().snapshot) !== focusTarget) return;
      if (focusTarget === null) newTerminalButton.current?.focus();
      else terminals.focus(focusTarget);
    });
    return () => cancelAnimationFrame(frame);
  }, [store, terminals, focusTarget, focusKey]);

  if (view.mode === "tiles" && view.maximized) {
    const item = view.focusedTile;
    return (
      <div className="terminal-area" data-display-mode="tiles" data-maximized="true">
        {item === null ? (
          <EmptyState title="ターミナルがありません" />
        ) : (
          <MaximizedTile item={item} ui={ui} actions={actions} terminals={terminals} />
        )}
      </div>
    );
  }

  if (view.mode === "tiles" && !view.maximized) {
    const none = (snapshot?.projects.length ?? 0) === 0;
    return (
      <div className="terminal-area" data-display-mode="tiles">
        {none ? (
          <NoProjects />
        ) : view.tiles.length === 0 ? (
          <EmptyState title="ターミナルがありません" />
        ) : (
          <TileGrid view={view} store={store} ui={ui} actions={actions} terminals={terminals} />
        )}
      </div>
    );
  }

  if (project === null) {
    const none = (snapshot?.projects.length ?? 0) === 0;
    return (
      <div className="terminal-area">
        {none ? (
          <NoProjects />
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

function NoProjects() {
  return (
    <EmptyState title="プロジェクトがありません">
      <p>左の「フォルダを追加」でフォルダを登録すると、そのフォルダでターミナルが開きます。</p>
    </EmptyState>
  );
}

function EmptyState({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <div className="terminals-empty" data-testid="terminals-empty">
      <p className="terminals-empty__title">{title}</p>
      {children !== undefined && <div className="terminals-empty__body">{children}</div>}
    </div>
  );
}

/**
 * 入力のフォーカスの宛先。タイル表示(拡大中を含む)はフォーカス中のタイル(=選択中のプロジェクトの表示中のタブ。前提 (c))、
 * タブ表示は表示中のタブ。無ければ null。
 */
function inputFocusTarget(snapshot: ReturnType<AppStore["getState"]>["snapshot"]): TerminalId | null {
  const view = tileView(snapshot);
  if (view.mode === "tiles") return view.focusedTile?.id ?? null;
  return shownTerminal(snapshot)?.id ?? null;
}

/** 開いているメニュー・確認ダイアログがあるか(あれば、そちらの操作の最中なので入力のフォーカスを動かさない)。 */
function hasOpenOverlay(): boolean {
  return document.querySelector('[role="menu"], [role="dialog"], [role="alertdialog"], dialog[open]') !== null;
}

/**
 * いま入力のフォーカスをターミナルへ移してよいか(M2 のレビュー: 勝手にフォーカスを奪わない)。宛先が変わった時点の
 * フォーカスの位置で決める。
 * - 奪わない: メニュー・確認ダイアログの表示中、ツリーにフォーカスがある(キーで操作している最中・名前の入力中)、
 *   右ペインの外の入力欄(文字を打っている最中)
 * - 移す: フォーカスが無い(body)、右ペインの中(前のターミナル、タブ、ページ送りや元に戻すボタン)、それ以外の
 *   ボタンなど(カードの選択、表示モードの切替。押した結果として移ってよい)
 */
function mayTakeInputFocus(): boolean {
  if (hasOpenOverlay()) return false;
  const active = document.activeElement;
  if (active === null || active === document.body) return true;
  if (active.closest(".terminal-area") !== null) return true;
  if (active.closest('[role="tree"]') !== null) return false;
  return !active.matches('input, textarea, select, [contenteditable]:not([contenteditable="false"])');
}
