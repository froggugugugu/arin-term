import { describe, expect, it } from "vitest";

import { APP_NAME } from "./index";

describe("src/logic の骨格", () => {
  it("アプリ名を返す", () => {
    expect(APP_NAME).toBe("Arin");
  });
});
