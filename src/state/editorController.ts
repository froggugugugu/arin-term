// エディタの操作と写しへの追従(M4.8。ADR-015 の 3〜6、AC-E01・E05・E06・E07)。main.tsx が作り、部品へ渡す。
//
// - 部品(EditorManager)は形(EditorPort)だけを知る(State は src/editor の部品に依存しない。src/state/terminalSync.ts と同じ)
// - 開く: open_document の応答の本文で部品を作り、作れたら置き場所にフォーカスを求める。開けない理由はツリーの知らせに出す
// - 未保存: 部品が変わり目だけ知らせ、ここが set_document_dirty で Rust へ送る。保存に成功すると Rust が印を消すので、
//   保存のあいだに打った分は set_document_dirty(true) で送り直す
// - 保存: changed-outside・missing は保存の衝突の確認を開く。読み取り専用・大きすぎる・失敗はエディタの帯に出す
// - 閉じる: × は close_editor(discard: false)。未保存なら Rust が needs-confirmation を返す(⌘W は Rust が
//   editor-close-requested を送る)ので、閉じる確認を開く。「保存」は保存できたら閉じる(衝突の確認を挟んでもよい)
// - 外での変更: 選択中のプロジェクトは tree-changed(開いている文書の親フォルダか全部読み直し)、ほかはタブを表示に戻したとき・
//   ウィンドウが前面に戻ったときに check_document で確かめる。未保存が無ければ本文ごと受け取って読み直し、あれば帯で知らせる
// - 写しへの追従(syncEditors): 消えたタブの部品と表示の状態を片付け、文字の大きさ(ターミナルの文字の大きさ)・Vim・読み取り専用を当てる
//
// どの操作も Promise を失敗させない(失敗は reportError に 1 行で報告する。actions.ts と同じ)。

import type { ArinIpc } from "../ipc/client";
import type { AppSnapshot } from "../ipc/generated/AppSnapshot";
import type { DocumentNotAllowedReason } from "../ipc/generated/DocumentNotAllowedReason";
import type { EditorId } from "../ipc/generated/EditorId";
import type { LineEnding } from "../ipc/generated/LineEnding";
import type { ProjectId } from "../ipc/generated/ProjectId";
import type { RelPath } from "../ipc/generated/RelPath";
import type { TreeChange } from "../ipc/generated/TreeChange";
import { describeError } from "../logic/describeError";
import { notFoundMessage } from "./actions";
import type { AppStore } from "./appStore";
import { editorParentDirs } from "./selectors";
import type { EditorViewMode, UiStore } from "./uiStore";

/** エディタの部品の管理のうち、ここで使う形(src/editor/EditorManager.ts はこの形を満たす)。 */
export interface EditorPort {
  open(id: EditorId, document: { text: string; lineEnding: LineEnding; readOnly: boolean }): Promise<void>;
  has(id: EditorId): boolean;
  text(id: EditorId): string | null;
  isDirty(id: EditorId): boolean;
  markSaved(id: EditorId, savedText: string): void;
  replaceText(id: EditorId, text: string, lineEnding: LineEnding): void;
  setReadOnly(id: EditorId, readOnly: boolean): void;
  setFontSize(px: number): void;
  setVim(enabled: boolean): void;
  retain(ids: ReadonlySet<EditorId>): void;
}

/** ここが使う Rust の操作(ArinIpc の一部)。 */
export type EditorBackend = Pick<
  ArinIpc,
  "openDocument" | "saveDocument" | "checkDocument" | "setDocumentDirty" | "selectEditor" | "closeEditor" | "setEditorVim"
>;

export interface EditorControllerDeps {
  ipc: EditorBackend;
  app: AppStore;
  ui: UiStore;
  /** 部品の管理。部品の知らせ(未保存・保存のキー)をここへつなぐため、作る順の都合で関数で受け取る。 */
  editors: () => EditorPort;
  reportError: (message: string) => void;
}

