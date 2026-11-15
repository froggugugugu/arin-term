import { type ShortcutCommandId, type ShortcutTable, shortcutLabel } from "../../logic/keys/shortcutTable";
import type { TilePaging } from "../../state/selectors";
import { ChevronLeftIcon, ChevronRightIcon } from "../tree/icons";
import { ATTENTION_LABEL } from "./TileTitleBar";

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

/** ボタンの読み上げの名前(その方向にアテンションがあれば、それも伝える)。 */
function buttonLabel(text: string, attention: boolean): string {
  return attention ? `${text}(${ATTENTION_LABEL})` : text;
}

/** ボタンに重ねるアテンションの小さな丸(飾り。意味はボタンの名前が伝える)。 */
function AttentionBadge() {
  return <span className="attention-dot page-control__attention" aria-hidden="true" data-testid="page-attention" />;
}

/**
 * ページ表示(FR-052、AC-M14。設計書 §8「タイル表示」、§9.1 の Presentational)。タイル表示の帯の右端に置く。
 *
 * - 線画の「‹」「›」のアイコンのボタンと、その間の `1/2`。ページが 1 つのときは親が出さない(【仮定】3)
 * - 端のボタンは無効(薄く)。循環しない
 * - ツールチップは「前のページ(⌥⌘←)」の形。キーは受け取った表から引く(前提 (f))
 * - その方向のページにアテンションがあるときだけ、ボタンの右上に小さな丸を重ねる(FR-055。タイトルバーの印と同じ色と形)。
 *   読み上げはボタンの名前に「新しい通知があります」を足す(色だけに頼らない)
 */
export function PageControl({ paging, shortcuts, onPrevious, onNext }: PageControlProps) {
  return (
    <div className="page-control" role="group" aria-label="ページ" data-testid="tile-paging">
      <button
        type="button"
        className="icon-button page-control__button"
        aria-label={buttonLabel("前のページ", paging.previousAttention)}
        title={tooltip("前のページ", shortcuts, "view.previous-page")}
        disabled={!paging.canGoPrevious}
        data-attention={paging.previousAttention ? "true" : undefined}
        onClick={onPrevious}
      >
        <ChevronLeftIcon />
        {paging.previousAttention && <AttentionBadge />}
      </button>
      <span className="page-control__label" data-testid="tile-page-label">
        {paging.label}
      </span>
      <button
        type="button"
        className="icon-button page-control__button"
        aria-label={buttonLabel("次のページ", paging.nextAttention)}
        title={tooltip("次のページ", shortcuts, "view.next-page")}
        disabled={!paging.canGoNext}
        data-attention={paging.nextAttention ? "true" : undefined}
        onClick={onNext}
      >
        <ChevronRightIcon />
        {paging.nextAttention && <AttentionBadge />}
      </button>
    </div>
  );
}
