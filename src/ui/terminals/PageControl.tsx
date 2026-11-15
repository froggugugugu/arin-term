import { type ShortcutCommandId, type ShortcutTable, shortcutLabel } from "../../logic/keys/shortcutTable";
import type { TilePaging } from "../../state/selectors";
import { ChevronLeftIcon, ChevronRightIcon } from "../tree/icons";

interface PageControlProps {
  /** ページ表示(`1/2`、前後に進めるか)。tileView の paging。 */
  paging: TilePaging;
  /** ショートカットの表(ツールチップのキーを引く。部品にキーの文字列を書かない)。 */
  shortcuts: ShortcutTable;
  onPrevious: () => void;
  onNext: () => void;
}

/** ツールチップの文言(メニューの項目と同じ文言)と、表から引いたキー。キーが無ければ文言だけ。 */
function tooltip(text: string, shortcuts: ShortcutTable, command: ShortcutCommandId): string {
  const key = shortcutLabel(shortcuts, command);
  return key === null ? text : `${text}(${key})`;
}

/**
 * ページ表示(FR-052、AC-M14。設計書 §8「タイル表示」、§9.1 の Presentational)。タイル表示の帯の右端に置く。
 *
 * - 線画の「‹」「›」のアイコンのボタンと、その間の `1/2`。ページが 1 つのときは親が出さない(【仮定】3)
 * - 端のボタンは無効(薄く)。循環しない
 * - ツールチップは「前のページ(⌥⌘←)」の形。キーは受け取った表から引く(前提 (f))
 * - T47g で、前後のページのアテンションの印(paging.previousAttention・nextAttention)をボタンに足す
 */
export function PageControl({ paging, shortcuts, onPrevious, onNext }: PageControlProps) {
  return (
    <div className="page-control" role="group" aria-label="ページ" data-testid="tile-paging">
      <button
        type="button"
        className="icon-button page-control__button"
        aria-label="前のページ"
        title={tooltip("前のページ", shortcuts, "view.previous-page")}
        disabled={!paging.canGoPrevious}
        onClick={onPrevious}
      >
        <ChevronLeftIcon />
      </button>
      <span className="page-control__label" data-testid="tile-page-label">
        {paging.label}
      </span>
      <button
        type="button"
        className="icon-button page-control__button"
        aria-label="次のページ"
        title={tooltip("次のページ", shortcuts, "view.next-page")}
        disabled={!paging.canGoNext}
        onClick={onNext}
      >
        <ChevronRightIcon />
      </button>
    </div>
  );
}
