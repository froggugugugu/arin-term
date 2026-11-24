import { describe, expect, it } from "vitest";

import { updateNoticeText } from "./NoticeBar";

describe("updateNoticeText(新しい版の確認の知らせの文。ADR-018)", () => {
  it("種類ごとの文", () => {
    expect(updateNoticeText({ kind: "available", version: "0.4.0" })).toBe("新しい版 v0.4.0 があります");
    expect(updateNoticeText({ kind: "upToDate", version: "0.3.0" })).toBe("Arin v0.3.0 は最新です");
    expect(updateNoticeText({ kind: "failed", reason: "GitHub に問い合わせられませんでした" })).toBe(
      "新しい版を確かめられませんでした: GitHub に問い合わせられませんでした",
    );
  });
});
