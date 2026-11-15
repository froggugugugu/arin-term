import { describe, expect, it } from "vitest";

import { applyDirListing, createTreeModel, dirsToReload, flattenRows } from "./treeModel";
import { createUpdateGate, type HoldReason, holdUpdates, isHeld, offerUpdates, releaseUpdates } from "./updateGate";

describe("UpdateGate(更新の保留。FR-037、【仮定】18)", () => {
  it("保留が無ければ届いた変更をすぐ返す(重なりは 1 つにする)", () => {
    const result = offerUpdates(createUpdateGate(), ["src", "docs", "src"]);

    expect(result.flush).toEqual(["src", "docs"]);
    expect(result.gate.pending).toEqual([]);
    expect(isHeld(result.gate)).toBe(false);
  });

  it("保留中は変更を積むだけで行が変わらない", () => {
    const model = applyDirListing(createTreeModel(), "", [{ name: "src", kind: "directory" }]);
    const before = flattenRows(model);
    const gate = holdUpdates(createUpdateGate(), "contextMenu");

    const result = offerUpdates(gate, dirsToReload(model, { dirs: [""], fullReload: false }));

    expect(result.flush).toEqual([]);
    expect(result.gate.pending).toEqual([""]);
    // 読み直しを求めないので、モデルも行もそのまま
    expect(flattenRows(model)).toEqual(before);
  });

  it("ドラッグ 名前の入力 メニュー 確認のどれかが残っていれば保留が続く", () => {
    const reasons: HoldReason[] = ["drag", "nameInput", "contextMenu", "dialog"];
    let gate = createUpdateGate();
    for (const reason of reasons) {
      gate = holdUpdates(gate, reason);
    }
    gate = offerUpdates(gate, ["src"]).gate;

    for (const reason of reasons.slice(0, -1)) {
      const released = releaseUpdates(gate, reason);
      expect(released.flush).toEqual([]);
      expect(isHeld(released.gate)).toBe(true);
      gate = released.gate;
    }

    const last = releaseUpdates(gate, "dialog");
    expect(last.flush).toEqual(["src"]);
    expect(isHeld(last.gate)).toBe(false);
    expect(last.gate.pending).toEqual([]);
  });

  it("保留が解けると 100 件の追加で変わったフォルダをまとめて 1 回で返す", () => {
    let gate = holdUpdates(createUpdateGate(), "nameInput");
    for (let i = 0; i < 100; i += 1) {
      // 100 件の追加は、src と src/lib の 2 つのフォルダの変更として届く
      const result = offerUpdates(gate, [i % 2 === 0 ? "src" : "src/lib"]);
      expect(result.flush).toEqual([]);
      gate = result.gate;
    }

    const released = releaseUpdates(gate, "nameInput");

    expect(released.flush).toEqual(["src", "src/lib"]);
    expect(releaseUpdates(released.gate, "nameInput").flush).toEqual([]);
  });

  it("同じ理由を 2 度掛けても 1 度解けば解ける_掛けていない理由を解いても何も起きない", () => {
    let gate = holdUpdates(holdUpdates(createUpdateGate(), "drag"), "drag");
    gate = offerUpdates(gate, ["a"]).gate;

    const unrelated = releaseUpdates(gate, "dialog");
    expect(unrelated.flush).toEqual([]);
    expect(isHeld(unrelated.gate)).toBe(true);

    expect(releaseUpdates(unrelated.gate, "drag").flush).toEqual(["a"]);
  });

  it("保留が無いときに解いても空を返す", () => {
    const result = releaseUpdates(createUpdateGate(), "drag");

    expect(result.flush).toEqual([]);
    expect(isHeld(result.gate)).toBe(false);
  });
});
