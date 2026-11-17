// 画面の自動テスト用の偽の Rust 側(T11。設計書 §10.3、ADR-006)。
//
// # 差し込み方
//
// Playwright の page.addInitScript で、画面のスクリプトより先に window.__TAURI_INTERNALS__ を置く。
// これは Tauri が本物の WebView に注入する内部の口で、@tauri-apps/api の invoke() はここを呼ぶ
// (node_modules/@tauri-apps/api/core.js の invoke)。そのため画面は src/main.tsx → createTauriIpc()
// (src/ipc/tauri.ts)→ @tauri-apps/api の invoke() という本物の経路をそのまま通り、最後の 1 段だけが偽物になる。
//
// - 製品のソース(src/)には差し込み口を一切置かない。偽物はテストのプロセスから注入するだけなので、
//   `npm run build:web` や `npm run app` のビルド結果に偽物が入る余地が無い(N2: 外部から操作する口を作らない)
// - @tauri-apps/api/mocks の mockIPC も同じ口を置くだけの関数だが、ES モジュールで export を持つため
//   addInitScript(古典的なスクリプトとして実行される)にそのまま渡せない。Arin が使うのは invoke と
//   チャネル(Channel)とイベントの購読(listen)だけなので、必要な部分だけをここに書く
// - チャネル(T19): @tauri-apps/api の Channel は transformCallback で受け取りの関数を登録し、
//   コマンドの引数では `__CHANNEL__:<番号>` の文字列になる。Rust の送り出しと同じく、その番号の関数に
//   `{ message, index }`(index は 0 から順に増える)を渡せば出力が届く。sendChannelMessages がこれを行う
// - イベント(T19): `plugin:event|listen` で登録された受け取りの関数を覚え、emitEvent で
//   Tauri と同じ `{ event, id, payload }` の形で呼ぶ
// - イベントの購読・解除(`plugin:event|listen` など)は Tauri の内部の口なので、記録(__ARIN_E2E_CALLS__)には残さない
// - ツリー(T43b): list_directory はフォルダごとの応答の表(listings。キーは listingKey)を先に引き、無ければ
//   コマンドの既定の応答(空のフォルダ)を返す。外からの変更は表を置き換えてから tree-changed を emitEvent で流す
// - 設定(T65e): get_shortcuts・get_settings_notice の応答を setMockReply で替えてから settings-changed(中身は null)を
//   emitEvent で流すと、本物の Rust の「保存を検知して読み直した」と同じ形になる。設定で一部の行を替えた表は
//   shortcutTableWith で作る(T66a)
//
// # 型の合わせ方
//
// 応答の形は src/ipc/client.ts の CommandMap と、Rust から生成した型(src/ipc/generated/)から作る。
// コマンドが増えたり戻り値の型が変わったりすると、DEFAULT_REPLIES が型検査で通らなくなる
// (ただし、型検査の対象に e2e/ を入れた場合。Playwright はテストを実行するとき型を検査しない)。

import type { Page } from "@playwright/test";

import type { CommandMap, CommandName, EventMap, EventName } from "../../src/ipc/client";
import type { AppSnapshot } from "../../src/ipc/generated/AppSnapshot";
import type { DirEntry } from "../../src/ipc/generated/DirEntry";
import type { DirListing } from "../../src/ipc/generated/DirListing";
import type { DisplayMode } from "../../src/ipc/generated/DisplayMode";
import type { EditorTabSnapshot } from "../../src/ipc/generated/EditorTabSnapshot";
import type { EntryKind } from "../../src/ipc/generated/EntryKind";
import type { ProjectCardSnapshot } from "../../src/ipc/generated/ProjectCardSnapshot";
import type { ProjectColor } from "../../src/ipc/generated/ProjectColor";
import type { RunningTerminal } from "../../src/ipc/generated/RunningTerminal";
import type { ShortcutCommandId } from "../../src/ipc/generated/ShortcutCommandId";
import type { ShortcutTable } from "../../src/ipc/generated/ShortcutTable";
import type { TerminalStatus } from "../../src/ipc/generated/TerminalStatus";
import type { TerminalTabSnapshot } from "../../src/ipc/generated/TerminalTabSnapshot";
import type { TileGrid } from "../../src/ipc/generated/TileGrid";
// 既定の表の見本。生成した src/ipc/generated/defaultShortcuts.json と同じ中身であることを
// scripts/default-shortcuts.test.mjs が確かめる(e2e/ の tsconfig は JSON を対象に含めないので見本を使う)。
import { DEFAULT_SHORTCUT_TABLE } from "../../src/logic/keys/defaultShortcuts";

