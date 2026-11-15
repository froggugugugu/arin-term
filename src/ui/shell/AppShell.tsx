import "../design/tokens.css";
import "./AppShell.css";

import { useStore } from "zustand";

import { APP_NAME } from "../../logic/index";
import { DEFAULT_PANE_SIZES } from "../../logic/panes/paneLayout";
import type { Actions } from "../../state/actions";
import type { AppStore } from "../../state/appStore";
import { selectedProject, shownTerminal } from "../../state/selectors";
import type { TreeStore } from "../../state/treeStore";
import type { UiStore } from "../../state/uiStore";
import type { TerminalManager } from "../../terminal/TerminalManager";
import { QuitDialog } from "../dialogs/QuitDialog";
import { RemoveProjectDialog } from "../dialogs/RemoveProjectDialog";
import { ProjectCardList } from "../projects/ProjectCardList";
import { TerminalArea } from "../terminals/TerminalArea";
import { VerificationGrid } from "../terminals/VerificationGrid";
import { NoticeBar } from "./NoticeBar";
import { PaneSplit } from "./PaneSplit";
import { RendererLabel } from "./RendererLabel";
import { TreePane } from "./TreePane";
import { usePaneSizes } from "./usePaneSizes";

interface AppShellProps {
  store: AppStore;
  /** 表示だけの状態(確認・メニュー・閉じた知らせ)。 */
  ui: UiStore;
  /** ツリーの状態(表示中のプロジェクトのツリー。M3)。 */
  tree: TreeStore;
  /** 画面の操作(コマンドの包み)。 */
  actions: Actions;
  terminals: TerminalManager;
  /** 9 枚の検証画面(起動時の指定 ARIN_VERIFY_GRID。M1 の検証用で、M4 の T49 で外す)。既定は false。 */
  verifyGrid?: boolean;
}

/**
 * ウィンドウ全体の枠(設計書 §8・§9.1)。上から順に、見出し(アプリ名・写しの版・左ペインの折りたたみ・描画方式(検証用)・
 * 起動の失敗)、知らせ(1 行)、ペインの分割(左: カード一覧とファイルツリー、右: ターミナル領域)。削除と終了の確認もここに置く。
 * 右ペインはタブバーと表示中のターミナル(選択中のプロジェクトの表示中のタブ)を 1 枚だけ描く(TerminalArea)。
 * 他のタブ・他のプロジェクトのターミナルは TerminalManager が隠したまま取り込みを続ける。
 * 9 枚の検証画面の指定があるときは、M1 と同じく検証画面(写しにある全部のターミナル)だけを描く。終了の確認はどちらでも出す。
 */
export function AppShell({ store, ui, tree, actions, terminals, verifyGrid = false }: AppShellProps) {
  const snapshot = useStore(store, (s) => s.snapshot);
  const startupError = useStore(store, (s) => s.startupError);
  const panes = usePaneSizes(snapshot?.layout ?? DEFAULT_PANE_SIZES, actions);
  const terminal = shownTerminal(snapshot) ?? undefined;
  const collapsed = panes.sizes.sidebarCollapsed;

  return (
    <main data-testid="app-shell" className="app-shell">
      <header className="app-shell__header">
        {!verifyGrid && snapshot !== null && (
          <button
            type="button"
            className="app-shell__sidebar-toggle"
            aria-controls="pane-sidebar"
            aria-expanded={!collapsed}
            onClick={() => panes.setCollapsed(!collapsed)}
          >
            {collapsed ? "左ペインを表示" : "左ペインを隠す"}
          </button>
        )}
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
        : snapshot !== null && (
            <>
              <NoticeBar notice={snapshot.notice} ui={ui} />
              <PaneSplit
                panes={panes}
                cards={<ProjectCardList store={store} ui={ui} actions={actions} />}
                tree={<TreePane project={selectedProject(snapshot)} tree={tree} ui={ui} actions={actions} />}
                terminal={<TerminalArea store={store} actions={actions} terminals={terminals} />}
              />
              <RemoveProjectDialog store={store} ui={ui} actions={actions} />
            </>
          )}
      <QuitDialog ui={ui} actions={actions} />
    </main>
  );
}
