// Rust との通信の型付きの包み(設計書 §4・§5.2)。
// 通信の実体(IpcTransport)は外から受け取る。本物は ./tauri.ts、テストでは src/test/fakeIpc.ts を渡す。

import type { AddProjectOutcome } from "./generated/AddProjectOutcome";
import type { AppSnapshot } from "./generated/AppSnapshot";
import type { ConflictResolution } from "./generated/ConflictResolution";
import type { ConnectOutcome } from "./generated/ConnectOutcome";
import type { DirListing } from "./generated/DirListing";
import type { FileOpOutcome } from "./generated/FileOpOutcome";
import type { LaunchOptions } from "./generated/LaunchOptions";
import type { MenuCommand } from "./generated/MenuCommand";
import type { NewEntryKind } from "./generated/NewEntryKind";
import type { PathKind } from "./generated/PathKind";
import type { ProjectColor } from "./generated/ProjectColor";
import type { ProjectId } from "./generated/ProjectId";
import type { QuitRequest } from "./generated/QuitRequest";
import type { RelPath } from "./generated/RelPath";
import type { RemoveProjectOutcome } from "./generated/RemoveProjectOutcome";
import type { TerminalId } from "./generated/TerminalId";
import type { TreeChange } from "./generated/TreeChange";

/**
 * 出力チャネルを Rust へ渡すときの値。中身は通信の実体が決める(Tauri では Channel のインスタンス)。
 * 画面のコードは中身を見ない。
 */
export type ChannelToken = { readonly __channelToken: unique symbol };

/**
 * 画面から呼べる Rust のコマンドと、その引数・戻り値の対応。
 * 名前は src-tauri/src/commands/ の関数名と合わせる。引数のキーは camelCase。戻り値の無いコマンドは null が返る。
 */
export interface CommandMap {
  get_app_snapshot: { args: undefined; result: AppSnapshot };
  get_launch_options: { args: undefined; result: LaunchOptions };
  notify_app_ready: { args: undefined; result: null };
  report_frontend_error: { args: { message: string }; result: null };
  confirm_quit: { args: undefined; result: null };
  connect_terminal: {
    args: { terminalId: TerminalId; rows: number; cols: number; onOutput: ChannelToken };
    result: ConnectOutcome;
  };
  write_terminal: { args: { terminalId: TerminalId; data: string }; result: null };
  /** data は 0〜255 の数の配列(Rust の Vec<u8>)。 */
  write_terminal_bytes: { args: { terminalId: TerminalId; data: number[] }; result: null };
  new_terminal: { args: undefined; result: TerminalId };
  close_terminal: { args: { terminalId: TerminalId }; result: null };
  select_terminal: { args: { terminalId: TerminalId }; result: null };
  ack_terminal_output: { args: { terminalId: TerminalId; bytes: number }; result: null };
  resize_terminal: { args: { terminalId: TerminalId; rows: number; cols: number }; result: null };
  add_project_by_dialog: { args: undefined; result: AddProjectOutcome };
  select_project: { args: { projectId: ProjectId }; result: null };
  remove_project: { args: { projectId: ProjectId; confirmed: boolean }; result: RemoveProjectOutcome };
  relink_project: { args: { projectId: ProjectId }; result: boolean };
  set_project_color: { args: { projectId: ProjectId; color: ProjectColor }; result: null };
  copy_project_path: { args: { projectId: ProjectId }; result: null };
  set_pane_sizes: { args: { sidebarWidth: number; cardAreaHeight: number }; result: null };
  set_sidebar_collapsed: { args: { collapsed: boolean }; result: null };
  /** path はルート基準の相対パス(ルートは空の文字列)。絶対パス・`..` は Rust が拒否する。 */
  list_directory: { args: { projectId: ProjectId; path: RelPath }; result: DirListing };
  set_show_hidden: { args: { projectId: ProjectId; showHidden: boolean }; result: null };
  /**
   * ファイル操作。項目はルート基準の相対パス(ルートは空の文字列)で指す。resolution は衝突したあとの呼び直しでだけ付ける
   * (省略=衝突なら何も変えずに conflict を返す)。
   */
  create_entry: {
    args: { projectId: ProjectId; parent: RelPath; name: string; kind: NewEntryKind; resolution?: ConflictResolution };
    result: FileOpOutcome;
  };
  rename_entry: {
    args: { projectId: ProjectId; path: RelPath; newName: string; resolution?: ConflictResolution };
    result: FileOpOutcome;
  };
  move_entry: {
    args: { projectId: ProjectId; source: RelPath; destDir: RelPath; resolution?: ConflictResolution };
    result: FileOpOutcome;
  };
  copy_entry: {
    args: { projectId: ProjectId; source: RelPath; destDir: RelPath; resolution?: ConflictResolution };
    result: FileOpOutcome;
  };
  trash_entry: { args: { projectId: ProjectId; path: RelPath }; result: FileOpOutcome };
  copy_entries: { args: { projectId: ProjectId; paths: RelPath[] }; result: FileOpOutcome };
  cut_entries: { args: { projectId: ProjectId; paths: RelPath[] }; result: FileOpOutcome };
  paste_entries: {
    args: { projectId: ProjectId; destDir: RelPath; resolution?: ConflictResolution };
    result: FileOpOutcome;
  };
  /** デスクトップとパス(T42d)。項目はルート基準の相対パス(ルートは空の文字列)。絶対パスも挿入する文字列も送らない。 */
  reveal_entry: { args: { projectId: ProjectId; path: RelPath }; result: FileOpOutcome };
  open_entry: { args: { projectId: ProjectId; path: RelPath }; result: FileOpOutcome };
  copy_entry_path: { args: { projectId: ProjectId; path: RelPath; kind: PathKind }; result: FileOpOutcome };
  insert_entry_path: {
    args: { projectId: ProjectId; paths: RelPath[]; terminalId: TerminalId };
    result: FileOpOutcome;
  };
}

