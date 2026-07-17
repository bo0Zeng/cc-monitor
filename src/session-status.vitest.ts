// F91（#27）：会话活动状态共享纯逻辑 activityLightClass 断言。
// **单一事实源守护**：这些断言钉住 tab-bar（tabs.ts:updateTabButton）与 grid 共用的红绿灯语义。
import { describe, it, expect } from "vitest";
import { activityLightClass } from "./session-status";

describe("F91 activityLightClass", () => {
  it("idle / shell → act-idle（红：等输入）", () => {
    expect(activityLightClass("idle")).toBe("act-idle");
    expect(activityLightClass("shell")).toBe("act-idle");
  });
  it("waiting → act-waiting（黄：等决策）", () => {
    expect(activityLightClass("waiting")).toBe("act-waiting");
  });
  it("busy / null / 未知 → 空串（默认绿点）", () => {
    expect(activityLightClass("busy")).toBe("");
    expect(activityLightClass(null)).toBe("");
    expect(activityLightClass("something-new")).toBe("");
  });
});