/**
 * 1 つのコマンドへの偽の応答。ok なら値を返し、error なら Rust のコマンドの失敗と同じく文字列で失敗させる。
 * 画面のページへ JSON として渡すので、JSON にできる値だけにする。
 */
export type MockReply<C extends CommandName> = { ok: CommandMap[C]["result"] } | { error: string };

/** コマンドごとの偽の応答。画面が呼ぶコマンドはすべて埋める(足りなければ型検査で気づける)。 */
export type TauriMockConfig = { [C in CommandName]: MockReply<C> };

/** 画面から偽の Rust 側に届いたコマンド 1 件。 */
export interface RecordedCall {
  command: string;
  args: unknown;
}

/** テスト用の写し。既定はプロジェクト 0 件・ターミナル 0 枚で、ペインの寸法は Rust の初期値と同じ。 */
export function sampleSnapshot(overrides: Partial<AppSnapshot> = {}): AppSnapshot {
  return {
    schemaVersion: 1,
    appVersion: "0.0.0-e2e",
    projects: [],
    selectedProjectId: null,
    terminals: [],
    layout: { sidebarWidth: 260, sidebarCollapsed: false, cardAreaHeight: 200 },
    notice: null,
    // 表示の状態の既定(タブ表示・拡大なし・0 枚)。タイル表示の写しは withTileDisplay で作る
    display: {
      mode: "tabs",
      maximized: false,
      page: 0,
      pageCount: 0,
      grid: { cols: 0, rows: 0 },
      tileIds: [],
      focusedTerminalId: null,
      attentionPages: [],
    },
    // 文字の大きさの既定(Rust の `FontSizes::default()` と範囲。ADR-013 の 1)
    fontSizes: {
      terminal: { value: 15, min: 9, max: 28, defaultValue: 15 },
      interface: { value: 13, min: 11, max: 18, defaultValue: 13 },
    },
    // 透過の既定(Rust の `Translucency::default()` と範囲・刻み。ADR-014 の 1)
    translucency: {
      window: { value: 45, min: 0, max: 100, defaultValue: 45, step: 5 },
      terminal: { value: 20, min: 0, max: 80, defaultValue: 20, step: 5 },
    },
    // エディタのタブは 0 枚、Vim は既定の偽(ADR-015 の 3・6)
    editors: [],
    editor: { vim: false },
    ...overrides,
  };
}

/** テスト用の写しのターミナル 1 枚。既定はプロジェクトを持たない(持ち主の無いタブ。Rust 側は T49d で無くす)。 */
export function sampleTerminal(
  id: string,
  status: TerminalStatus,
  overrides: Partial<TerminalTabSnapshot> = {},
): TerminalTabSnapshot {
  return { id, projectId: null, label: "ターミナル 1", status, attention: false, ...overrides };
}

/** テスト用のプロジェクトの ID(projectSnapshot が使う)。 */
export const SAMPLE_PROJECT_ID = "p-e2e-1";

/**
 * ターミナルを 1 つのプロジェクトのタブとして載せた写し。プロジェクトを選択中にし、最初のタブを表示中にする
 * (画面は選択中のプロジェクトの表示中のタブを出す。T34b)。
 */
export function projectSnapshot(terminals: readonly TerminalTabSnapshot[], overrides: Partial<AppSnapshot> = {}): AppSnapshot {
  return sampleSnapshot({
    projects: [
      {
        id: SAMPLE_PROJECT_ID,
        name: "e2e",
        color: "blue",
        displayPath: "~/e2e",
        missing: false,
        showHidden: false,
        terminalIds: terminals.map((t) => t.id),
        activeTerminalId: terminals[0]?.id ?? null,
        editorIds: [],
        shownEditorId: null,
      },
    ],
    selectedProjectId: SAMPLE_PROJECT_ID,
    terminals: terminals.map((t) => ({ ...t, projectId: SAMPLE_PROJECT_ID })),
    ...overrides,
  });
}

/** テスト用のプロジェクト 1 件(projectsSnapshot に渡す)。`tabs` はタブの表示名で、ターミナルの ID は `<id>-t<番号>`。 */
export interface SampleProject {
  id: string;
  name: string;
  color?: ProjectColor;
  displayPath?: string;
  missing?: boolean;
  /** タブの表示名(タブの順)。既定は見つからないなら 0 枚、それ以外は「ターミナル 1」の 1 枚。 */
  tabs?: readonly string[];
}