export type CommandName = keyof CommandMap;

/**
 * Rust から届くイベントと、その中身の対応。名前は src-tauri/src/commands/app.rs(app-snapshot)と
 * src-tauri/src/events.rs(menu-command・quit-requested・tree-changed)の定数と合わせる。
 */
export interface EventMap {
  "app-snapshot": AppSnapshot;
  "menu-command": MenuCommand;
  "quit-requested": QuitRequest;
  "tree-changed": TreeChange;
}

export type EventName = keyof EventMap;

/** 通信の実体。コマンド名と引数を Rust へ送り、戻り値を返す。 */
export type InvokeFn = <C extends CommandName>(
  command: C,
  args: CommandMap[C]["args"],
) => Promise<CommandMap[C]["result"]>;

/** 購読をやめる関数。 */
export type Unlisten = () => void;

/** Rust から生のバイト列を受け取るチャネル。 */
export interface OutputChannel {
  /** コマンドの引数に入れる値。 */
  readonly token: ChannelToken;
  /** 受け取りをやめる。2 回目以降は何もしない。 */
  close(): void;
}

/** 通信の実体(コマンド、チャネル、イベント)。 */
export interface IpcTransport {
  invoke: InvokeFn;
  /** チャネルを作る。Rust が送った 1 回ぶんの中身(Tauri では生のバイト列は ArrayBuffer)を onMessage に渡す。 */
  openChannel(onMessage: (message: unknown) => void): OutputChannel;
  /** イベントを購読する。購読が済んだら、やめる関数で解決する。 */
  listen<E extends EventName>(event: E, handler: (payload: EventMap[E]) => void): Promise<Unlisten>;
}

/** 接続中の出力の受け取り。 */
export interface OutputSubscription {
  /**
   * 接続のコマンドの結果。sizeApplied が false なら、再接続で寸法を PTY に反映できなかった(画面が寸法を送り直す)。
   * 失敗したら受け取りはやめてある。
   */
  readonly connected: Promise<ConnectOutcome>;
  /** 受け取りをやめる(以後に届いた出力は捨てる)。2 回目以降は何もしない。 */
  close(): void;
}

