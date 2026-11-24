import { useStore } from "zustand";

import type { Actions } from "../../state/actions";
import type { UiStore, UpdateNotice } from "../../state/uiStore";
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

/** 新しい版の確認の知らせの文(ADR-018)。 */
export function updateNoticeText(notice: UpdateNotice): string {
  switch (notice.kind) {
    case "available":
      return `新しい版 v${notice.version} があります`;
    case "upToDate":
      return `Arin v${notice.version} は最新です`;
    case "failed":
      return `新しい版を確かめられませんでした: ${notice.reason}`;
  }
}

/**
 * 新しい版の確認の知らせ(ADR-018)。新しい版があれば「ダウンロードのページを開く」(Releases のページをブラウザで開く。
 * 入れ替えは本人が行う)。メニューから確かめたときは「最新です」「確かめられませんでした」も出す。
 */
export function UpdateNoticeBar({
  ui,
  actions,
}: {
  ui: UiStore;
  actions: Pick<Actions, "openReleasePage" | "dismissUpdateNotice">;
}) {
  const notice = useStore(ui, (s) => s.updateNotice);
  if (notice === null) return null;
  return (
    <NoticeLine
      text={updateNoticeText(notice)}
      testId="update-notice"
      closeLabel="アップデートの知らせを閉じる"
      onDismiss={() => void actions.dismissUpdateNotice()}
      action={
        notice.kind === "available"
          ? { label: "ダウンロードのページを開く", onClick: () => void actions.openReleasePage() }
          : undefined
      }
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
  /** 文の右に置くボタン(任意)。 */
  action?: { label: string; onClick: () => void } | undefined;
}

/** 知らせの 1 行(控えめな地、細い区切り線、右端に閉じるアイコンのボタン)。文は文字として差し込む。 */
function NoticeLine({ text, testId, closeLabel, onDismiss, action }: NoticeLineProps) {
  return (
    <div className="notice-bar" role="status" data-testid={testId}>
      <p className="notice-bar__text" data-testid={`${testId}-text`} title={text}>
        {text}
      </p>
      {action !== undefined && (
        <button type="button" className="button notice-bar__action" data-testid={`${testId}-action`} onClick={action.onClick}>
          {action.label}
        </button>
      )}
      <button type="button" className="icon-button notice-bar__close" aria-label={closeLabel} title="閉じる" onClick={onDismiss}>
        <CloseIcon />
      </button>
    </div>
  );
}
