// 活着的会话那颗点 / 灯：按核心写好的语气排（标签栏 · 状态点 · 监控板 · 轮换名单共用 `activityFace`），不认活动态的码。
import { describe, it, expect } from "vitest";
import { activityFace, sessionDot } from "../../../src/frontend/ui/session-status";
import { ENDED, LIVE, RECONNECTABLE } from "../../../src/frontend/ui/tab-session-state";

describe("activityFace", () => {
  it("★ 核心的语气各对一颗点、一盏灯：now 在跑 · need 需手动 · 其余常规", () => {
    expect(activityFace("now")).toEqual({ dot: "running", light: "" });
    expect(activityFace("need")).toEqual({ dot: "needs-you", light: "act-waiting" });
    expect(activityFace("plain")).toEqual({ dot: "idle", light: "act-idle" });
  });
  it("还没收到那一格 ⇒ 在运行的点、不叠灯", () => {
    expect(activityFace(null)).toEqual({ dot: "running", light: "" });
  });
});

describe("sessionDot", () => {
  it("颜色按语气、形状按两轴：死了的语气再新也不算", () => {
    expect(sessionDot(LIVE, "plain")).toBe("idle");
    expect(sessionDot(LIVE, "need")).toBe("needs-you");
    expect(sessionDot(RECONNECTABLE, "now")).toBe("exited");
    expect(sessionDot(ENDED, "now")).toBe("ended");
  });
});
