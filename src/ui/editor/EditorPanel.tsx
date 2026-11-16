import "./EditorPanel.css";

import { useEffect } from "react";
import { useStore } from "zustand";

import type { EditorManager } from "../../editor/EditorManager";
import type { MdBlock } from "../../editor/markdownTypes";
import { ariaKeyShortcuts, shortcutLabel } from "../../logic/keys/shortcutTable";
import type { EditorController } from "../../state/editorController";
import { type EditorBanner, editorViewMode, type UiStore } from "../../state/uiStore";
import type { EditorTab, ReadOnlyReason } from "../snapshotTypes";
import { EditorHost } from "./EditorHost";
import { MarkdownPreview } from "./MarkdownPreview";
import { ViewSwitch } from "./ViewSwitch";

interface EditorPanelProps {
  tab: EditorTab;
  /** 本文の文字の大きさ(ターミナルの文字の大きさ。ADR-015 の 7)。 */
  fontSize: number;
  ui: UiStore;
  editor: Pick<EditorController, "save" | "setView">;
  editors: EditorManager;
  /** tabpanel の id と、それを名づけるタブの要素の id。 */
  panelId: string;
  labelledBy: string;
}

/** 読み取り専用の帯の文言(AC-E02)。 */
const READ_ONLY: Readonly<Record<ReadOnlyReason, string>> = {
  "hard-link": "ほかの名前からも参照されているファイル(ハードリンク)のため、読み取り専用で開いています",
  "not-owner": "持ち主が自分でないファイルのため、読み取り専用で開いています",
  "not-writable": "書き込みの権限が無いため、読み取り専用で開いています",
  "folder-not-writable": "フォルダに書き込めないため、読み取り専用で開いています",
};

/** 外の変更・保存できなかったの帯の文言(ADR-015 の 5)。 */
function bannerText(banner: EditorBanner): string {
  switch (banner.kind) {
    case "changed":
      return "ほかのアプリで変更されました。保存すると、上書きするか確かめます。";
    case "missing":
      return "ファイルが見つかりません。保存すると、同じ名前で作るか確かめます。";
    case "failed":
      return banner.message;
  }
}

const NO_BLOCKS: readonly MdBlock[] = [];

/**
 * エディタのタブの中身(ADR-015 の 8)。上端の細い帯(左に相対パス、右に表示の切替と「保存」)、読み取り専用・外の変更の帯、
 * 本文(「編集」はエディタだけ、「並べて」は左右半分、「プレビュー」はプレビューだけ)。
 * - 「保存」は未保存のときだけ押せる。ツールチップと読み上げのキーは表の editor.save から引く
 * - 地は不透明(ターミナルの透過を当てない)。境は細い線 1 本
 * - プレビューは見えているあいだだけ EditorManager が解析する(setPreviewVisible)
 */
export function EditorPanel({ tab, fontSize, ui, editor, editors, panelId, labelledBy }: EditorPanelProps) {
  const mode = useStore(ui, (s) => editorViewMode(s, tab.id));
  const banner = useStore(ui, (s) => s.editorBanners.get(tab.id) ?? null);
  const blocks = useStore(ui, (s) => s.editorPreviews.get(tab.id) ?? NO_BLOCKS);
  const shortcuts = useStore(ui, (s) => s.shortcuts);
  const saveKey = shortcutLabel(shortcuts, "editor.save");
  const saveAria = ariaKeyShortcuts(shortcuts, "editor.save");
  const previewVisible = mode !== "edit";
  const readOnly = tab.readOnly;

  useEffect(() => {
    editors.setPreviewVisible(tab.id, previewVisible);
    return () => editors.setPreviewVisible(tab.id, false);
  }, [editors, tab.id, previewVisible]);

  return (
    <div
      role="tabpanel"
      id={panelId}
      aria-labelledby={labelledBy}
      className="editor-panel"
      data-testid="editor-panel"
      data-view={mode}
    >
      <div className="editor-panel__bar">
        <span className="editor-panel__path" title={tab.path} data-testid="editor-path">
          {tab.path}
        </span>
        <ViewSwitch value={mode} onChange={(next) => editor.setView(tab.id, next)} />
        <button
          type="button"
          className="button editor-panel__save"
          disabled={!tab.dirty || readOnly !== null}
          aria-keyshortcuts={saveAria ?? undefined}
          title={saveKey === null ? "保存" : `保存(${saveKey})`}
          data-testid="editor-save"
          onClick={() => void editor.save(tab.id)}
        >
          保存
        </button>
      </div>
      {readOnly !== null && (
        <p className="editor-panel__banner" role="status" data-testid="editor-read-only">
          {READ_ONLY[readOnly]}
        </p>
      )}
      {banner !== null && (
        <p
          className="editor-panel__banner editor-panel__banner--notice"
          role={banner.kind === "failed" ? "alert" : "status"}
          data-testid="editor-banner"
          data-kind={banner.kind}
        >
          {bannerText(banner)}
        </p>
      )}
      <div className="editor-panel__body">
        <EditorHost editors={editors} ui={ui} editorId={tab.id} hidden={mode === "preview"} />
        {previewVisible && (
          <section className="editor-panel__preview" aria-label="プレビュー" data-testid="editor-preview">
            <MarkdownPreview blocks={blocks} fontSize={fontSize} />
          </section>
        )}
      </div>
    </div>
  );
}
