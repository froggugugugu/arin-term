import { describe, expect, it } from "vitest";

import { createIpc, type ScreenLaunchOptions } from "../ipc/client";
import type { LaunchOptions } from "../ipc/generated/LaunchOptions";
import { createFakeIpc } from "../test/fakeIpc";
import { createWithLaunchOptions, DEFAULT_LAUNCH_OPTIONS } from "./launchOptions";

describe("createWithLaunchOptions(起動時の指定を取ってからエンジンを作る)", () => {
  it("指定を取得し終えてから、その指定でエンジンを作る。報告はしない", async () => {
    const events: string[] = [];
    let resolveOptions: ((options: LaunchOptions) => void) | undefined;
    const fake = createFakeIpc({
      get_launch_options: () =>
        new Promise<LaunchOptions>((resolve) => {
          events.push("get_launch_options");
          resolveOptions = resolve;
        }),
    });
    const reported: string[] = [];

    const pending = createWithLaunchOptions(createIpc(fake.transport), (m) => reported.push(m), (options) => {
      events.push(`create(${options.renderer},${options.unicode})`);
      return "engine";
    });
    await Promise.resolve();
    // 応答が届くまではエンジンを作らない。
    expect(events).toEqual(["get_launch_options"]);
    resolveOptions?.({ renderer: "webgl", unicode: "graphemes" });

    await expect(pending).resolves.toBe("engine");
    expect(events).toEqual(["get_launch_options", "create(webgl,graphemes)"]);
    expect(fake.calls.map((c) => c.command)).toEqual(["get_launch_options"]);
    expect(reported).toEqual([]);
  });

  it("取得に失敗したら、既定(dom・unicode11)で作り、原因を 1 行で報告する", async () => {
    const fake = createFakeIpc({
      get_launch_options: () => {
        throw "command get_launch_options not found";
      },
    });
    const reported: string[] = [];
    const created: ScreenLaunchOptions[] = [];

    await createWithLaunchOptions(createIpc(fake.transport), (m) => reported.push(m), (options) => {
      created.push(options);
    });

    expect(created).toEqual([DEFAULT_LAUNCH_OPTIONS]);
    expect(DEFAULT_LAUNCH_OPTIONS).toEqual({ renderer: "dom", unicode: "unicode11" });
    expect(reported).toEqual([
      "起動時の指定を取得できなかったので、既定(dom・unicode11)で起動する: command get_launch_options not found",
    ]);
  });
});
