import "./TileGrid.css";

import type { CSSProperties, PointerEvent } from "react";
import { useStore } from "zustand";

import type { Actions } from "../../state/actions";
import type { AppStore } from "../../state/appStore";
import type { TileItem, TileView } from "../../state/selectors";
import { describeTerminalStatus } from "../../state/terminalStatus";
import type { UiStore } from "../../state/uiStore";
import type { TerminalManager } from "../../terminal/TerminalManager";
import { PageControl } from "./PageControl";
import { TerminalHost } from "./TerminalHost";
import { TileTitleBar } from "./TileTitleBar";

interface TileGridProps {
  /** 右ペインの表示(tileView の結果。タイル表示で拡大中でないとき)。 */
  view: TileView;
  store: AppStore;
  ui: UiStore;
  actions: Pick<Actions, "focusTile" | "nextTilePage" | "previousTilePage">;
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
 * - T47g で、入力のフォーカスの移し替え・タイトルバーのダブルクリックの拡大・アテンションの印を足す
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
}

/** タイル 1 枚: タイトルバー、状態(終了・起動の失敗)、ターミナルの置き場所。 */
function Tile({ item, lastCol, lastRow, terminals, onFocus }: TileProps) {
  const status = describeTerminalStatus(item.status);
  const onPointerDownCapture = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button === 0) onFocus();
  };
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
    >
      <TileTitleBar title={item.title} color={item.color} focused={item.focused} />
      <div className="tile__body">
        {status !== null && (
          <p role="status" className="terminal-status tile__status" data-testid="terminal-status">
            {status}
          </p>
        )}
        <TerminalHost manager={terminals} terminalId={item.id} />
      </div>
    </div>
  );
}
