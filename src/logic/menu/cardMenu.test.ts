import { describe, expect, it } from "vitest";

import { buildCardMenu, type CardMenuEntry, PROJECT_COLOR_IDS } from "./cardMenu";

/** 区切り以外の項目の名前を、上から順に並べる。 */
const labels = (entries: readonly CardMenuEntry[]): string[] =>
  entries.flatMap((entry) => (entry.kind === "separator" ? [] : [entry.label]));

describe("PROJECT_COLOR_IDS(プロジェクト色の識別子)", () => {
  it("8 つでパレットの順に並ぶ", () => {
    expect(PROJECT_COLOR_IDS).toEqual([
      "red",
      "orange",
      "yellow",
      "green",
      "teal",
      "blue",
      "purple",
      "pink",
    ]);
  });
});

describe("buildCardMenu(カードの右クリックのメニュー)", () => {
  it("カードのメニューに新規ターミナルと絶対パスをコピーと色と一覧から削除がある", () => {
    const menu = buildCardMenu({ missing: false, color: "green" });

    expect(labels(menu)).toEqual(["新規ターミナル", "絶対パスをコピー", "色", "一覧から削除"]);
    expect(menu.map((entry) => entry.kind)).toEqual([
      "command",
      "separator",
      "command",
      "separator",
      "submenu",
      "separator",
      "command",
    ]);
    expect(menu[0]).toEqual({ kind: "command", label: "新規ターミナル", command: { type: "new-terminal" } });
    expect(menu[2]).toEqual({ kind: "command", label: "絶対パスをコピー", command: { type: "copy-path" } });
    expect(menu[6]).toEqual({ kind: "command", label: "一覧から削除", command: { type: "remove" } });
  });

  it("色の下に 8 色がパレットの順に並び、いまの色に印が付く", () => {
    const menu = buildCardMenu({ missing: false, color: "blue" });
    const submenu = menu[4];
    if (submenu?.kind !== "submenu") throw new Error("色の項目が無い");

    expect(submenu.items.map((item) => item.command)).toEqual(
      PROJECT_COLOR_IDS.map((color) => ({ type: "set-color", color })),
    );
    expect(submenu.items.map((item) => item.label)).toEqual([
      "赤",
      "オレンジ",
      "黄",
      "緑",
      "青緑",
      "青",
      "紫",
      "ピンク",
    ]);
    expect(submenu.items.filter((item) => item.checked).map((item) => item.command.color)).toEqual([
      "blue",
    ]);
  });

  it("見つからないカードのメニューは再リンクと一覧から削除", () => {
    const menu = buildCardMenu({ missing: true, color: "red" });

    expect(menu).toEqual([
      { kind: "command", label: "再リンク…", command: { type: "relink" } },
      { kind: "separator" },
      { kind: "command", label: "一覧から削除", command: { type: "remove" } },
    ]);
  });
});