/**
 * 複数のプロジェクトを載せた写し(T34c)。既定では最初のプロジェクトを選択中にし、各プロジェクトの最初のタブを表示中にする。
 * ターミナルは動作中で、写しの順(プロジェクトの順、その中はタブの順)に並ぶ。
 */
export function projectsSnapshot(projects: readonly SampleProject[], overrides: Partial<AppSnapshot> = {}): AppSnapshot {
  const cards: ProjectCardSnapshot[] = [];
  const terminals: TerminalTabSnapshot[] = [];
  for (const project of projects) {
    const tabs = project.tabs ?? (project.missing === true ? [] : ["ターミナル 1"]);
    const ids = tabs.map((_, index) => `${project.id}-t${index + 1}`);
    cards.push({
      id: project.id,
      name: project.name,
      color: project.color ?? "blue",
      displayPath: project.displayPath ?? `~/dev/${project.name}`,
      missing: project.missing ?? false,
      showHidden: false,
      terminalIds: ids,
      activeTerminalId: ids[0] ?? null,
      editorIds: [],
      shownEditorId: null,
    });
    tabs.forEach((label, index) => {
      terminals.push({
        id: ids[index] ?? "",
        projectId: project.id,
        label,
        status: { kind: "running" },
        attention: false,
      });
    });
  }
  const snapshot = sampleSnapshot({ projects: cards, selectedProjectId: projects[0]?.id ?? null, terminals, ...overrides });
  if (overrides.display !== undefined) return snapshot;
  // Rust の写しは、タブ表示でもフォーカス中のターミナル(選択中のプロジェクトの表示中のタブ)を載せる
  // (crates/arin-core/src/workspace/display.rs)。空のままだと、画面が「まだフォーカスしていない」と読んでしまう。
  const selected = snapshot.projects.find((p) => p.id === snapshot.selectedProjectId);
  return { ...snapshot, display: { ...snapshot.display, focusedTerminalId: selected?.activeTerminalId ?? null } };
}

// ── 表示(タイル。T47f)。Rust の規則(crates/arin-core/src/layout/tiles.rs と workspace/display.rs)をテスト用に写したもの ──
// 偽の Rust 側は写しを送り返さない(コマンドは記録するだけ)ので、テストが Rust の返す写しをここで作って流す。

/** 1 ページのタイルの最大数(FR-052)。 */
export const TILES_PER_PAGE = 9;

/** 枚数からグリッド(列×行)を決める(FR-051。0→0×0、1→1×1、2→2×1、3〜4→2×2、5〜6→3×2、7〜9→3×3)。 */
export function tileGridFor(count: number): TileGrid {
  if (count <= 0) return { cols: 0, rows: 0 };
  if (count === 1) return { cols: 1, rows: 1 };
  if (count === 2) return { cols: 2, rows: 1 };
  if (count <= 4) return { cols: 2, rows: 2 };
  if (count <= 6) return { cols: 3, rows: 2 };
  return { cols: 3, rows: 3 };
}

/** withTileDisplay の指定。どれも省略できる。 */
export interface TileDisplayOptions {
  /** 表示モード。既定はタイル表示。 */
  mode?: DisplayMode;
  /** 拡大中か。既定は false。 */
  maximized?: boolean;
  /**
   * 表示するページ(0 から)。指定すると、Rust のページ送りと同じく、そのページの先頭のタイルにフォーカスを移す
   * (そのプロジェクトを選び、そのタブを表示中にする。計画【仮定】4)。範囲の外は最後のページに丸める。
   */
  page?: number;
  /** フォーカスするタイル(Rust の focus_terminal と同じく、そのプロジェクトを選び、そのタブを表示中にする)。 */
  focusedTerminalId?: string;
  /**
   * アテンションの契機があったターミナル(T47g)。Rust の規則(計画【仮定】14、ADR-011 の 7)と同じく、
   * フォーカス中のターミナル(どの表示モードでも 1 枚)には印を立てない・消す。タイル表示で見えているだけのタイルには立つ。
   * 省略すると写しのターミナルの attention をそのまま使う(フォーカス中のものは同じく消す)。
   */
  attention?: readonly string[];
}

/**
 * 写しの display を、写しのターミナルの並び(プロジェクトの順 → タブの順)から Rust と同じ規則で作り直した写しを返す。
 * フォーカス中のタイルは選択中のプロジェクトの表示中のタブで、ページは(page を指定しなければ)フォーカス中のタイルのページ。
 * 最終ページのグリッドはそのページの枚数で決める(計画【仮定】2)。渡した写しは書き換えない。
 */
