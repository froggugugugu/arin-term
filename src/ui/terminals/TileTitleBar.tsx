import { projectColorStyle } from "../design/palette";
import type { ProjectColor } from "../snapshotTypes";

interface TileTitleBarProps {
  /** 「プロジェクト名: タブ名」(FR-061)。信頼しない文字列なので、文字として差し込む(設計書 §12)。 */
  title: string;
  /** プロジェクト色(FR-060。小さな丸い印)。持ち主の無いターミナルは null(印を出さない)。 */
  color: ProjectColor | null;
  /** フォーカス中のタイルか(文字を濃く太くし、地を明るく浮かせる)。 */
  focused: boolean;
}

/**
 * タイルのタイトルバー(FR-060・FR-061。設計書 §8「タイル表示」、§9.1 の Presentational)。値だけを受け取る。
 *
 * - カード・タブと同じプロジェクト色の小さな丸い印と、「プロジェクト名: タブ名」。地はプロジェクト色で塗らない
 * - 長い名前は末尾を「…」で省略し、全文はツールチップ
 * - クリック(フォーカス)はタイル全体(TileGrid)が捕捉の段階で受ける。ここではイベントを持たない
 * - T47g で、ダブルクリックの拡大(onDoubleClick)と、右端のアテンションの印(attention)をここに足す
 */
export function TileTitleBar({ title, color, focused }: TileTitleBarProps) {
  return (
    <div
      className={focused ? "tile-title tile-title--focused" : "tile-title"}
      data-testid="tile-title"
      title={title}
    >
      {color !== null && (
        <span
          className="project-colored project-dot tile-title__dot"
          style={projectColorStyle(color)}
          aria-hidden="true"
        />
      )}
      <span className="tile-title__label">{title}</span>
    </div>
  );
}
