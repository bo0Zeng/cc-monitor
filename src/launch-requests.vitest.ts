// `validateLocalLaunch`（F06 引入，R07 改名）单测——锁死本地路径的**前置校验**：sid 字符集，
// 以及 `transport:{kind:"local"}` 走一遍维度注册表不抛异常。纯函数，零 tauri/config 依赖，无需 mock。
//
// **R07 订正**：这段头注原写"证明本地路径真的在用同一套维度注册表（不是套了个类型皮的假装）"
// ——**那句是假的**。4 个生产调用点全部把返回值当语句丢弃，真命令由 Rust 独立构造
// （`history.rs::build_local_ps_command`）。本地路径**借** IR 做校验，但**不消费**它的输出，
// 而且这是 F06 论证过的设计（`Get-Command` 探测是 render-time 决策、只能在目标机做），
// 不是半成品。函数已随之改名并返回 `void`，见 `doc/INVARIANTS.md` §36。
import { describe, it, expect } from "vitest";
import { validateLocalLaunch, planResumeDirect, planResumeTmux } from "./launch-requests";
import { buildLaunchPlan } from "./launch-plan.ts";
import type { LaunchAction, LaunchContext } from "./launch-plan.ts";

describe("validateLocalLaunch（本地路径的前置校验；F06 引入、R07 改名并收成纯校验）", () => {
  it("非法 sid（含 shell 元字符 / 空串）→ throw，且抢在任何 IPC 之前（同其余 planXxx 的既有校验模式）", () => {
    expect(() => validateLocalLaunch({ kind: "resume", sid: "a; rm -rf /" }, "/p")).toThrow(
      /非法 sessionId/,
    );
    expect(() => validateLocalLaunch({ kind: "resume", sid: "" }, "/p")).toThrow(/非法 sessionId/);
  });

  // **这条是文档不是门禁**（Phase D 审计指出）：把 throw 整个删掉它照样绿，
  // 结构上不存在能让它红的变异。留着是为了说明"合法输入不该被拦"，别把它算进守护。
  it("合法输入不被拦（new 无 sid / resume 合法 sid / cwd 允许为 null）", () => {
    expect(() => validateLocalLaunch({ kind: "resume", sid: "abc-123" }, "/home/p")).not.toThrow();
    expect(() => validateLocalLaunch({ kind: "new" }, "/home/p")).not.toThrow();
    expect(() => validateLocalLaunch({ kind: "new" }, null)).not.toThrow();
  });
});

// R07：这一组的被测对象**不是** `validateLocalLaunch`，是**维度注册表在 `transport:local` 下的行为**
// ——它是 `doc/INVARIANTS.md` §36 那两条主张的证据。
//
// 为什么单独成组：`validateLocalLaunch` 现在**根本不构造 IR**（R07 Phase D 审计发现它内部那遍
// `buildLaunchPlan` 零门禁守护、且与生产无关，已删）。所以这些断言必须直接冲着 `buildLaunchPlan` 去，
// 不能借道那个函数——否则就是"通过一个已经不做这件事的函数去测这件事"。
describe("维度注册表在 transport:local 下的行为（INVARIANTS §36 的证据）", () => {
  const localCtx = (action: LaunchAction, cwd: string | null = "/p"): LaunchContext => ({
    transport: { kind: "local" },
    action,
    container: { kind: "none" },
    cwd,
    account: { kind: "base" },
    launcherOverride: undefined,
    ccmSid: undefined,
  });

  it("plan.env 因 nested-env-reset 维度恒非空（resume/new 都触发）——本地路径故意不消费它" +
     "（等价保护已在 lib.rs::scrub_env_vars 做完，见 features/F06-local-path-ir.md §0/§2）", () => {
    expect(buildLaunchPlan(localCtx({ kind: "resume", sid: "abc" })).env.length).toBeGreaterThan(0);
    expect(buildLaunchPlan(localCtx({ kind: "new" })).env.length).toBeGreaterThan(0);
  });

  it("account 维度对本地 base 态是无 env op 的 no-op（不因 F05 的 applies 恒真而误注入）", () => {
    const plan = buildLaunchPlan(localCtx({ kind: "new" }));
    expect(plan.env.some((op) => op.kind === "export-config-dir")).toBe(false);
  });

  it("本地 ctx 的其余字段照原样进 plan（transport/container/action/cwd）", () => {
    const plan = buildLaunchPlan(localCtx({ kind: "resume", sid: "abc-123" }));
    expect(plan.transport).toEqual({ kind: "local" });
    expect(plan.container).toEqual({ kind: "none" });
    expect(plan.action).toEqual({ kind: "resume", sid: "abc-123" });
    expect(plan.cwd).toBe("/p");
  });

  // Phase D 审计发现的**真覆盖丢失**：拆分前有一条钉 `cwd: null` 原样透传（不被转成 `""`），
  // 拆分后 `localCtx` 把 cwd 钉死成 `"/p"`，那条覆盖没了。审计实测：给 `buildLaunchPlan` 塞
  // `cwd: ctx.cwd ?? ""` 这个变异，改造前红、改造后**全仓 705 全绿**。
  // **这是共享代码**（`buildLaunchPlan` 远端路径也吃），不是本地专属，所以这条必须补回来。
  // （诚实标注：三个 `plan.cwd` 消费者今天都用真值判断，`""` 与 `null` 行为相同 → 当前是等价变异、
  // 低危；但哪天有人写 `plan.cwd !== null` 它就变成真缺口，钉住的成本近零。）
  it("cwd 为 null 时原样透传进 plan（不被悄悄转成空串）", () => {
    expect(buildLaunchPlan(localCtx({ kind: "new" }, null)).cwd).toBeNull();
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
