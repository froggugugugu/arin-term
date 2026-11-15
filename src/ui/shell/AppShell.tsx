import "../design/tokens.css";
import "./AppShell.css";

import type { ReactNode } from "react";
import { useStore } from "zustand";

import { APP_NAME } from "../../logic/index";
import { shortcutLabel } from "../../logic/keys/shortcutTable";
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
import type { Snapshot } from "../snapshotTypes";
import { SidebarIcon, TabsViewIcon, TilesViewIcon } from "../tree/icons";
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
 * ウィンドウ全体の枠(設計書 §8・§9.1)。上から順に、上部の帯(左にサイドバーの開閉のアイコンのボタンと起動の失敗、
 * 右端にアプリ名・写しの版・描画方式(検証用)を小さく控えめに)、知らせ(1 行)、ペインの分割(左: カード一覧とファイルツリー、
 * 右: ターミナル領域)。削除と終了の確認もここに置く。
 * 地はウィンドウのすりガラス(ADR-010)に半透明の淡い色を重ねたもので、ターミナルの置き場所だけが不透明。
 * 上部の帯の右側(アプリ名の左)に、タブ表示とタイル表示を切り替える区分けのボタンを置く(FR-048。計画【仮定】11)。
 * 右ペインは表示モードに従い、タブ表示ならタブバーと表示中のターミナル 1 枚、タイル表示なら表示中のページのタイルを描く
 * (TerminalArea)。見えないターミナルは TerminalManager が隠したまま取り込みを続ける。
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
            className="icon-button app-shell__sidebar-toggle"
            aria-controls="pane-sidebar"
            aria-expanded={!collapsed}
            aria-label={collapsed ? "左ペインを表示" : "左ペインを隠す"}
            title={collapsed ? "左ペインを表示" : "左ペインを隠す"}
            onClick={() => panes.setCollapsed(!collapsed)}
          >
            <SidebarIcon />
          </button>
        )}
        {startupError !== null && (
          <p role="alert" className="app-shell__error" data-testid="startup-error">
            {startupError}
          </p>
        )}
        {!verifyGrid && snapshot !== null && (
          <DisplayModeSwitch
            mode={snapshot.display.mode}
            ui={ui}
            onChange={(mode) => void actions.setDisplayMode(mode)}
          />
        )}
        <div className="app-shell__meta">
          <h1 className="app-shell__name">{APP_NAME}</h1>
          {snapshot !== null && (
            <p className="app-shell__version" data-testid="app-version">
              {snapshot.appVersion}
            </p>
          )}
          {verifyGrid ? (
            <p className="app-shell__renderer" data-testid="verify-grid-label">
              検証画面(M1)
            </p>
          ) : (
            terminal !== undefined && <RendererLabel terminals={terminals} terminalId={terminal.id} />
          )}
        </div>
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
                terminal={<TerminalArea store={store} ui={ui} actions={actions} terminals={terminals} />}
              />
              <RemoveProjectDialog store={store} ui={ui} actions={actions} />
            </>
          )}
      <QuitDialog ui={ui} actions={actions} />
    </main>
  );
}

type DisplayMode = Snapshot["display"]["mode"];

interface DisplayModeSwitchProps {
  mode: DisplayMode;
  /** ショートカットの表(ツールチップのキーを引く。部品にキーの文字列を書かない)。 */
  ui: UiStore;
  onChange: (mode: DisplayMode) => void;
}

/**
 * 表示モードの切替(FR-048。macOS のセグメントのコントロールにならう)。タブ表示とタイル表示の線画のアイコン 2 つで、
 * いまのモードの区分けだけ明るく浮かせる。読み上げは aria-pressed で分かる。いまのモードを押しても何も送らない。
 * ツールチップは「タイル表示(⇧⌘G)」の形で、キーは表の view.toggle-display-mode から引く。
 */
function DisplayModeSwitch({ mode, ui, onChange }: DisplayModeSwitchProps) {
  const shortcuts = useStore(ui, (s) => s.shortcuts);
  const key = shortcutLabel(shortcuts, "view.toggle-display-mode");
  const segment = (value: DisplayMode, label: string, icon: ReactNode) => {
    const pressed = mode === value;
    return (
      <button
        type="button"
        className={pressed ? "segmented__item segmented__item--selected" : "segmented__item"}
        aria-label={label}
        aria-pressed={pressed}
        title={key === null ? label : `${label}(${key})`}
        onClick={() => {
          if (!pressed) onChange(value);
        }}
      >
        {icon}
      </button>
    );
  };
  return (
    <div className="segmented app-shell__display-mode" role="group" aria-label="表示モード" data-testid="display-mode-switch">
      {segment("tabs", "タブ表示", <TabsViewIcon />)}
      {segment("tiles", "タイル表示", <TilesViewIcon />)}
    </div>
  );
}
