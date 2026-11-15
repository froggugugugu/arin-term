// Rust との通信の型付きの包み(設計書 §4・§5.2)。
// 通信の実体(IpcTransport)は外から受け取る。本物は ./tauri.ts、テストでは src/test/fakeIpc.ts を渡す。

import type { AppSnapshot } from "./generated/AppSnapshot";
import type { ConnectOutcome } from "./generated/ConnectOutcome";
import type { LaunchOptions } from "./generated/LaunchOptions";
import type { TerminalId } from "./generated/TerminalId";

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
}

export type CommandName = keyof CommandMap;

/** Rust から届くイベントと、その中身の対応。名前は src-tauri/src/commands/app.rs の定数と合わせる。 */
export interface EventMap {
  "app-snapshot": AppSnapshot;
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
   * 出力のチャネルをつなぐ。最初の接続で Rust が rows × cols の PTY を起動する。
   * 届いた出力は 1 回ぶんずつ onOutput に渡す。
   */
  connectTerminal(
    id: TerminalId,
    rows: number,
    cols: number,
    onOutput: (data: Uint8Array) => void,
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
    connectTerminal: (terminalId, rows, cols, onOutput) => {
      let closed = false;
      const channel = transport.openChannel((message) => {
        if (closed) return;
        const bytes = toBytes(message);
        if (bytes !== null) onOutput(bytes);
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
  };
}