export function withTileDisplay(snapshot: AppSnapshot, options: TileDisplayOptions = {}): AppSnapshot {
  const ids = snapshot.terminals.map((t) => t.id);
  const total = ids.length;
  const pageCount = Math.ceil(total / TILES_PER_PAGE);
  const clampPage = (page: number) => Math.max(0, Math.min(page, pageCount - 1));
  let projects = snapshot.projects;
  let selectedProjectId = snapshot.selectedProjectId;
  const focusOn = (terminalId: string) => {
    const owner = snapshot.terminals.find((t) => t.id === terminalId)?.projectId ?? null;
    if (owner === null) return;
    selectedProjectId = owner;
    projects = projects.map((p) => (p.id === owner ? { ...p, activeTerminalId: terminalId } : p));
  };
  let page: number;
  if (options.page !== undefined) {
    page = clampPage(options.page);
    const first = ids[page * TILES_PER_PAGE];
    if (first !== undefined) focusOn(first);
  } else {
    if (options.focusedTerminalId !== undefined) focusOn(options.focusedTerminalId);
    const focused = projects.find((p) => p.id === selectedProjectId)?.activeTerminalId ?? null;
    const index = focused === null ? -1 : ids.indexOf(focused);
    page = index < 0 ? 0 : clampPage(Math.floor(index / TILES_PER_PAGE));
  }
  if (total === 0) page = 0;
  const tileIds = ids.slice(page * TILES_PER_PAGE, (page + 1) * TILES_PER_PAGE);
  const focusedTerminalId = projects.find((p) => p.id === selectedProjectId)?.activeTerminalId ?? null;
  const mode = options.mode ?? "tiles";
  const maximized = mode === "tiles" && (options.maximized ?? false);
  const marked = options.attention === undefined ? null : new Set(options.attention);
  const terminals = snapshot.terminals.map((t) => {
    const attention = t.id !== focusedTerminalId && (marked === null ? t.attention : marked.has(t.id));
    return attention === t.attention ? t : { ...t, attention };
  });
  const attentionPages = [
    ...new Set(terminals.flatMap((t, i) => (t.attention ? [Math.floor(i / TILES_PER_PAGE)] : []))),
  ];
  return {
    ...snapshot,
    projects,
    selectedProjectId,
    terminals,
    display: {
      mode,
      maximized,
      page,
      pageCount,
      grid: tileGridFor(tileIds.length),
      tileIds,
      focusedTerminalId,
      attentionPages,
    },
  };
}

/**
 * 文字の大きさを替えた写しを返す(T70a。Rust の step_font_size・reset_appearance のあとに届く写しの形)。
 * 範囲と既定は写しのまま(Rust の規則で端に留めるのはテストの側の役目)。渡した写しは書き換えない。
 */
export function withFontSizes(
  snapshot: AppSnapshot,
  sizes: { terminal?: number; interface?: number },
): AppSnapshot {
  const { terminal, interface: ui } = snapshot.fontSizes;
  return {
    ...snapshot,
    fontSizes: {
      terminal: { ...terminal, value: sizes.terminal ?? terminal.value },
      interface: { ...ui, value: sizes.interface ?? ui.value },
    },
  };
}

/**
 * Rust の step_font_size のあとに届く写しを返す(T71。crates/arin-core/src/layout/font_sizes.rs の `stepped` の写し:
 * 今の値に delta を足し、写しの最小・最大の端に留める)。端で変わらないときも同じ形の写しを返す(Rust は送らない)。
 */
export function withSteppedFontSize(
  snapshot: AppSnapshot,
  target: "terminal" | "interface",
  delta: number,
): AppSnapshot {
  const current = snapshot.fontSizes[target];
  const value = Math.max(current.min, Math.min(current.max, current.value + delta));
  return withFontSizes(snapshot, { [target]: value });
}

/**
 * 透過を替えた写しを返す(M4.7。Rust の step_translucency・reset_appearance のあとに届く写しの形)。
 * 範囲・既定・刻みは写しのまま。渡した写しは書き換えない。
 */
export function withTranslucency(
  snapshot: AppSnapshot,
  values: { window?: number; terminal?: number },
): AppSnapshot {
  const { window, terminal } = snapshot.translucency;
  return {
    ...snapshot,
    translucency: {
      window: { ...window, value: values.window ?? window.value },
      terminal: { ...terminal, value: values.terminal ?? terminal.value },
    },
  };
}

/**
 * Rust の step_translucency のあとに届く写しを返す(crates/arin-core/src/layout/translucency.rs の写し:
 * 今の値に delta(% の差分)を足し、刻みの倍数に丸めて、写しの最小・最大の端に留める)。
 */
