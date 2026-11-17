import "./TileGrid.css";

import type { CSSProperties, FocusEvent, MouseEvent, PointerEvent } from "react";
import { useStore } from "zustand";

import type { Actions } from "../../state/actions";
import type { AppStore } from "../../state/appStore";
import type { TileItem, TileView } from "../../state/selectors";
import { describeTerminalStatus } from "../../state/terminalStatus";
import { shortcutLabel } from "../../logic/keys/shortcutTable";
import type { UiStore } from "../../state/uiStore";
import type { TerminalManager } from "../../terminal/TerminalManager";
import { projectColorStyle } from "../design/palette";
import { RestoreIcon } from "../tree/icons";
import { PageControl } from "./PageControl";
import { TerminalHost } from "./TerminalHost";
import { TileTitleBar } from "./TileTitleBar";

interface TileGridProps {
  /** 右ペインの表示(tileView の結果。タイル表示で拡大中でないとき)。 */
  view: TileView;
  store: AppStore;
  ui: UiStore;
  actions: Pick<Actions, "focusTile" | "nextTilePage" | "previousTilePage" | "toggleTileMaximize">;
  terminals: TerminalManager;
}

/**
 * タイル表示(FR-051〜053・FR-060・FR-061、NFR-009。設計書 §8「タイル表示」、§9.1 の Container)。
 * 上にタブバーの位置の帯(左にターミナルの数、右端にページ表示)、下に表示中のページのタイルのグリッド。
 *
 * - グリッドの列×行とタイルの並びは写しの display(Rust が計算したもの)をそのまま使う。規則をここに持たない
 * - 表示中のページのタイルの置き場所だけを描く。ほかのページのターミナルは TerminalManager が隠したまま出力を取り込み続け
 *   (FR-052)、置き場所は DOM に残さない(大きさ 0 の置き場所で寸法を測らないため。TerminalManager.present の約束)
 * - 最後の行が埋まらないとき(3 枚で 2×2 など)は、タイルを引き伸ばさず空きを残す(【仮定】。タイルの大きさをそろえる)
 * - タイルのどこを押しても(タイトルバーでもターミナルの本文でも)フォーカスのコマンドを送る。本文の押下は xterm.js が
 *   文字の選択に使うので、捕捉の段階で見るだけにして止めない。フォーカス中のタイルなら actions.focusTile が何もしない
 * - クリック以外(Tab キーなど)でタイルに入力のフォーカスが入ったときも、捕捉の段階でフォーカスのコマンドを送る
 *   (M4 のレビューの S2。キーの行き先と、Rust のフォーカス中のタイル=輪・ツリー・⌘W の対象・アテンションの判定をそろえる)。
 *   TerminalArea が入力のフォーカスを移す先はいつもフォーカス中のタイルなので、そのときは focusTile が何もしない。
 *   押下のあとに xterm.js が入力欄へ移すフォーカスでも呼ぶが、写しが届くまで同じタイルは送り直さない(actions.focusTile)
 * - タイトルバーのダブルクリックで、そのタイルの ID を渡して拡大を求める(FR-054、計画【仮定】8、M4 のレビューの S3)。
 *   Rust が「そのタイルにフォーカスしてから拡大」を 1 回で行うので、フォーカスのコマンドの成否や届く順によらない
 * - アテンションの印はタイトルバーの右端と、ページ送りの前後のボタン(FR-055)
 * - 各タイルのターミナルの置き場所の右上に、表示中のページの中の番号の札「#1」〜を控えめに重ねる(⌘1〜9 の番号。ADR-014 の 4)。
 *   キーの表示ではないので表から引かない。クリックは通し(pointer-events: none)、読み上げない(タイトルで足りる)。
 *   拡大中(MaximizedTile)とタブ表示では描かない
 * - 入力のフォーカスの移し替えは TerminalArea が行う(フォーカス中のタイルが替わったとき)
 */
export function TileGrid({ view, store, ui, actions, terminals }: TileGridProps) {
  const total = useStore(store, (s) => s.snapshot?.terminals.length ?? 0);
  const shortcuts = useStore(ui, (s) => s.shortcuts);
  const { cols, rows } = view.grid;
  const gridStyle = { "--tile-cols": cols, "--tile-rows": rows } as CSSProperties;
  const single = view.tiles.length === 1;

  return (
    <>
      <div className="tile-strip" data-testid="tile-strip">
        <p className="tile-strip__summary">{total} 個のターミナル</p>
        {view.paging !== null && (
          <PageControl
            paging={view.paging}
            shortcuts={shortcuts}
            onPrevious={() => void actions.previousTilePage()}
            onNext={() => void actions.nextTilePage()}
          />
        )}
      </div>
      <div
        className={single ? "tile-grid tile-grid--single" : "tile-grid"}
        style={gridStyle}
        data-testid="tile-grid"
        data-cols={cols}
        data-rows={rows}
      >
        {view.tiles.map((item, index) => (
          <Tile
            key={item.id}
            item={item}
            lastCol={index % cols === cols - 1}
            lastRow={Math.floor(index / cols) === rows - 1}
            terminals={terminals}
            onFocus={() => void actions.focusTile(item.id)}
            onMaximize={() => void actions.toggleTileMaximize(item.id)}
          />
        ))}
      </div>
    </>
  );
}