/** 画面が使う Rust の操作。 */
export interface ArinIpc {
  /** 状態の写しを取得する。 */
  getAppSnapshot(): Promise<AppSnapshot>;
  /** 起動時の指定(描画方式と全角幅の方式。M1 の検証用)を取得する。起動時に 1 回だけ呼ぶ。 */
  getLaunchOptions(): Promise<LaunchOptions>;
  /** 画面の準備ができたことを Rust へ知らせる。 */
  notifyAppReady(): Promise<void>;
  /** 状態の写しが変わるたびに listener を呼ぶ。購読が済んだら、やめる関数で解決する。 */
  onAppSnapshot(listener: (snapshot: AppSnapshot) => void): Promise<Unlisten>;
  /** 画面で起きた失敗を Rust へ知らせる(Rust が標準エラーに 1 行で書く)。 */
  reportFrontendError(message: string): Promise<void>;
  /**
   * メニューバーの項目から画面へ送られる操作(いまは「すべてを選択」だけ)を受けるたびに listener を呼ぶ。
   * タブの操作(⌘T・⌘W・⌘⇧[・⌘⇧])は Rust が行い、結果は写しで届く。購読が済んだら、やめる関数で解決する。
   */
  onMenuCommand(listener: (command: MenuCommand) => void): Promise<Unlisten>;
  /**
   * 終了の要求(⌘Q・ウィンドウの閉じるボタン)を Rust が動作中のターミナルのために止めたとき、動作中の一覧を渡して
   * listener を呼ぶ(FR-063)。確認したら confirmQuit を呼ぶ。取り消すなら何も呼ばない。購読が済んだら、やめる関数で解決する。
   */
  onQuitRequested(listener: (request: QuitRequest) => void): Promise<Unlisten>;
  /** 終了の確認で「終了」が選ばれた。Rust が状態を保存し、全ターミナルを終わらせてからアプリを終える。 */
  confirmQuit(): Promise<void>;
  /**
   * 出力のチャネルをつなぐ。最初の接続で Rust が rows × cols の PTY を起動する。
   * 届いた出力は 1 回ぶんずつ onOutput に渡す。バイト列にできない中身(toBytes が null)は onOutput に渡さず、
   * onUnreadable があればそこへ渡す(診断に報告するため)。
   */
  connectTerminal(
    id: TerminalId,
    rows: number,
    cols: number,
    onOutput: (data: Uint8Array) => void,
    onUnreadable?: (message: unknown) => void,
  ): OutputSubscription;
  /** キー入力・貼り付けの文字列を PTY に書き込む。 */
  writeTerminal(id: TerminalId, data: string): Promise<void>;
  /**
   * バイト列をそのまま PTY に書き込む(xterm.js の onBinary)。
   * 同じターミナルへの writeTerminal と合わせて、前の応答を待ってから次を送る(順序は送る側が保つ)。
   */
  writeTerminalBytes(id: TerminalId, data: Uint8Array): Promise<void>;
  /** 選択中のプロジェクトに新しいタブを開いて表示中にし、その ID を返す。シェルは接続で起動する。 */
  newTerminal(): Promise<TerminalId>;
  /** タブを閉じ、そのシェルを終わらせる。 */
  closeTerminal(id: TerminalId): Promise<void>;
  /** そのタブを表示中にする。 */
  selectTerminal(id: TerminalId): Promise<void>;
  /** 出力を bytes バイト取り込んだことを知らせる(流量制御)。 */
  ackTerminalOutput(id: TerminalId, bytes: number): Promise<void>;
  /** 寸法(行・桁)を変える。 */
  resizeTerminal(id: TerminalId, rows: number, cols: number): Promise<void>;
  /**
   * Rust がフォルダ選択のダイアログを開き、選ばれたフォルダを登録して選び、タブを 1 枚開く。
   * 選んだパスは画面に届かない。登録済みのフォルダなら増やさずそれを選ぶ(alreadyRegistered)。取り消しは cancelled。
   */
  addProjectByDialog(): Promise<AddProjectOutcome>;
  /** プロジェクトを選ぶ。ターミナルには触れない。 */
  selectProject(id: ProjectId): Promise<void>;
  /**
   * プロジェクトを一覧から外す(フォルダは消さない)。動作中のターミナルがあり confirmed が false なら、
   * 何も変えずに needsConfirmation と一覧を返す(確認したら confirmed を true にして呼び直す)。
   */
  removeProject(id: ProjectId, confirmed: boolean): Promise<RemoveProjectOutcome>;
  /** Rust がフォルダ選択のダイアログを開き、選ばれたフォルダにつなぎ直す。つなぎ直したら true、取り消しは false。 */
  relinkProject(id: ProjectId): Promise<boolean>;
  /** プロジェクトの色を変える。 */
  setProjectColor(id: ProjectId, color: ProjectColor): Promise<void>;
  /** ルートの絶対パスを Rust がクリップボードに書く。選択は変えない。パスは画面に届かない。 */
  copyProjectPath(id: ProjectId): Promise<void>;
  /**
   * 左ペインの幅とカード領域の高さ(CSS ピクセル)を Rust へ送る。Rust が最小寸法などの規則で直し、写しで返す。
   * 変わったら保存が予約される(0.5 秒まとめ)。
   */
  setPaneSizes(sidebarWidth: number, cardAreaHeight: number): Promise<void>;
  /** 左ペインを折りたたむ・戻す。幅は Rust が覚えている。 */
  setSidebarCollapsed(collapsed: boolean): Promise<void>;
  /**
   * フォルダの中身を問い合わせる(ツリーの展開)。path はルート基準の相対パスで、ルートは空の文字列。
   * Rust が除外(隠しファイル表示が OFF のとき)と並び順を当てて返す。プロジェクトやフォルダが見つからなければ notFound、
   * ファイルを指したら notADirectory、シンボリックリンクでルートの外へ出るなら outsideRoot(中身は読まない)。
   * 絶対パスは送らず、届かない。
   */
  listDirectory(projectId: ProjectId, path: RelPath): Promise<DirListing>;
  /** ツリーの隠しファイル表示を切り替える(プロジェクトごと)。結果は写しの showHidden で届き、保存が予約される。 */
  setShowHidden(projectId: ProjectId, showHidden: boolean): Promise<void>;
  /**
   * 選択中のプロジェクトのツリーで外からの変更が届くたびに listener を呼ぶ(Rust は選択中のプロジェクトのルートだけを監視する)。
   * 中身はプロジェクト ID と、中身が変わったフォルダの相対パス(ルートは空の文字列)、全部読み直しの印。絶対パスは届かない。
   * 除外(隠しファイル表示が OFF のとき)の中の変更は届かない。購読が済んだら、やめる関数で解決する。
   */
  onTreeChanged(listener: (change: TreeChange) => void): Promise<Unlisten>;
  /*
   * ファイル操作(FR-025〜030)。どれもプロジェクトの ID とルート基準の相対パスで指し、応答は FileOpOutcome で返る:
   * done(select は操作のあとに選ぶ項目。無ければ null)・notFound(実行の時点で無かった。何も変えていない)・
   * conflict(置く先に同じ名前がある。何も変えていない。確認を出し、resolution を付けて呼び直す。中止なら呼び直さない)・
   * invalidName・notAllowed(ルート自体・ルートの外・自分の子孫へ)・failed(1 行の文言)。
   * ツリーの更新は tree-changed と画面の読み直しに任せる。絶対パスは送らず、届かない。
   */
  /** フォルダ parent(ルートは空の文字列)に空のファイルかフォルダ name を作る。 */
  createEntry(
    projectId: ProjectId,
    parent: RelPath,
    name: string,
    kind: NewEntryKind,
    resolution?: ConflictResolution,
  ): Promise<FileOpOutcome>;
  /** 項目 path の名前を newName に変える。 */
  renameEntry(
    projectId: ProjectId,
    path: RelPath,
    newName: string,
    resolution?: ConflictResolution,
  ): Promise<FileOpOutcome>;
  /** 項目 source をフォルダ destDir の中へ移す(ツリー内のドロップ)。 */
  moveEntry(
    projectId: ProjectId,
    source: RelPath,
    destDir: RelPath,
    resolution?: ConflictResolution,
  ): Promise<FileOpOutcome>;
  /** 項目 source をフォルダ destDir の中へ写す(⌥ を押しながらのドロップ)。 */
  copyEntry(
    projectId: ProjectId,
    source: RelPath,
    destDir: RelPath,
    resolution?: ConflictResolution,
  ): Promise<FileOpOutcome>;
  /** 項目 path をゴミ箱に入れる(ゴミ箱を経由しない削除は無い)。 */
  trashEntry(projectId: ProjectId, path: RelPath): Promise<FileOpOutcome>;
  /** 項目 paths をファイルの URL としてペーストボードに書く(Finder にペーストできる)。 */
  copyEntries(projectId: ProjectId, paths: RelPath[]): Promise<FileOpOutcome>;
  /** 項目 paths を切り取る(ファイルはペーストまで動かない)。 */
  cutEntries(projectId: ProjectId, paths: RelPath[]): Promise<FileOpOutcome>;
  /**
   * ペーストボードのファイルをフォルダ destDir の中へ置く。切り取ったものなら移し、ほか(Finder でコピーしたものなど)はコピーする。
   * ペーストボードにファイルが無ければ何もせず done(select は null)。
   */
  pasteEntries(projectId: ProjectId, destDir: RelPath, resolution?: ConflictResolution): Promise<FileOpOutcome>;
  /*
   * デスクトップとパス(FR-015・FR-016・FR-018・FR-024)。応答は FileOpOutcome(done の select は常に null)。
   * notFound=項目が無い、notAllowed=ルートの外(リンク経由を含む)・ルート自体、failed=1 行の文言。絶対パスは届かない。
   */
  /** 項目 path を Finder で選んだ状態で表示する。ルート(空の文字列)も表示できる。 */
  revealEntry(projectId: ProjectId, path: RelPath): Promise<FileOpOutcome>;
  /**
   * ファイル path を既定のアプリで開く。フォルダ(画面が開閉する)・リンク・実行できるファイル・ルートは開かず
   * failed か notAllowed を返す。
   */
  openEntry(projectId: ProjectId, path: RelPath): Promise<FileOpOutcome>;
  /**
   * 項目 path のパスを Rust がクリップボードに書く(kind が relative なら相対、absolute なら絶対)。パスは画面に届かない。
   * ルートの絶対パスは path を空の文字列にして呼ぶ(ルートの相対パスは notAllowed)。
   */
  copyEntryPath(projectId: ProjectId, path: RelPath, kind: PathKind): Promise<FileOpOutcome>;
  /**
   * 項目 paths のパスを、Rust がエスケープして末尾に空白を 1 つ付け、ターミナル terminalId の入力行に書く。Enter は送らない。
   * 宛先が同じプロジェクトのターミナルなら相対パス、違えば絶対パス。宛先が無い・動いていなければ何も書かず failed。
   * M3 の宛先は、選択中のプロジェクトの表示中のタブ(パスをターミナルに挿入)か、ドロップ先のターミナル。
   */
  insertEntryPath(projectId: ProjectId, paths: RelPath[], terminalId: TerminalId): Promise<FileOpOutcome>;
}

