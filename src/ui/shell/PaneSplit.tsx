import { type ReactNode, useLayoutEffect, useRef, useState } from "react";

import {
  cardAreaHeightDuringDrag,
  computePaneLayout,
  MIN_CARD_AREA_HEIGHT,
  MIN_SIDEBAR_WIDTH,
  MIN_TERMINAL_WIDTH,
  MIN_TREE_AREA_HEIGHT,
  type PaneArea,
  sidebarWidthDuringDrag,
} from "../../logic/panes/paneLayout";
import { Divider } from "./Divider";
import type { PaneSizesControl } from "./usePaneSizes";

interface PaneSplitProps {
  panes: PaneSizesControl;
  /** 左ペインの上段(カード一覧)。 */
  cards: ReactNode;
  /** 左ペインの下段(ツリーのヘッダと領域)。 */
  tree: ReactNode;
  /** 右ペイン(ターミナル領域)。 */
  terminal: ReactNode;
}

/**
 * 測る前の大きさ。測るまではペインを描かない(測ってすぐ描き直すので画面には出ない)。
 * 仮の大きさで描くと、ターミナルが仮の寸法で接続し、すぐに寸法を送り直すことになるため。
 */
const UNMEASURED: PaneArea = { width: 0, height: 0 };

/**
 * ウィンドウ内の分割(FR-009、設計書 §9.1)。左ペイン(上段: カード、下段: ツリー)と右ペイン(ターミナル)。
 * 表示値は src/logic/panes の computePaneLayout で決め、どのペインも最小寸法を下回らない。
 * 境界はドラッグ(離したときに 1 回送る)とキーボードの矢印で動かせる。折りたたみ中は左ペインと左右の境界を出さない。
 * 左ペインの幅とカード領域の高さだけを表示値で書き、右ペインとツリー領域は残りを CSS(flex)で埋める。
 * ウィンドウの大きさが変わったとき、ターミナルの置き場所が描き直しを待たずに 1 回で新しい大きさになるようにするため
 * (幅と高さが別々に変わると、ターミナルが途中の寸法を送ることがある)。
 */
export function PaneSplit({ panes, cards, tree, terminal }: PaneSplitProps) {
  const bodyRef = useRef<HTMLDivElement>(null);
  const [area, setArea] = useState<PaneArea>(UNMEASURED);

  useLayoutEffect(() => {
    const body = bodyRef.current;
    if (body === null) return;
    const measure = () => {
      // 整数のピクセルに丸める(送る寸法を整数にするため。はみ出す 1px 未満は枠の overflow: hidden で切れる)。
      const rect = body.getBoundingClientRect();
      const width = Math.round(rect.width);
      const height = Math.round(rect.height);
      setArea((prev) => (prev !== UNMEASURED && prev.width === width && prev.height === height ? prev : { width, height }));
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(body);
    return () => observer.disconnect();
  }, []);

  const { sizes } = panes;
  const layout = computePaneLayout(area, sizes);
  const collapsed = sizes.sidebarCollapsed;
  const measured = area !== UNMEASURED;

  return (
    <div ref={bodyRef} className="pane-split" data-testid="pane-split">
      {measured && !collapsed && (
        <aside
          id="pane-sidebar"
          className="pane-sidebar"
          data-testid="pane-sidebar"
          aria-label="プロジェクト"
          style={{ width: layout.sidebarWidth }}
        >
          <div id="card-area" className="card-area" data-testid="card-area" style={{ height: layout.cardAreaHeight }}>
            {cards}
          </div>
          <Divider
            orientation="horizontal"
            label="カード一覧とツリーの境界"
            testId="divider-horizontal"
            controls="card-area"
            value={layout.cardAreaHeight}
            min={MIN_CARD_AREA_HEIGHT}
            max={Math.max(MIN_CARD_AREA_HEIGHT, area.height - MIN_TREE_AREA_HEIGHT)}
            valueDuringDrag={(drag, y) => cardAreaHeightDuringDrag(drag, y, area.height)}
            onPreview={(value) => panes.preview(value === null ? null : { cardAreaHeight: value })}
            onCommit={(value) => panes.commit(sizes.sidebarWidth, value)}
            style={{ top: layout.cardAreaHeight }}
          />
          <div className="tree-area" data-testid="tree-area" style={{ minHeight: MIN_TREE_AREA_HEIGHT }}>
            {tree}
          </div>
        </aside>
      )}
      {measured && !collapsed && (
        <Divider
          orientation="vertical"
          label="左右のペインの境界"
          testId="divider-vertical"
          controls="pane-sidebar"
          value={layout.sidebarWidth}
          min={MIN_SIDEBAR_WIDTH}
          max={Math.max(MIN_SIDEBAR_WIDTH, area.width - MIN_TERMINAL_WIDTH)}
          valueDuringDrag={(drag, x) => sidebarWidthDuringDrag(drag, x, area.width)}
          onPreview={(value) => panes.preview(value === null ? null : { sidebarWidth: value })}
          onCommit={(value) => panes.commit(value, sizes.cardAreaHeight)}
          style={{ left: layout.sidebarWidth }}
        />
      )}
      {measured && (
        <section
          className="pane-terminal"
          data-testid="pane-terminal"
          aria-label="ターミナル"
          style={{ minWidth: MIN_TERMINAL_WIDTH }}
        >
          {terminal}
        </section>
      )}
    </div>
  );
}
