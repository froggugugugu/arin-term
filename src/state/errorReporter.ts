// 画面で起きた失敗を Rust へ知らせる診断の機能(常に有効)。Rust は決まった頭の文字列を付けて標準エラーに 1 行で書く。
// 実アプリでは画面のコンソールを見られないので、CSP 違反などを起動スモーク(scripts/smoke.mjs)が検出する唯一の道になる。
//
// window と document は引数で受け取る(テストでは偽物を渡す)。文の組み立ては src/logic/frontendFailure.ts。

import { describeError } from "../logic/describeError";
import { createReportLimiter, describeFrontendFailure, type FrontendFailureKind } from "../logic/frontendFailure";

/** イベントを拾う的(window・document)。 */
export interface FailureEventTarget {
  addEventListener(type: string, listener: (event: unknown) => void, capture?: boolean): void;
  removeEventListener(type: string, listener: (event: unknown) => void, capture?: boolean): void;
}

export interface ErrorReporterDeps {
  window: FailureEventTarget;
  document: FailureEventTarget;
  /** Rust へ送る(ArinIpc.reportFrontendError)。 */
  report: (message: string) => Promise<void>;
  /** ログの出し先。既定は console.error。 */
  log?: (...args: unknown[]) => void;
  /** Rust へ送る数の上限。既定は 50。 */
  max?: number;
}

export interface ErrorReporter {
  /** 失敗を 1 行で知らせる(ログにも出す)。 */
  report(message: string): void;
  /** 受け取り手を外す。 */
  uninstall(): void;
}

export const DEFAULT_REPORT_LIMIT = 50;

export function installErrorReporter({
  window,
  document,
  report,
  log = console.error,
  max = DEFAULT_REPORT_LIMIT,
}: ErrorReporterDeps): ErrorReporter {
  const limiter = createReportLimiter(max);

  const send = (message: string) => {
    log(message);
    const decision = limiter.take();
    if (decision === "drop") return;
    const line =
      decision === "last" ? `${message}(画面の失敗の報告が上限の ${max} 件に達したので、以後は送らない)` : message;
    // 送るのに失敗しても投げ直さない(未処理の失敗になると、それをまた報告して繰り返すため)。
    report(line).catch((cause: unknown) => {
      log(`画面の失敗を Rust へ送れなかった: ${describeError(cause)}`);
    });
  };

  const handler = (kind: FrontendFailureKind) => (event: unknown) => send(describeFrontendFailure(kind, event));
  const onViolation = handler("securitypolicyviolation");
  const onError = handler("error");
  const onRejection = handler("unhandledrejection");

  document.addEventListener("securitypolicyviolation", onViolation);
  // 読み込みの失敗は要素で起きて泡立たないので、捕捉の段階で拾う。
  window.addEventListener("error", onError, true);
  window.addEventListener("unhandledrejection", onRejection);

  return {
    report: send,
    uninstall() {
      document.removeEventListener("securitypolicyviolation", onViolation);
      window.removeEventListener("error", onError, true);
      window.removeEventListener("unhandledrejection", onRejection);
    },
  };
}