/** 閉じる確認の選択。 */
export type UnsavedCloseChoice = "save" | "discard" | "cancel";
/** 保存の衝突の確認の選択(`create` は消えていたファイルを同じ名前で作る)。 */
export type SaveConflictChoice = "overwrite" | "reload" | "create" | "cancel";

/** 画面のエディタの操作。どの Promise も失敗しない(失敗は報告済み)。 */
export interface EditorController {
  /** `.md`・`.markdown` をエディタのタブで開く(actions.openEntry が回す)。 */
  openDocument(projectId: ProjectId, path: RelPath): Promise<void>;
  /** エディタのタブを表示する(タブのクリック)。 */
  selectEditor(id: EditorId): Promise<void>;
  /** タブの閉じるボタン。未保存なら閉じる確認を開く。 */
  requestClose(id: EditorId): Promise<void>;
  /** 保存する(⌘S・保存のボタン・Vim の `:w`)。 */
  save(id: EditorId): Promise<void>;
  /** 閉じる確認で選んだ。確認を閉じてから行う。確認が無ければ何もしない。 */
  resolveUnsavedClose(choice: UnsavedCloseChoice): Promise<void>;
  /** 保存の衝突の確認で選んだ。確認を閉じてから行う。確認が無ければ何もしない。 */
  resolveSaveConflict(choice: SaveConflictChoice): Promise<void>;
  /** タブの表示の切替(「編集」「並べて」「プレビュー」)。Rust には送らない。 */
  setView(id: EditorId, mode: EditorViewMode): void;
  /** Vim のキー操作の切替(設定画面)。結果は写しの editor.vim で届く。 */
  setVim(enabled: boolean): Promise<void>;
  /** 外で変わったかを確かめる。部品が無ければ何もしない。 */
  check(id: EditorId): Promise<void>;
  /** 開いている文書をすべて確かめる(ウィンドウが前面に戻った)。 */
  checkAll(): void;
  /** tree-changed(選択中のプロジェクト)。開いている文書の親フォルダが変わった(か全部読み直し)なら確かめる。 */
  handleTreeChange(change: TreeChange): void;
  /** ⌘W で表示中のエディタが未保存だった(editor-close-requested)。閉じる確認を開く。 */
  handleCloseRequested(id: EditorId): void;
  /** 部品が未保存の変わり目を知らせた(EditorManager の onDirtyChange)。 */
  handleDirtyChange(id: EditorId, dirty: boolean): void;
}

/** 開かない理由の文言(ツリーの知らせ。AC-E02)。 */
const NOT_ALLOWED: Readonly<Record<DocumentNotAllowedReason, string>> = {
  "not-markdown": "エディタで開けるのは Markdown(.md・.markdown)のファイルだけです",
  link: "シンボリックリンクはエディタで開けません",
  directory: "フォルダはエディタで開けません",
  "too-large": "1 MiB を超えるファイルはエディタで開けません",
  "not-text": "UTF-8 の文字として読めないファイルはエディタで開けません",
  "outside-root": "プロジェクトのフォルダの外のファイルは開けません",
};

const READ_ONLY_SAVE = "読み取り専用のため保存できません";
const TOO_LARGE_SAVE = "1 MiB を超えるため保存できません";
const RELOAD_FAILED = "読み直せませんでした(大きすぎるか、文字として読めません)";