export function withSteppedTranslucency(
  snapshot: AppSnapshot,
  target: "window" | "terminal",
  delta: number,
): AppSnapshot {
  const current = snapshot.translucency[target];
  const rounded = Math.round((current.value + delta) / current.step) * current.step;
  const value = Math.max(current.min, Math.min(current.max, rounded));
  return withTranslucency(snapshot, { [target]: value });
}

/** テスト用のエディタのタブ 1 枚(M4.8)。表示名はパスのファイル名、未保存なし、書ける。 */
export function sampleEditorTab(
  id: string,
  projectId: string,
  path: string,
  overrides: Partial<EditorTabSnapshot> = {},
): EditorTabSnapshot {
  return { id, projectId, path, label: path.slice(path.lastIndexOf("/") + 1), dirty: false, readOnly: null, ...overrides };
}

/**
 * エディタのタブを載せた写しを返す(M4.8。Rust の open_document・select_editor・close_editor・set_document_dirty の
 * あとに届く写しの形)。projectId のエディタを tabs に置き換え、shownEditorId を表示中にする。表示中にするときは
 * Rust と同じくそのプロジェクトを選び、タブ表示に替える(ADR-015 の 3。【要確認】1 の A)。渡した写しは書き換えない。
 */
export function withEditors(
  snapshot: AppSnapshot,
  projectId: string,
  tabs: readonly EditorTabSnapshot[],
  shownEditorId: string | null = tabs.at(-1)?.id ?? null,
): AppSnapshot {
  const others = snapshot.editors.filter((e) => e.projectId !== projectId);
  const shown = shownEditorId !== null;
  return {
    ...snapshot,
    selectedProjectId: shown ? projectId : snapshot.selectedProjectId,
    projects: snapshot.projects.map((p) =>
      p.id === projectId ? { ...p, editorIds: tabs.map((t) => t.id), shownEditorId } : p,
    ),
    editors: [...others, ...tabs],
    display: shown ? { ...snapshot.display, mode: "tabs", maximized: false } : snapshot.display,
  };
}

/** 終了・削除の確認に載せる動作中のターミナル 1 枚(quit-requested のイベントの中身などに使う。T34d)。 */
export function sampleRunningTerminal(
  projectId: string,
  projectName: string,
  terminalId: string,
  label: string,
): RunningTerminal {
  return { terminalId, projectId, projectName, label };
}

/**
 * 既定の表のうち、keys に書いた行だけキーを替えた表(get_shortcuts の応答。T66a)。本物の Rust が設定ファイルの問題の無い行を
 * 当てた結果の形で、行の順と受け口は既定のまま。問題のある行(既定に戻した行)は keys に書かない。
 */
export function shortcutTableWith(keys: Readonly<Partial<Record<ShortcutCommandId, string>>> = {}): ShortcutTable {
  return DEFAULT_SHORTCUT_TABLE.map((binding) => ({ ...binding, accelerator: keys[binding.command] ?? binding.accelerator }));
}

const DEFAULT_REPLIES: TauriMockConfig = {
  get_app_snapshot: { ok: sampleSnapshot() },
  get_shortcuts: { ok: DEFAULT_SHORTCUT_TABLE.map((binding) => ({ ...binding })) },
  get_settings_notice: { ok: null },
  notify_app_ready: { ok: null },
  report_frontend_error: { ok: null },
  confirm_quit: { ok: null },
  connect_terminal: { ok: { sizeApplied: true } },
  write_terminal: { ok: null },
  ack_terminal_output: { ok: null },
  resize_terminal: { ok: null },
  write_terminal_bytes: { ok: null },
  new_terminal: { ok: "terminal-new" },
  close_terminal: { ok: null },
  select_terminal: { ok: null },
  add_project_by_dialog: { ok: { kind: "cancelled" } },
  select_project: { ok: null },
  remove_project: { ok: { kind: "removed" } },
  relink_project: { ok: false },
  set_project_color: { ok: null },
  copy_project_path: { ok: null },
  set_pane_sizes: { ok: null },
  set_sidebar_collapsed: { ok: null },
  set_display_mode: { ok: null },
  focus_terminal: { ok: null },
  next_tile_page: { ok: null },
  previous_tile_page: { ok: null },
  toggle_tile_maximize: { ok: null },
  // 文字の大きさ(T70a)。偽の Rust 側は写しを送り返さないので、テストが withFontSizes で作った写しを流す
  step_font_size: { ok: null },
  // 透過(M4.7)。同じく、テストが withTranslucency で作った写しを流す
  step_translucency: { ok: null },
  reset_appearance: { ok: null },
  open_settings_file: { ok: null },
  list_directory: { ok: { kind: "listed", entries: [] } },
  set_show_hidden: { ok: null },
  create_entry: { ok: { kind: "done", select: null } },
  rename_entry: { ok: { kind: "done", select: null } },
  move_entry: { ok: { kind: "done", select: null } },
  copy_entry: { ok: { kind: "done", select: null } },
  trash_entry: { ok: { kind: "done", select: null } },
  copy_entries: { ok: { kind: "done", select: null } },
  cut_entries: { ok: { kind: "done", select: null } },
  paste_entries: { ok: { kind: "done", select: null } },
  reveal_entry: { ok: { kind: "done", select: null } },
  open_entry: { ok: { kind: "done", select: null } },
  copy_entry_path: { ok: { kind: "done", select: null } },
  insert_entry_path: { ok: { kind: "done", select: null } },
  // エディタ(M4.8)。偽の Rust 側は写しを送り返さないので、開いたあとはテストが withEditor で作った写しを流す
  open_document: { ok: { kind: "not-found" } },
  save_document: { ok: { kind: "saved" } },
  check_document: { ok: { kind: "unchanged" } },
  adopt_document: { ok: true },
  set_document_dirty: { ok: null },
  select_editor: { ok: null },
  close_editor: { ok: { kind: "closed" } },
  set_editor_vim: { ok: null },
};

