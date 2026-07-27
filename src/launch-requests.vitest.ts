// F06（unify-launch）：`planLocal` 单测——证明本地路径真的在用同一套维度注册表（不是套了个
// 类型皮的假装），并锁死实现期修正加的 sid 校验。纯函数，零 tauri/config 依赖，无需 mock。
import { describe, it, expect } from "vitest";
import { planLocal } from "./launch-requests";

describe("planLocal（F06：本地路径的 LaunchContext/LaunchPlan 构造）", () => {
  it("resume 动作 → ctx/plan 字段与输入一致（transport:local, container:none, account:base）", () => {
    const { ctx, plan } = planLocal({ kind: "resume", sid: "abc-123" }, "/home/p");
    expect(ctx.transport).toEqual({ kind: "local" });
    expect(ctx.container).toEqual({ kind: "none" });
    expect(ctx.account).toEqual({ kind: "base" });
    expect(ctx.cwd).toBe("/home/p");
    expect(plan.action).toEqual({ kind: "resume", sid: "abc-123" });
    expect(plan.cwd).toBe("/home/p");
  });

  it("new 动作 → plan.action 与输入一致", () => {
    const { plan } = planLocal({ kind: "new" }, "/home/p");
    expect(plan.action).toEqual({ kind: "new" });
  });

  it("cwd 为 null → ctx/plan.cwd 原样透传（不强行转空字符串）", () => {
    const { plan } = planLocal({ kind: "new" }, null);
    expect(plan.cwd).toBeNull();
  });

  it("非法 sid（含 shell 元字符）→ throw，不构造 ctx/plan（同其余 planXxx 的既有校验模式）", () => {
    expect(() => planLocal({ kind: "resume", sid: "a; rm -rf /" }, "/p")).toThrow(/非法 sessionId/);
    expect(() => planLocal({ kind: "resume", sid: "" }, "/p")).toThrow(/非法 sessionId/);
  });

  it("plan.env 因 nested-env-reset 维度恒非空（resume/new 都触发）——本地路径故意不消费它" +
     "（等价保护已在 lib.rs::scrub_env_vars 做完，见 features/F06-local-path-ir.md §0/§2）", () => {
    const { plan: resumePlan } = planLocal({ kind: "resume", sid: "abc" }, "/p");
    const { plan: newPlan } = planLocal({ kind: "new" }, "/p");
    expect(resumePlan.env.length).toBeGreaterThan(0);
    expect(newPlan.env.length).toBeGreaterThan(0);
  });

  it("account 维度对本地 base 态是无 env op 的 no-op（不因 F05 的 applies 恒真而误注入）", () => {
    const { plan } = planLocal({ kind: "new" }, "/p");
    expect(plan.env.some((op) => op.kind === "export-config-dir")).toBe(false);
  });
});
