/**
 * 设计/97 §8（主会话 09-28 裁 FIX4）「全景小程序卸口：给（受管工具都应可卸，照 SU1 装卸账）」—— 界面那一半。
 *
 * 钉：① 成品 ⇒ 那一句（卸掉了 · 没装 · 读不懂）；② 按钮先问一句，不点「确认」一个字节不发；点了问的是**当前那台**的 `panorama-uninstall`。
 */
import { describe, expect, it, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { PanoramaSection, saidOfUninstall } from "../../../../src/frontend/ui/settings/panorama-section";
import { setCurrentMachine } from "../../../../src/frontend/ui/settings/machine-context";
import { chanArgsJson, chanReply, type ChanCallArgs } from "../../../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const calls = () =>
  invokeMock.mock.calls.filter(([c]) => c === "chan_call").map(([, a]) => [(a as ChanCallArgs).origin, (a as ChanCallArgs).op, chanArgsJson(a as ChanCallArgs)]);

describe("FIX4 · 全景小程序卸口（界面）", () => {
  beforeEach(() => invokeMock.mockReset());

  it("成品 ⇒ 一句话", () => {
    expect(saidOfUninstall({ removed: true, path: "/h/.cc-monitor/bin/cc-monitor-panorama", index: "/h/.cc-monitor/panorama" })).toBe(
      "卸掉了：/h/.cc-monitor/bin/cc-monitor-panorama。索引还在 /h/.cc-monitor/panorama，要删自己删。",
    );
    expect(saidOfUninstall({ removed: false, path: "/h/x", index: "/h/i" })).toBe("这台没装代码全景组件（/h/x 不在）。");
    expect(() => saidOfUninstall({ removed: "yes" })).toThrow(/读不懂/);
  });

  it("先问一句：不确认不发；确认了问当前那台的 panorama-uninstall", async () => {
    invokeMock.mockImplementation(async () => chanReply({ removed: true, path: "/p", index: "/i" }));
    let answer = false;
    const s = new PanoramaSection(() => answer);
    setCurrentMachine("pi");
    const btn = s.element.querySelector("button")!;
    await s.run(btn);
    expect(calls()).toEqual([]);
    answer = true;
    await s.run(btn);
    expect(calls()).toEqual([["pi", "panorama-uninstall", {}]]);
    expect(s.element.textContent).toContain("卸掉了：/p");
  });
});