interface TileProps {
  item: TileItem;
  /** 右端の列・下端の行か(タイルの間の区切り線を、外周には引かない)。 */
  lastCol: boolean;
  lastRow: boolean;
  terminals: TerminalManager;
  onFocus: () => void;
  onMaximize: () => void;
}

/** タイル 1 枚: タイトルバー、状態(終了・起動の失敗)、ターミナルの置き場所。 */
function Tile({ item, lastCol, lastRow, terminals, onFocus, onMaximize }: TileProps) {
  const status = describeTerminalStatus(item.status);
  const onPointerDownCapture = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button === 0) onFocus();
  };
  const onFocusCapture = (_event: FocusEvent<HTMLDivElement>) => onFocus();
  const className = ["tile", item.focused && "tile--focused", lastCol && "tile--last-col", lastRow && "tile--last-row"]
    .filter(Boolean)
    .join(" ");

  return (
    <div
      role="group"
      className={className}
      aria-label={item.title}
      aria-current={item.focused ? "true" : undefined}
      data-testid="tile"
      data-tile-id={item.id}
      onPointerDownCapture={onPointerDownCapture}
      onFocusCapture={onFocusCapture}
    >
      <TileTitleBar
        title={item.title}
        color={item.color}
        focused={item.focused}
        attention={item.attention}
        onDoubleClick={onMaximize}
      />
      <div className="tile__body">
        {status !== null && (
          <p role="status" className="terminal-status tile__status" data-testid="terminal-status">
            {status}
          </p>
        )}
        <TerminalHost manager={terminals} terminalId={item.id} />
        {item.position > 0 && (
          <span className="tile__number" aria-hidden="true" data-testid="tile-number">
            #{item.position}
          </span>
        )}
      </div>
    </div>
  );
}

interface MaximizedTileProps {
  /** 拡大しているタイル(tileView の focusedTile。拡大しているのはフォーカス中のタイル)。 */
  item: TileItem;
  ui: UiStore;
  actions: Pick<Actions, "toggleTileMaximize">;
  terminals: TerminalManager;
}

/**
 * 拡大中の右ペイン(FR-054。設計書 §8「タイルからの拡大」、前提 (d)、計画【仮定】7・8)。タブ表示と同じく 1 枚を出し、
 * タブバーの位置に、タイルの帯と同じ高さの帯を置く: 拡大しているタイルの色の印と「プロジェクト名: タブ名」、右端に元に戻す
 * アイコンのボタン。タブバーとページ送りは出さない。
 *
 * - 元に戻すのは、ボタン・帯のダブルクリック・同じショートカット(メニューの項目。Rust が受ける)
 * - ボタンをダブルクリックしても 1 回だけ戻す(2 回目の click と dblclick を捨てる。戻したあとにもう一度拡大しない)
 * - ツールチップは「タイル表示に戻す(⇧⌘↩)」の形で、キーは表の view.toggle-maximize から引く(前提 (f))
 * - 拡大中にフォーカス中のタイルが替わると(カードの選択・タブの切替)、帯の名前と置き場所がそのタイルに替わる
 */
export function MaximizedTile({ item, ui, actions, terminals }: MaximizedTileProps) {
  const shortcuts = useStore(ui, (s) => s.shortcuts);
  const key = shortcutLabel(shortcuts, "view.toggle-maximize");
  const status = describeTerminalStatus(item.status);
  const restore = () => void actions.toggleTileMaximize();
  const onStripDoubleClick = (event: MouseEvent<HTMLDivElement>) => {
    if ((event.target as Element).closest("button") !== null) return;
    restore();
  };
  const onRestoreClick = (event: MouseEvent<HTMLButtonElement>) => {
    if (event.detail > 1) return;
    restore();
  };

  return (
    <>
      <div
        className="tile-strip tile-strip--maximized"
        data-testid="maximized-strip"
        title={item.title}
        onDoubleClick={onStripDoubleClick}
      >
        {item.color !== null && (
          <span
            className="project-colored project-dot tile-title__dot"
            style={projectColorStyle(item.color)}
            aria-hidden="true"
          />
        )}
        <p className="tile-strip__title" data-testid="maximized-title">
          {item.title}
        </p>
        <button
          type="button"
          className="icon-button tile-strip__restore"
          aria-label="タイル表示に戻す"
          title={key === null ? "タイル表示に戻す" : `タイル表示に戻す(${key})`}
          onClick={onRestoreClick}
        >
          <RestoreIcon />
        </button>
      </div>
      <div className="terminal-area__panel" role="group" aria-label={item.title} data-testid="maximized-panel">
        {status !== null && (
          <p role="status" className="terminal-status" data-testid="terminal-status">
            {status}
          </p>
        )}
        {/* ターミナルごとに置き場所を作り直す(要素の付け外しは TerminalManager が行う)。 */}
        <TerminalHost key={item.id} manager={terminals} terminalId={item.id} />
      </div>
    </>
  );
}
