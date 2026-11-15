import { projectColorStyle } from "../design/palette";
import type { ProjectColor } from "../snapshotTypes";

/** アテンションの印の読み上げ(色だけに頼らない。ページ送りのボタンの名前にも同じ文言を足す)。 */
export const ATTENTION_LABEL = "新しい通知があります";

interface TileTitleBarProps {
  /** 「プロジェクト名: タブ名」(FR-061)。信頼しない文字列なので、文字として差し込む(設計書 §12)。 */
  title: string;
  /** プロジェクト色(FR-060。小さな丸い印)。持ち主の無いターミナルは null(印を出さない)。 */
  color: ProjectColor | null;
  /** フォーカス中のタイルか(文字を濃く太くし、地を明るく浮かせる)。 */
  focused: boolean;
  /** アテンションの印があるか(FR-055。写しの attention)。 */
  attention: boolean;
  /** ダブルクリック(拡大。FR-054、計画【仮定】8)。 */
  onDoubleClick: () => void;
}

/**
 * タイルのタイトルバー(FR-054・FR-055・FR-060・FR-061。設計書 §8「タイル表示」「タイルからの拡大」、§9.1 の Presentational)。
 * 値だけを受け取る。
 *
 * - カード・タブと同じプロジェクト色の小さな丸い印と、「プロジェクト名: タブ名」。地はプロジェクト色で塗らない
 * - 長い名前は末尾を「…」で省略し、全文はツールチップ
 * - アテンションの印は右端の小さな丸(システムの強調色に淡い輪。左のプロジェクト色の印とは位置・色・形で見分ける)。
 *   点滅させない。読み上げは「新しい通知があります」
 * - ダブルクリックで拡大する(ターミナルの本文のダブルクリックは xterm.js の単語の選択なので使わない)
 * - クリック(フォーカス)はタイル全体(TileGrid)が捕捉の段階で受ける。ここでは持たない
 */
export function TileTitleBar({ title, color, focused, attention, onDoubleClick }: TileTitleBarProps) {
  return (
    <div
      className={focused ? "tile-title tile-title--focused" : "tile-title"}
      data-testid="tile-title"
      title={title}
      onDoubleClick={onDoubleClick}
    >
      {color !== null && (
        <span
          className="project-colored project-dot tile-title__dot"
          style={projectColorStyle(color)}
          aria-hidden="true"
        />
      )}
      <span className="tile-title__label">{title}</span>
      {attention && <span className="attention-dot tile-title__attention" role="img" aria-label={ATTENTION_LABEL} />}
    </div>
  );
}
