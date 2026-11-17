import { useStore } from "zustand";

import type { UiStore } from "../../state/uiStore";
import { CloseIcon } from "../tree/icons";

interface NoticeBarProps {
  /** 写しの知らせ(状態ファイルを退避したときの 1 行)。無ければ null。 */
  notice: string | null;
  ui: UiStore;
}

/**
 * 知らせ(写しの `notice`)を 1 行で出す。長い文は省略し、全文はツールチップで読める。閉じるのは右端の × のアイコンのボタン。閉じたら同じ文の知らせを出さない。
 */
export function NoticeBar({ notice, ui }: NoticeBarProps) {
  const dismissed = useStore(ui, (s) => s.dismissedNotice);
  if (notice === null || notice === dismissed) return null;
  return (
    <NoticeLine
      text={notice}
      testId="notice"
      closeLabel="知らせを閉じる"
      onDismiss={() => ui.getState().dismissNotice(notice)}
    />
  );
}

/**
 * 設定の知らせ(get_settings_notice の 1 行。T65e)を、状態の知らせと同じ形の別の 1 行で出す。閉じるのも別。
 * 閉じたあとに違う文の知らせが届けば出し直す(uiStore の settingsNoticeDismissed)。
 */
export function SettingsNoticeBar({ ui }: { ui: UiStore }) {
  const notice = useStore(ui, (s) => s.settingsNotice);
  const dismissed = useStore(ui, (s) => s.settingsNoticeDismissed);
  if (notice === null || dismissed) return null;
  return (
    <NoticeLine
      text={notice}
      testId="settings-notice"
      closeLabel="設定の知らせを閉じる"
      onDismiss={() => ui.getState().dismissSettingsNotice()}
    />
  );
}

interface NoticeLineProps {
  text: string;
  /** 行の data-testid。文は `<testId>-text`。 */
  testId: string;
  /** 閉じるボタンの読み上げの名前。 */
  closeLabel: string;
  onDismiss: () => void;
}

/** 知らせの 1 行(控えめな地、細い区切り線、右端に閉じるアイコンのボタン)。文は文字として差し込む。 */
function NoticeLine({ text, testId, closeLabel, onDismiss }: NoticeLineProps) {
  return (
    <div className="notice-bar" role="status" data-testid={testId}>
      <p className="notice-bar__text" data-testid={`${testId}-text`} title={text}>
        {text}
      </p>
      <button type="button" className="icon-button notice-bar__close" aria-label={closeLabel} title="閉じる" onClick={onDismiss}>
        <CloseIcon />
      </button>
    </div>
  );
}
