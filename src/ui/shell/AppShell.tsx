import "../design/tokens.css";
import "./AppShell.css";

import { type ReactNode, useInsertionEffect, useMemo } from "react";
import { useStore } from "zustand";

import { INTERFACE_BASE_SIZE, interfaceMetrics } from "../../logic/appearance/interfaceMetrics";
import { TRANSLUCENCY_DEFAULTS, translucencyTokens } from "../../logic/appearance/translucency";
import { APP_NAME } from "../../logic/index";
import { shortcutLabel } from "../../logic/keys/shortcutTable";
import { DEFAULT_PANE_SIZES } from "../../logic/panes/paneLayout";
import type { Actions } from "../../state/actions";
import type { EditorManager } from "../../editor/EditorManager";
import type { AppStore } from "../../state/appStore";
import type { EditorController } from "../../state/editorController";
import { selectedProject } from "../../state/selectors";
import type { TreeStore } from "../../state/treeStore";
import type { UiStore } from "../../state/uiStore";
import type { TerminalManager } from "../../terminal/TerminalManager";
import { applyInterfaceMetrics, InterfaceMetricsContext } from "../design/applyInterfaceMetrics";
import { applyTranslucency } from "../design/applyTranslucency";
import { CloseTabsDialog } from "../dialogs/CloseTabsDialog";
import { QuitDialog } from "../dialogs/QuitDialog";
import { RemoveProjectDialog } from "../dialogs/RemoveProjectDialog";
import { SaveConflictDialog } from "../dialogs/SaveConflictDialog";
import { UnsavedCloseDialog } from "../dialogs/UnsavedCloseDialog";
import { ProjectCardList } from "../projects/ProjectCardList";
import { SettingsDialog } from "../settings/SettingsDialog";
import { TerminalArea } from "../terminals/TerminalArea";
import type { Snapshot } from "../snapshotTypes";
import { SidebarIcon, TabsViewIcon, TilesViewIcon } from "../tree/icons";
import { NoticeBar, SettingsNoticeBar } from "./NoticeBar";
import { PaneSplit } from "./PaneSplit";
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
  /** エディタの操作(M4.8。タブ・保存・確認・Vim)。 */
  editor: EditorController;
  /** エディタの部品(置き場所への付け外しとフォーカス)。 */
  editors: EditorManager;
}

/**
 * ウィンドウ全体の枠(設計書 §8・§9.1)。上から順に、上部の帯(左にサイドバーの開閉のアイコンのボタンと起動の失敗、
 * 右端にアプリ名と写しの版を小さく控えめに)、知らせ(状態の知らせと設定の知らせ。それぞれ 1 行で、別々に閉じる)、ペインの分割(左: カード一覧とファイルツリー、
 * 右: ターミナル領域)。削除と終了の確認もここに置く。
 * 地はウィンドウのすりガラス(ADR-010)に半透明の淡い色を重ねたもので、ターミナルの置き場所だけが不透明。
 * 上部の帯の右側(アプリ名の左)に、タブ表示とタイル表示を切り替える区分けのボタンを置く(FR-048。計画【仮定】11)。
 * 設定画面(⌘, とメニューの「設定…」。M4.6)も確認と同じくここに置く(終了の確認はその上に重なる)。
 * 右ペインは表示モードに従い、タブ表示ならタブバーと表示中のターミナル 1 枚、タイル表示なら表示中のページのタイルを描く
 * (TerminalArea)。見えないターミナルは TerminalManager が隠したまま取り込みを続ける。
 * 画面の文字の大きさ(写しの fontSizes.interface。M4.6)は、:root の変数に当て、ツリーの行の高さは文脈で配る(ADR-013 の 4)。
 * 透過(写しの translucency。M4.7)は、ウィンドウとターミナルの地の不透明度の変数を :root に当てる(ADR-014 の 2)。
 * エディタ(M4.8)の閉じる確認と保存の衝突の確認もここに置く。
 */
export function AppShell({ store, ui, tree, actions, terminals, editor, editors }: AppShellProps) {
  const snapshot = useStore(store, (s) => s.snapshot);
  const startupError = useStore(store, (s) => s.startupError);
  const panes = usePaneSizes(snapshot?.layout ?? DEFAULT_PANE_SIZES, actions);
  const collapsed = panes.sizes.sidebarCollapsed;
  const interfaceSize = snapshot?.fontSizes.interface.value ?? INTERFACE_BASE_SIZE;
  const metrics = useMemo(() => interfaceMetrics(interfaceSize), [interfaceSize]);
  // 変数は、子の部品が寸法を測る(レイアウトの副作用)より前に当てる。挿入の副作用は子のレイアウトの副作用より先に走る
  // (ツリーは選択中の行を見せるとき、新しい行の高さで縮んだ・伸びた見える範囲を測る)。
  useInsertionEffect(() => applyInterfaceMetrics(document.documentElement, metrics), [metrics]);
  const windowTranslucency = snapshot?.translucency.window.value ?? TRANSLUCENCY_DEFAULTS.window;
  const terminalTranslucency = snapshot?.translucency.terminal.value ?? TRANSLUCENCY_DEFAULTS.terminal;
  useInsertionEffect(
    () => applyTranslucency(document.documentElement, translucencyTokens(windowTranslucency, terminalTranslucency)),
    [windowTranslucency, terminalTranslucency],
  );

  return (
    <InterfaceMetricsContext value={metrics}>
      <main data-testid="app-shell" className="app-shell">
        <header className="app-shell__header">
          {snapshot !== null && (
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
          {snapshot !== null && (
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
          </div>
        </header>
        {snapshot !== null && (
          <>
            <NoticeBar notice={snapshot.notice} ui={ui} />
            <SettingsNoticeBar ui={ui} />
            <PaneSplit
              panes={panes}
              cards={<ProjectCardList store={store} ui={ui} actions={actions} />}
              tree={<TreePane project={selectedProject(snapshot)} tree={tree} ui={ui} actions={actions} />}
              terminal={
                <TerminalArea
                  store={store}
                  ui={ui}
                  actions={actions}
                  terminals={terminals}
                  editor={editor}
                  editors={editors}
                />
              }
            />
            <RemoveProjectDialog store={store} ui={ui} actions={actions} />
            <CloseTabsDialog ui={ui} actions={actions} />
            <SettingsDialog store={store} ui={ui} actions={actions} editor={editor} />
            <UnsavedCloseDialog store={store} ui={ui} editor={editor} />
            <SaveConflictDialog store={store} ui={ui} editor={editor} />
          </>
        )}
        <QuitDialog ui={ui} actions={actions} />
      </main>
    </InterfaceMetricsContext>
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