/** 画面のページの中で、偽の Rust 側が記録を置く場所(テストの側からだけ読む)。 */
interface MockWindow {
  __TAURI_INTERNALS__?: Record<string, unknown>;
  __TAURI_EVENT_PLUGIN_INTERNALS__?: Record<string, unknown>;
  __ARIN_E2E_CALLS__?: RecordedCall[];
  __ARIN_E2E__?: MockControls;
}

/** 偽の Rust 側から画面へ送るための操作(画面のページの中にある。テストの側から page.evaluate で呼ぶ)。 */
interface MockControls {
  /** チャネルに中身を順に送る(index はチャネルごとに続きから振る)。受け取りの関数が無ければ false。 */
  sendChannel(channelId: number, messages: unknown[]): boolean;
  /** イベントを、購読している受け取りの関数すべてに届ける。届けた数を返す。 */
  emit(event: string, payload: unknown): number;
  /** コマンドの応答を差し替える(以後の呼び出しから効く。T35a)。 */
  setReply(command: string, reply: { ok: unknown } | { error: string }): void;
  /**
   * フォルダごとの list_directory の応答を差し替える(T43b)。"pending" は応答を返さずに待たせ、
   * あとで別の応答を置いたときに、待っていた問い合わせへもその応答を返す。null は差し替えを外す(既定の応答に戻る)。
   */
  setListing(key: string, listing: unknown): void;
}

/** フォルダごとの list_directory の応答。"pending" は、応答を置き直すまで返さない(読み込み中の表示を見るため)。 */
export type MockListing = DirListing | "pending";

/** フォルダごとの応答の表のキー(プロジェクトとルート基準の相対パス。ルートは空の文字列)。 */
export function listingKey(projectId: string, path: string): string {
  return `${projectId}\u0000${path}`;
}

/** 名前と種類の組から、フォルダの中身(listed)を作る。並び順は渡した順のまま(並べ替えは Rust の役目)。 */
export function listed(entries: ReadonlyArray<readonly [string, EntryKind]>): DirListing {
  return { kind: "listed", entries: entries.map(([name, kind]): DirEntry => ({ name, kind })) };
}

/**
 * 画面のページの中で実行する本体。Playwright はこの関数を文字列にして新しい文書の先頭で実行するので、
 * 外の変数や import を参照せず、引数の replies だけを使う(型の import は実行時には消える)。
 */