/**
 * チャネルで届いた 1 回ぶんの中身をバイト列にする。
 * Tauri は生のバイト列を ArrayBuffer で渡す。念のため型付き配列と数の配列も受ける。それ以外は null。
 */
export function toBytes(message: unknown): Uint8Array | null {
  if (message instanceof ArrayBuffer) return new Uint8Array(message);
  if (ArrayBuffer.isView(message)) {
    return new Uint8Array(message.buffer, message.byteOffset, message.byteLength);
  }
  if (Array.isArray(message) && message.every((b) => Number.isInteger(b) && b >= 0 && b <= 255)) {
    return Uint8Array.from(message as number[]);
  }
  return null;
}

/** 衝突の対処は、あるときだけ引数に載せる(無いときはキーごと省く)。 */
function withResolution<A extends object>(
  args: A,
  resolution: ConflictResolution | undefined,
): A & { resolution?: ConflictResolution } {
  return resolution === undefined ? args : { ...args, resolution };
}

export function createIpc(transport: IpcTransport): ArinIpc {
  const { invoke } = transport;
  const done = async (result: Promise<null>): Promise<void> => {
    await result;
  };
  return {
    getAppSnapshot: () => invoke("get_app_snapshot", undefined),
    getLaunchOptions: () => invoke("get_launch_options", undefined),
    notifyAppReady: () => done(invoke("notify_app_ready", undefined)),
    onAppSnapshot: (listener) => transport.listen("app-snapshot", listener),
    reportFrontendError: (message) => done(invoke("report_frontend_error", { message })),
    onMenuCommand: (listener) => transport.listen("menu-command", listener),
    onQuitRequested: (listener) => transport.listen("quit-requested", listener),
    confirmQuit: () => done(invoke("confirm_quit", undefined)),
    connectTerminal: (terminalId, rows, cols, onOutput, onUnreadable) => {
      let closed = false;
      const channel = transport.openChannel((message) => {
        if (closed) return;
        const bytes = toBytes(message);
        if (bytes !== null) onOutput(bytes);
        else onUnreadable?.(message);
      });
      const close = () => {
        if (closed) return;
        closed = true;
        channel.close();
      };
      const connected = invoke("connect_terminal", {
        terminalId,
        rows,
        cols,
        onOutput: channel.token,
      }).then(
        (outcome) => outcome,
        (cause: unknown) => {
          close();
          throw cause;
        },
      );
      return { connected, close };
    },
    writeTerminal: (terminalId, data) => done(invoke("write_terminal", { terminalId, data })),
    writeTerminalBytes: (terminalId, data) =>
      done(invoke("write_terminal_bytes", { terminalId, data: Array.from(data) })),
    newTerminal: () => invoke("new_terminal", undefined),
    closeTerminal: (terminalId) => done(invoke("close_terminal", { terminalId })),
    selectTerminal: (terminalId) => done(invoke("select_terminal", { terminalId })),
    ackTerminalOutput: (terminalId, bytes) =>
      done(invoke("ack_terminal_output", { terminalId, bytes })),
    resizeTerminal: (terminalId, rows, cols) =>
      done(invoke("resize_terminal", { terminalId, rows, cols })),
    addProjectByDialog: () => invoke("add_project_by_dialog", undefined),
    selectProject: (projectId) => done(invoke("select_project", { projectId })),
    removeProject: (projectId, confirmed) => invoke("remove_project", { projectId, confirmed }),
    relinkProject: (projectId) => invoke("relink_project", { projectId }),
    setProjectColor: (projectId, color) => done(invoke("set_project_color", { projectId, color })),
    copyProjectPath: (projectId) => done(invoke("copy_project_path", { projectId })),
    setPaneSizes: (sidebarWidth, cardAreaHeight) =>
      done(invoke("set_pane_sizes", { sidebarWidth, cardAreaHeight })),
    setSidebarCollapsed: (collapsed) => done(invoke("set_sidebar_collapsed", { collapsed })),
    listDirectory: (projectId, path) => invoke("list_directory", { projectId, path }),
    setShowHidden: (projectId, showHidden) => done(invoke("set_show_hidden", { projectId, showHidden })),
    onTreeChanged: (listener) => transport.listen("tree-changed", listener),
    createEntry: (projectId, parent, name, kind, resolution) =>
      invoke("create_entry", withResolution({ projectId, parent, name, kind }, resolution)),
    renameEntry: (projectId, path, newName, resolution) =>
      invoke("rename_entry", withResolution({ projectId, path, newName }, resolution)),
    moveEntry: (projectId, source, destDir, resolution) =>
      invoke("move_entry", withResolution({ projectId, source, destDir }, resolution)),
    copyEntry: (projectId, source, destDir, resolution) =>
      invoke("copy_entry", withResolution({ projectId, source, destDir }, resolution)),
    trashEntry: (projectId, path) => invoke("trash_entry", { projectId, path }),
    copyEntries: (projectId, paths) => invoke("copy_entries", { projectId, paths }),
    cutEntries: (projectId, paths) => invoke("cut_entries", { projectId, paths }),
    pasteEntries: (projectId, destDir, resolution) =>
      invoke("paste_entries", withResolution({ projectId, destDir }, resolution)),
    revealEntry: (projectId, path) => invoke("reveal_entry", { projectId, path }),
    openEntry: (projectId, path) => invoke("open_entry", { projectId, path }),
    copyEntryPath: (projectId, path, kind) => invoke("copy_entry_path", { projectId, path, kind }),
    insertEntryPath: (projectId, paths, terminalId) =>
      invoke("insert_entry_path", { projectId, paths, terminalId }),
  };
}
