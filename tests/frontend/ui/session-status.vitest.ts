// 活动态 ⇒ 点 / 灯：标签栏 · 状态点 · 总览共用的那一张表（`activityFace`）。
import { describe, it, expect } from "vitest";
import { activityFace } from "../../../src/frontend/ui/session-status";

describe("activityFace", () => {
  it("后端翻好的三态各对一颗点、一盏灯", () => {
    expect(activityFace("working")).toEqual({ dot: "running", light: "" });
    expect(activityFace("needs_you")).toEqual({ dot: "needs-you", light: "act-waiting" });
    expect(activityFace("idle")).toEqual({ dot: "idle", light: "act-idle" });
  });
  it("说不清 ⇒ 默认：在运行的点、不叠灯", () => {
    expect(activityFace(null)).toEqual({ dot: "running", light: "" });
  });
});
