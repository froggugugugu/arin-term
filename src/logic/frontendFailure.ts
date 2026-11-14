// 画面で起きた失敗(CSP 違反、未処理の例外、未処理の Promise の失敗)を、Rust へ送る 1 行の文にする。
// DOM の型を使わず、イベントは中身の分からない値として受け取って必要なものだけを読む(src/logic は DOM を使わない)。

import { describeError } from "./describeError";

/** 拾うイベントの種類(window・document のイベント名)。 */
export type FrontendFailureKind = "securitypolicyviolation" | "error" | "unhandledrejection";

function field(value: unknown, key: string): unknown {
  return typeof value === "object" && value !== null ? (value as Record<string, unknown>)[key] : undefined;
}

function text(value: unknown, key: string): string {
  const v = field(value, key);
  return typeof v === "string" ? v : typeof v === "number" ? String(v) : "";
}

/** `(file:line:col)` の形にする。分からない部分は省く。全部分からなければ空。 */
function location(file: string, line: string, col: string): string {
  if (file === "" && line === "") return "";
  return `(${[file, line, col].filter((part) => part !== "" && part !== "0").join(":")})`;
}

export function describeFrontendFailure(kind: FrontendFailureKind, event: unknown): string {
  switch (kind) {
    case "securitypolicyviolation": {
      const directive = text(event, "effectiveDirective") || text(event, "violatedDirective") || "(不明な指示)";
      const blocked = text(event, "blockedURI") || "(インライン)";
      const where = location(text(event, "sourceFile"), text(event, "lineNumber"), text(event, "columnNumber"));
      return `CSP 違反: ${directive} が ${blocked} を止めた${where}`;
    }
    case "error": {
      const message = text(event, "message");
      if (message !== "") {
        return `未処理の例外: ${message}${location(text(event, "filename"), text(event, "lineno"), text(event, "colno"))}`;
      }
      // スクリプトやスタイルシートの読み込みの失敗は、要素で起きて message を持たない(捕捉の段階で拾う)。
      const target = field(event, "target");
      const tag = text(target, "tagName").toLowerCase();
      if (tag === "") return "読み込みの失敗: (不明な要素)";
      const url = text(target, "src") || text(target, "href");
      return `読み込みの失敗: <${tag}>${url === "" ? "" : ` ${url}`}`;
    }
    case "unhandledrejection":
      return `未処理の Promise の失敗: ${describeError(field(event, "reason"))}`;
  }
}

/** 報告の数の上限。同じ失敗が繰り返されたとき(描画のたびの CSP 違反など)に通信を埋めないため。 */
export interface ReportLimiter {
  /** 今回の報告を送るか。`last` は上限に達したことを添えて送る回、`drop` は捨てる。 */
  take(): "send" | "last" | "drop";
}

export function createReportLimiter(max: number): ReportLimiter {
  let count = 0;
  return {
    take() {
      count += 1;
      if (count <= max) return "send";
      return count === max + 1 ? "last" : "drop";
    },
  };
}