export function createEditorController({ ipc, app, ui, editors, reportError }: EditorControllerDeps): EditorController {
  /** 保存の途中のエディタ(続けて押しても 1 回だけ送る)。 */
  const saving = new Set<EditorId>();
  /** 閉じる確認の「保存」を選んだエディタ(保存できたら閉じる。衝突の確認を挟んでも続ける)。 */
  const closeAfterSave = new Set<EditorId>();

  const run = async (what: string, request: () => Promise<unknown>): Promise<void> => {
    try {
      await request();
    } catch (cause) {
      reportError(`${what}: ${describeError(cause)}`);
    }
  };
  const banner = ui.getState().setEditorBanner;

  const close = async (id: EditorId, discard: boolean) => {
    const outcome = await ipc.closeEditor(id, discard);
    if (outcome.kind === "needs-confirmation") ui.getState().openUnsavedClose(id);
  };

  /** 保存する。overwrite は「上書き保存」「作る」を選んだあと(外の変更・消えたファイルを置き換える・作る)。 */
  const save = (id: EditorId, overwrite: boolean) =>
    run("保存できなかった", async () => {
      const text = editors().text(id);
      if (text === null || saving.has(id)) return;
      saving.add(id);
      try {
        const outcome = await ipc.saveDocument(id, text, overwrite);
        switch (outcome.kind) {
          case "saved": {
            const port = editors();
            port.markSaved(id, text);
            banner(id, null);
            // 保存のあいだに打った分は未保存のまま(Rust は印を消したので送り直す)
            if (port.isDirty(id)) await ipc.setDocumentDirty(id, true);
            if (closeAfterSave.delete(id)) await close(id, false);
            return;
          }
          case "changed-outside":
            ui.getState().openSaveConflict({ editorId: id, kind: "changed" });
            return;
          case "missing":
            ui.getState().openSaveConflict({ editorId: id, kind: "missing" });
            return;
          case "read-only":
            closeAfterSave.delete(id);
            banner(id, { kind: "failed", message: READ_ONLY_SAVE });
            return;
          case "too-large":
            closeAfterSave.delete(id);
            banner(id, { kind: "failed", message: TOO_LARGE_SAVE });
            return;
          case "failed":
            closeAfterSave.delete(id);
            banner(id, { kind: "failed", message: outcome.message });
            return;
        }
      } finally {
        saving.delete(id);
      }
    });

  /** 外で変わったかを確かめる。force は「読み直す(変更を捨てる)」(未保存でも読み直す)。 */
  const check = (id: EditorId, force: boolean) =>
    run("ファイルの変更を確かめられなかった", async () => {
      const port = editors();
      if (!port.has(id)) return;
      const outcome = await ipc.checkDocument(id, force || !port.isDirty(id));
      switch (outcome.kind) {
        case "unchanged":
          if (ui.getState().editorBanners.get(id)?.kind !== "failed") banner(id, null);
          return;
        case "changed":
          // 応答を待つあいだに打ち始めていたら読み直さない(本文を受け取っていても捨てる)
          if (outcome.text !== null && (force || !port.isDirty(id))) {
            port.replaceText(id, outcome.text, outcome.lineEnding);
            banner(id, null);
          } else {
            banner(id, force ? { kind: "failed", message: RELOAD_FAILED } : { kind: "changed" });
          }
          return;
        case "missing":
          banner(id, { kind: "missing" });
          return;
      }
    });

  const knownEditors = (): EditorId[] => app.getState().snapshot?.editors.map((e) => e.id) ?? [];

  return {
    openDocument: (projectId, path) =>
      run("ファイルを開けなかった", async () => {
        const outcome = await ipc.openDocument(projectId, path);
        const notice = (message: string) => ui.getState().showTreeNotice(message, projectId);
        switch (outcome.kind) {
          case "opened": {
            const { editorId, text, lineEnding, readOnly } = outcome;
            try {
              await editors().open(editorId, { text, lineEnding, readOnly: readOnly !== null });
            } catch {
              // 部品を読み込めなかった(EditorManager が報告済み)。空のタブを残さない
              await ipc.closeEditor(editorId, true);
              notice("エディタを読み込めませんでした");
              return;
            }
            ui.getState().requestEditorFocus(editorId);
            return;
          }
          case "already-open":
            ui.getState().requestEditorFocus(outcome.editorId);
            return;
          case "not-allowed":
            notice(NOT_ALLOWED[outcome.reason]);
            return;
          case "not-found":
            notice(notFoundMessage(path));
            return;
          case "failed":
            notice(outcome.message);
            return;
        }
      }),
    selectEditor: (id) => run("エディタのタブを選べなかった", () => ipc.selectEditor(id)),
    requestClose: (id) => run("エディタのタブを閉じられなかった", () => close(id, false)),
    save: (id) => save(id, false),
    resolveUnsavedClose: async (choice) => {
      const pending = ui.getState().unsavedClose;
      if (pending === null) return;
      ui.getState().closeUnsavedClose();
      const id = pending.editorId;
      switch (choice) {
        case "save":
          closeAfterSave.add(id);
          await save(id, false);
          return;
        case "discard":
          await run("エディタのタブを閉じられなかった", () => close(id, true));
          return;
        case "cancel":
          ui.getState().requestEditorFocus(id);
          return;
      }
    },
    resolveSaveConflict: async (choice) => {
      const pending = ui.getState().saveConflict;
      if (pending === null) return;
      ui.getState().closeSaveConflict();
      const id = pending.editorId;
      switch (choice) {
        case "overwrite":
        case "create":
          await save(id, true);
          return;
        case "reload":
          closeAfterSave.delete(id);
          await check(id, true);
          ui.getState().requestEditorFocus(id);
          return;
        case "cancel":
          closeAfterSave.delete(id);
          ui.getState().requestEditorFocus(id);
          return;
      }
    },
    setView: (id, mode) => ui.getState().setEditorView(id, mode),
    setVim: (enabled) => run("Vim のキー操作を切り替えられなかった", () => ipc.setEditorVim(enabled)),
    check: (id) => check(id, false),
    checkAll: () => {
      for (const id of knownEditors()) void check(id, false);
    },
    handleTreeChange: (change) => {
      const dirs = new Set(change.dirs);
      for (const [id, parent] of editorParentDirs(app.getState().snapshot, change.projectId)) {
        if (change.fullReload || dirs.has(parent)) void check(id, false);
      }
    },
    handleCloseRequested: (id) => ui.getState().openUnsavedClose(id),
    handleDirtyChange: (id, dirty) =>
      void run("未保存の印を送れなかった", () => ipc.setDocumentDirty(id, dirty)),
  };
}

