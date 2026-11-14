// 起動時の指定(描画方式と全角幅の方式、9 枚の検証画面)を取得してから、ターミナルのエンジンを作る(T21)。main.tsx から呼ぶ。
//
// M1 の検証用。人間が DOM と WebGL、unicode11 と graphemes を実機で比べる(T26)ためのもので、T27 で固定して外す。
// 9 枚の検証画面(ARIN_VERIFY_GRID。T22)も M1 の検証用で、M4 の T49 で外す。
// 指定は Rust が環境変数(ARIN_RENDERER・ARIN_UNICODE・ARIN_VERIFY_GRID)から読み、get_launch_options で 1 回だけ渡す。
// 取得に失敗しても起動は止めない。既定(dom・unicode11)で作り、診断に報告する(起動スモークが失敗として気づける)。

import type { ArinIpc } from "../ipc/client";
import type { LaunchOptions } from "../ipc/generated/LaunchOptions";
import { describeError } from "../logic/describeError";

/** 取得できなかったときに使う指定(Rust の LaunchOptions::default と同じ)。 */
export const DEFAULT_LAUNCH_OPTIONS: LaunchOptions = { renderer: "dom", unicode: "unicode11", verifyGrid: false };

/**
 * 起動時の指定を取得し、それで create を呼んでエンジンを作る。
 * 取得に失敗したら reportError に 1 行で報告し、既定の指定で作る。この Promise は取得の失敗では失敗しない。
 */
export async function createWithLaunchOptions<T>(
  ipc: Pick<ArinIpc, "getLaunchOptions">,
  reportError: (message: string) => void,
  create: (options: LaunchOptions) => T,
): Promise<T> {
  let options: LaunchOptions;
  try {
    options = await ipc.getLaunchOptions();
  } catch (cause) {
    reportError(
      `起動時の指定を取得できなかったので、既定(${DEFAULT_LAUNCH_OPTIONS.renderer}・${DEFAULT_LAUNCH_OPTIONS.unicode})で起動する: ${describeError(cause)}`,
    );
    options = DEFAULT_LAUNCH_OPTIONS;
  }
  return create(options);
}
