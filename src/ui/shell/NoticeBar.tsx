import { useStore } from "zustand";

import type { UiStore } from "../../state/uiStore";

interface NoticeBarProps {
  /** 写しの知らせ(状態ファイルを退避したときの 1 行)。無ければ null。 */
  notice: string | null;
  ui: UiStore;
}

/**
 * 知らせ(写しの `notice`)を 1 行で出す。長い文は省略し、全文はツールチップで読める。閉じたら同じ文の知らせを出さない。
 */
export function NoticeBar({ notice, ui }: NoticeBarProps) {
  const dismissed = useStore(ui, (s) => s.dismissedNotice);
  if (notice === null || notice === dismissed) return null;
  return (
    <div className="notice-bar" role="status" data-testid="notice">
      <p className="notice-bar__text" data-testid="notice-text" title={notice}>
        {notice}
      </p>
      <button
        type="button"
        className="notice-bar__close"
        aria-label="知らせを閉じる"
        onClick={() => ui.getState().dismissNotice(notice)}
      >
        閉じる
      </button>
    </div>
  );
}