function injectTauriMock({
  replies,
  listings,
}: {
  replies: Record<string, { ok: unknown } | { error: string }>;
  listings: Record<string, unknown>;
}): void {
  const w = window as unknown as MockWindow;
  // 応答を待たせている list_directory の問い合わせ(キーごと)。
  const waiting = new Map<string, Array<(listing: unknown) => void>>();
  const calls: RecordedCall[] = [];
  w.__ARIN_E2E_CALLS__ = calls;
  // Channel と listen が渡す受け取りの関数(@tauri-apps/api の transformCallback)。
  const callbacks = new Map<number, (payload: unknown) => void>();
  let nextCallback = 1;
  let nextEvent = 1;
  // チャネルごとの次の index(Rust の Channel と同じく 0 から)。
  const channelIndex = new Map<number, number>();
  // イベントの購読: 購読の番号 → { イベント名, 受け取りの関数の番号 }。
  const listeners = new Map<number, { event: string; handler: number }>();
  w.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener: (_event: string, eventId: number) => {
      listeners.delete(eventId);
    },
  };
  w.__ARIN_E2E__ = {
    sendChannel: (channelId, messages) => {
      const callback = callbacks.get(channelId);
      if (callback === undefined) return false;
      for (const message of messages) {
        const index = channelIndex.get(channelId) ?? 0;
        channelIndex.set(channelId, index + 1);
        callback({ message, index });
      }
      return true;
    },
    emit: (event, payload) => {
      let delivered = 0;
      for (const [id, listener] of listeners) {
        if (listener.event !== event) continue;
        callbacks.get(listener.handler)?.({ event, id, payload });
        delivered++;
      }
      return delivered;
    },
    setReply: (command, reply) => {
      replies[command] = reply;
    },
    setListing: (key, listing) => {
      if (listing === null) delete listings[key];
      else listings[key] = listing;
      if (listing === null || listing === "pending") return;
      const resolvers = waiting.get(key) ?? [];
      waiting.delete(key);
      for (const resolve of resolvers) resolve(JSON.parse(JSON.stringify(listing)));
    },
  };
  w.__TAURI_INTERNALS__ = {
    transformCallback: (callback: (payload: unknown) => void): number => {
      const id = nextCallback++;
      callbacks.set(id, callback);
      return id;
    },
    unregisterCallback: (id: number): void => {
      callbacks.delete(id);
    },
    runCallback: (id: number, payload: unknown): void => {
      callbacks.get(id)?.(payload);
    },
    invoke: async (command: string, args: unknown): Promise<unknown> => {
      if (command === "plugin:event|listen") {
        const { event, handler } = args as { event: string; handler: number };
        const id = nextEvent++;
        listeners.set(id, { event, handler });
        return id;
      }
      if (command === "plugin:event|unlisten") {
        listeners.delete((args as { eventId: number }).eventId);
        return null;
      }
      // 画面が渡した引数をそのまま残すと、あとで画面が書き換えたときに記録も変わるので、写しを取る。
      calls.push({ command, args: args === undefined ? undefined : JSON.parse(JSON.stringify(args)) });
      if (command === "list_directory") {
        const { projectId, path } = args as { projectId: string; path: string };
        const key = `${projectId}\u0000${path}`;
        const listing = listings[key];
        if (listing === "pending") {
          return new Promise((resolve) => {
            const resolvers = waiting.get(key) ?? [];
            resolvers.push(resolve);
            waiting.set(key, resolvers);
          });
        }
        if (listing !== undefined) return JSON.parse(JSON.stringify(listing));
      }
      const reply = replies[command];
      if (reply === undefined) {
        // 本物の Tauri も、登録されていないコマンドは文字列で失敗させる。
        throw `command ${command} not found`;
      }
      if ("error" in reply) throw reply.error;
      // 画面が受け取った値を書き換えても、次の応答に影響しないよう、毎回作り直す。
      return JSON.parse(JSON.stringify(reply.ok));
    },
  };
}

/**
 * 偽の Rust 側を差し込む。page.goto の前に呼ぶ。渡さなかったコマンドは既定の応答を返す。
 * 再読み込みのたびに差し込み直され、記録も空から始まる。
 */
export async function installTauriMock(
  page: Page,
  overrides: Partial<TauriMockConfig> = {},
  listings: Readonly<Record<string, MockListing>> = {},
): Promise<void> {
  const replies: TauriMockConfig = { ...DEFAULT_REPLIES, ...overrides };
  await page.addInitScript(injectTauriMock, { replies, listings: { ...listings } });
}

/**
 * 差し込んだあとで、1 つのフォルダの list_directory の応答を差し替える(T43b)。listingKey で作ったキーを渡す。
 * 待たせていた("pending")問い合わせにも、この応答を返す。null は差し替えを外す。
 */
export async function setMockListing(page: Page, key: string, listing: MockListing | null): Promise<void> {
  await page.evaluate(
    ({ key, listing }) => {
      const controls = (window as unknown as MockWindow).__ARIN_E2E__;
      if (controls === undefined) throw new Error("偽の Rust 側が差し込まれていない");
      controls.setListing(key, listing);
    },
    { key, listing },
  );
}

/**
 * 差し込んだあとで、1 つのコマンドの応答を差し替える(T35a)。以後の呼び出しから効き、再読み込みすると
 * installTauriMock に渡した応答へ戻る。同じコマンドが続けて違う結果を返す流れ(削除の確認のあとの確定など)に使う。
 */
