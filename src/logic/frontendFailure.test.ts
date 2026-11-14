import { describe, expect, it } from "vitest";

import { createReportLimiter, describeFrontendFailure } from "./frontendFailure";

describe("describeFrontendFailure(画面の失敗を 1 行にする)", () => {
  it("CSP 違反は、止めた指示と対象と出どころを書く", () => {
    expect(
      describeFrontendFailure("securitypolicyviolation", {
        effectiveDirective: "style-src-elem",
        violatedDirective: "style-src",
        blockedURI: "inline",
        sourceFile: "tauri://localhost/assets/index.js",
        lineNumber: 12,
        columnNumber: 3,
      }),
    ).toBe("CSP 違反: style-src-elem が inline を止めた(tauri://localhost/assets/index.js:12:3)");
  });

  it("CSP 違反で effectiveDirective が無ければ violatedDirective を、対象が空ならインラインと書く", () => {
    expect(
      describeFrontendFailure("securitypolicyviolation", { violatedDirective: "style-src", blockedURI: "" }),
    ).toBe("CSP 違反: style-src が (インライン) を止めた");
  });

  it("未処理の例外は、メッセージと場所を書く", () => {
    expect(
      describeFrontendFailure("error", {
        message: "Uncaught TypeError: x is undefined",
        filename: "tauri://localhost/assets/index.js",
        lineno: 1,
        colno: 99,
      }),
    ).toBe("未処理の例外: Uncaught TypeError: x is undefined(tauri://localhost/assets/index.js:1:99)");
  });

  it("メッセージの無い error は、読み込みに失敗した要素を書く", () => {
    expect(
      describeFrontendFailure("error", { target: { tagName: "LINK", href: "tauri://localhost/a.css" } }),
    ).toBe("読み込みの失敗: <link> tauri://localhost/a.css");
    expect(describeFrontendFailure("error", { target: { tagName: "IMG", src: "x.png" } })).toBe(
      "読み込みの失敗: <img> x.png",
    );
    expect(describeFrontendFailure("error", {})).toBe("読み込みの失敗: (不明な要素)");
  });

  it("未処理の Promise の失敗は、理由を文字にする", () => {
    expect(describeFrontendFailure("unhandledrejection", { reason: new Error("壊れた") })).toBe(
      "未処理の Promise の失敗: 壊れた",
    );
    expect(describeFrontendFailure("unhandledrejection", { reason: "文字列の理由" })).toBe(
      "未処理の Promise の失敗: 文字列の理由",
    );
  });

  it("イベントがオブジェクトでなくても落ちない", () => {
    expect(describeFrontendFailure("unhandledrejection", null)).toBe("未処理の Promise の失敗: undefined");
    expect(describeFrontendFailure("securitypolicyviolation", 1)).toBe("CSP 違反: (不明な指示) が (インライン) を止めた");
  });
});

describe("createReportLimiter(報告の数の上限)", () => {
  it("上限までは送り、上限に達した回は最後の 1 回、それ以降は捨てる", () => {
    const limiter = createReportLimiter(2);

    expect([limiter.take(), limiter.take(), limiter.take(), limiter.take()]).toEqual([
      "send",
      "send",
      "last",
      "drop",
    ]);
  });

  it("上限が 0 なら最初から最後の 1 回になる", () => {
    const limiter = createReportLimiter(0);

    expect([limiter.take(), limiter.take()]).toEqual(["last", "drop"]);
  });
});
