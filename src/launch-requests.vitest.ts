// F06（unify-launch）：`planLocal` 单测——证明本地路径真的在用同一套维度注册表（不是套了个
// 类型皮的假装），并锁死实现期修正加的 sid 校验。纯函数，零 tauri/config 依赖，无需 mock。
import { describe, it, expect } from "vitest";
import { planLocal, planResumeDirect, planResumeTmux } from "./launch-requests";

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

// R03：**类型层防线**——`LaunchModifiers` 存在的首要理由不是"参数少几个"，而是让
// "传错顺序"这一整类 bug 在编译期不可表达。改造前三个尾参类型全是 `string | undefined`
// 且相邻，`configDir` 与 `accountName` 互换 tsc 照过、运行时却是"账号选择静默失效"
// （R11/R08 那一族"看起来生效了，只是用了错的号"的形状）。
//
// 这些断言靠 `@ts-expect-error` 生效：若哪天有人把签名改回位置参数（或给 bag 加了
// 索引签名之类使其重新接受裸字符串），`@ts-expect-error` 会因为"预期的错误没有发生"
// 而**让 tsc 报错**——即这条测试的守护力来自 tsc，不是运行时。
describe("R03：修饰只能以命名字段传入（类型层）", () => {
  it("旧的位置参数形态编译失败；bag 形态编译通过且等价", () => {
    // @ts-expect-error 尾部三元组已收进 LaunchModifiers，裸字符串不再是合法实参
    planResumeDirect("abc-123", "/p", "claude", "/h/z");
    // @ts-expect-error 同上：第 5 个位置参数已不存在
    planResumeTmux("abc-123", "/p", "claude", "cc-p", "/h/z");

    // 正确形态：命名字段，顺序无关（下面两种写法必须产出同一个 ctx.account）
    const a = planResumeDirect("abc-123", "/p", "claude", { configDir: "/h/z", accountName: "z" });
    const b = planResumeDirect("abc-123", "/p", "claude", { accountName: "z", configDir: "/h/z" });
    expect(a.ctx.account).toEqual({ kind: "account", name: "z", configDir: "/h/z" });
    expect(a.ctx.account).toEqual(b.ctx.account);
  });

  // R03 Phase D 对抗审计发现（重要）：`planXxx` 里"解包 bag → 填 ctx"这一层**全仓零覆盖**。
  // 审计实做变异 M5：让 `planResumeTmux` 不消费 `mods.modelOverride`（等价于"tmux resume 路径上
  // 每账号默认模型静默失效"）→ tsc 无输出、`npm test` 699 全绿、`ccm-print-parity` 12 全绿，
  // **三道门全瞎**。根因：`launch-dimensions.test.ts`/`launch-render-cli.test.ts` 都手搓 ctx、
  // 从不经 planXxx；`tabs.vitest.ts` 把 `remote-launch-run` 整个 mock 掉；
  // `remote-launch.ts` 的 builder 只传 `{ configDir }`。
  // 这是 F07 遗留的缺口（改造前同样没有），但 R03 是最该补它的功能——我上面那条只断言了
  // `ctx.account`，把手边最该钉的 `modelOverride` 漏了，计划 §5 的"抽 1-2 条做变异检查"因此没做到位。
  it("三个修饰字段都真的落进 ctx（不只是 account）", () => {
    const mods = { configDir: "/h/z", accountName: "z", modelOverride: "opus" };
    const builds = [
      planResumeDirect("abc-123", "/p", "claude", mods),
      planResumeTmux("abc-123", "/p", "claude", "cc-p", mods),
    ];
    for (const b of builds) {
      expect(b.ctx.modelOverride).toBe("opus");
      expect(b.ctx.account).toEqual({ kind: "account", name: "z", configDir: "/h/z" });
    }
  });

  it("bag 缺省 = 基座（向下兼容：不传修饰等于今天不带账号的行为）", () => {
    expect(planResumeDirect("abc-123", "/p", "claude").ctx.account).toEqual({ kind: "base" });
    expect(planResumeDirect("abc-123", "/p", "claude", {}).ctx.account).toEqual({ kind: "base" });
  });
});