export async function setMockReply<C extends CommandName>(page: Page, command: C, reply: MockReply<C>): Promise<void> {
  await page.evaluate(
    ({ command, reply }) => {
      const controls = (window as unknown as MockWindow).__ARIN_E2E__;
      if (controls === undefined) throw new Error("偽の Rust 側が差し込まれていない");
      controls.setReply(command, reply);
    },
    { command, reply },
  );
}

/** 画面から届いたコマンドを、届いた順に返す。 */
export async function readRecordedCalls(page: Page): Promise<RecordedCall[]> {
  return page.evaluate(() => (window as unknown as MockWindow).__ARIN_E2E_CALLS__ ?? []);
}

/** 画面から届いた、指定のコマンドの引数だけを届いた順に返す。 */
export async function readCallArgs<C extends CommandName>(page: Page, command: C): Promise<RecordedArgs<C>[]> {
  const calls = await readRecordedCalls(page);
  return calls.filter((call) => call.command === command).map((call) => call.args as RecordedArgs<C>);
}

/**
 * 記録に残る引数の形。JSON にした写しなので、チャネル(ChannelToken)は `__CHANNEL__:<番号>` の文字列になる。
 */
export type RecordedArgs<C extends CommandName> = {
  [K in keyof NonNullable<CommandMap[C]["args"]>]: K extends "onOutput"
    ? string
    : NonNullable<CommandMap[C]["args"]>[K];
};

/** `__CHANNEL__:<番号>` から番号を取り出す。形が違えば例外。 */
export function parseChannelId(token: string): number {
  const match = /^__CHANNEL__:(\d+)$/.exec(token);
  if (match === null) throw new Error(`チャネルの形ではない: ${token}`);
  return Number(match[1]);
}

/** チャネルで送る 1 回ぶん。文字列は UTF-8 にして送る。数の配列はそのままのバイト列(文字の途中で切った出力を作れる)。 */
export type OutputChunk = string | readonly number[];

/**
 * チャネルへ生のバイト列を順に送る(Tauri が生のバイト列を ArrayBuffer で渡すのと同じ形)。
 * 送ったバイト数の合計を返す。
 */
export async function sendChannelBytes(
  page: Page,
  channelId: number,
  chunks: readonly OutputChunk[],
): Promise<number> {
  return page.evaluate(
    ({ channelId, chunks }) => {
      const encoder = new TextEncoder();
      const buffers = chunks.map((chunk) => {
        const bytes = typeof chunk === "string" ? encoder.encode(chunk) : Uint8Array.from(chunk);
        // 中身ちょうどの ArrayBuffer にする(encode の結果は余白を持たないが、念のため切り出す)。
        return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
      });
      const controls = (window as unknown as MockWindow).__ARIN_E2E__;
      if (controls === undefined || !controls.sendChannel(channelId, buffers)) {
        throw new Error(`チャネル ${channelId} の受け取りの関数が無い`);
      }
      return buffers.reduce((sum, buffer) => sum + buffer.byteLength, 0);
    },
    { channelId, chunks: chunks.map((chunk) => (typeof chunk === "string" ? chunk : [...chunk])) },
  );
}

/** Rust からのイベントを画面へ届ける。購読が無ければ例外(届かないまま気づかずに進まないように)。 */
export async function emitEvent<E extends EventName>(page: Page, event: E, payload: EventMap[E]): Promise<void> {
  const delivered = await page.evaluate(
    ({ event, payload }) => (window as unknown as MockWindow).__ARIN_E2E__?.emit(event, payload) ?? 0,
    { event, payload },
  );
  if (delivered === 0) throw new Error(`イベント ${event} の購読が無い`);
}

/**
 * 同じイベントを payloads の順に、画面のページの 1 回の実行の中で続けて届ける(T71。あいだに描画のフレームを挟まないので、
 * Rust が続けて送った写しを画面が同じフレームで受けた形になる)。1 つでも購読が無ければ例外。
 */
export async function emitEventsInOneTurn<E extends EventName>(
  page: Page,
  event: E,
  payloads: readonly EventMap[E][],
): Promise<void> {
  const delivered = await page.evaluate(
    ({ event, payloads }) => {
      const controls = (window as unknown as MockWindow).__ARIN_E2E__;
      return payloads.map((payload) => controls?.emit(event, payload) ?? 0);
    },
    { event, payloads: [...payloads] },
  );
  if (delivered.some((n) => n === 0)) throw new Error(`イベント ${event} の購読が無い`);
}