/** 写しの追従の宛先(EditorManager と uiStore の片付け、表示に戻したときの確認)。 */
export interface EditorSyncDeps {
  app: AppStore;
  ui: UiStore;
  editors: EditorPort;
  controller: Pick<EditorController, "check">;
}

/**
 * 写しをエディタの部品へ伝え続ける(src/state/terminalSync.ts にならう)。外す関数を返す。
 * - 写しから消えたタブの部品を破棄し、表示の切替・帯・プレビュー・確認を片付ける
 * - 本文の文字の大きさはターミナルの文字の大きさ(ADR-015 の 7)、Vim は写しの editor.vim、読み取り専用は写しの readOnly
 * - プロジェクトの表示中のエディタが替わった(タブを表示に戻した)ら、外で変わったかを確かめる(ADR-015 の 5)
 */
export function syncEditors({ app, ui, editors, controller }: EditorSyncDeps): () => void {
  let shownBefore = new Map<ProjectId, EditorId | null>();
  const forward = (snapshot: AppSnapshot | null, first: boolean) => {
    if (snapshot === null) return;
    const ids = new Set(snapshot.editors.map((e) => e.id));
    editors.retain(ids);
    ui.getState().retainEditors(ids);
    editors.setFontSize(snapshot.fontSizes.terminal.value);
    editors.setVim(snapshot.editor.vim);
    for (const editor of snapshot.editors) editors.setReadOnly(editor.id, editor.readOnly !== null);
    const shown = new Map(snapshot.projects.map((p) => [p.id, p.shownEditorId]));
    if (!first) {
      for (const [projectId, id] of shown) {
        if (id !== null && shownBefore.get(projectId) !== id) void controller.check(id);
      }
    }
    shownBefore = shown;
  };
  forward(app.getState().snapshot, true);
  return app.subscribe((state, previous) => {
    if (state.snapshot !== previous.snapshot) forward(state.snapshot, false);
  });
}
