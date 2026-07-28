/**
 * launch-render-cli.ts 纯函数断言：诚实边界 + token 产出。
 * R04①：`canRenderCli`/`renderCli` 已合成单一 `tryRenderCli`；下面两个薄 shim 让既有断言
 * （尤其 9 条黄金串）**逐字节不变**地经新入口走一遍——断言内容没放松，只是入口变了。
 * 跑法：`tsx src/launch-render-cli.test.ts`。
 */
import { tryRenderCli } from "./launch-render-cli.ts";
import { buildLaunchPlan } from "./launch-plan.ts";
import type { CcmProbeResult } from "./ccm-probe.ts";
import type { LaunchContext, LaunchPlan } from "./launch-plan.ts";

let failed = 0;
function test(name: string, fn: () => void): void {
  try {
    fn();
    console.log(`  ✓ ${name}`);
  } catch (e) {
    failed++;
    console.error(`  ✗ ${name}\n      ${e instanceof Error ? e.message : String(e)}`);
  }
}
function eq(a: unknown, b: unknown, msg?: string): void {
  if (a !== b) throw new Error(`${msg ?? "eq"}: expected ${JSON.stringify(b)}, got ${JSON.stringify(a)}`);
}

console.log("launch-render-cli.test.ts");

const FULL_CAPS: CcmProbeResult = {
  installed: true,
  version: "1",
  capabilities: new Set(["new", "resume", "attach", "tmux", "account", "model", "cwd", "agent", "launcher", "ccm-sid", "print"]),
};
const NOT_INSTALLED: CcmProbeResult = { installed: false, version: null, capabilities: new Set() };

function ctxOf(overrides: Partial<LaunchContext>): LaunchContext {
  return {
    transport: { kind: "ssh" },
    action: { kind: "resume", sid: "abc-123" },
    container: { kind: "tmux", name: "cc-abc12345", nameQuoting: "raw", mode: "create-or-attach" },
    cwd: "/p",
    account: { kind: "base" },
    launcherOverride: "claude",
    ccmSid: undefined,
    ...overrides,
  };
}

/** 只取"能不能"这一位，等价于旧 `canRenderCli`。 */
const canRenderCli = (plan: LaunchPlan, ctx: LaunchContext, probe: CcmProbeResult): boolean =>
  tryRenderCli(plan, ctx, probe).ok;
/** 取渲染结果；拿不到就抛（旧 `renderCli` 在这些用例里恒能渲染，故语义等价）。 */
const renderCli = (plan: LaunchPlan, ctx: LaunchContext): string => {
  const r = tryRenderCli(plan, ctx, FULL_CAPS);
  if (!r.ok) throw new Error(`预期可渲染却降级了: ${r.reason}`);
  return r.cmd;
};

test("canRenderCli：未探测到 ccm → false", () => {
  const ctx = ctxOf({});
  eq(canRenderCli(buildLaunchPlan(ctx), ctx, NOT_INSTALLED), false);
});
test("canRenderCli：装了 + create-or-attach + 无账号 → true", () => {
  const ctx = ctxOf({});
  eq(canRenderCli(buildLaunchPlan(ctx), ctx, FULL_CAPS), true);
});
test("canRenderCli：local transport → false（设计上恒不走这条渲染器，见 F06）", () => {
  const ctx = ctxOf({ transport: { kind: "local" } });
  eq(canRenderCli(buildLaunchPlan(ctx), ctx, FULL_CAPS), false);
});
// F05：账号名已线通——canRenderCli 不再对"有账号"一律降级，account/base 两态都能安全走 CLI
// （前提是 ccm 声明支持 "account" 能力）。
test("canRenderCli：账号维度存在（具名账号）+ ccm 支持 account 能力 → true", () => {
  const ctx = ctxOf({ account: { kind: "account", name: "z", configDir: "/home/u/.claude-accts/z" } });
  eq(canRenderCli(buildLaunchPlan(ctx), ctx, FULL_CAPS), true);
});
test("canRenderCli：ccm 不支持 account 能力（旧版本）→ false，即便只是 base 态也强制降级", () => {
  const noAccountCap: CcmProbeResult = {
    installed: true,
    version: "0.9",
    capabilities: new Set(["new", "resume", "attach", "tmux", "cwd", "agent", "launcher", "ccm-sid", "print"]),
  };
  const ctx = ctxOf({}); // 默认 base
  eq(canRenderCli(buildLaunchPlan(ctx), ctx, noAccountCap), false);
});

// F08：ccm 学会了 --model，关闭 R14①——cliFlags 不再恒 null。canRenderCli 现在靠一条针对性
// 特判（不是塞进 CLI_REQUIRED_CAPS：那会让所有未配模型偏好的会话也被迫要求 ccm 支持 model
// 能力，过度收紧）：只在这次会话真配了模型偏好时才要求 ccm 报告支持 "model"。
test("canRenderCli：配了 modelOverride 且 ccm 支持 model 能力 → true", () => {
  const ctx = ctxOf({ modelOverride: "opus" });
  eq(canRenderCli(buildLaunchPlan(ctx), ctx, FULL_CAPS), true);
});
test("canRenderCli：配了 modelOverride 但 ccm 不支持 model 能力（旧版本）→ false", () => {
  const noModelCap: CcmProbeResult = {
    installed: true,
    version: "1",
    capabilities: new Set(["new", "resume", "attach", "tmux", "account", "cwd", "agent", "launcher", "ccm-sid", "print"]),
  };
  const ctx = ctxOf({ modelOverride: "opus" });
  eq(canRenderCli(buildLaunchPlan(ctx), ctx, noModelCap), false);
});
test("canRenderCli：未配 modelOverride → 不受影响，仍 true（即便 ccm 不支持 model 能力）", () => {
  const noModelCap: CcmProbeResult = {
    installed: true,
    version: "1",
    capabilities: new Set(["new", "resume", "attach", "tmux", "account", "cwd", "agent", "launcher", "ccm-sid", "print"]),
  };
  const ctx = ctxOf({});
  eq(ctx.modelOverride, undefined);
  eq(canRenderCli(buildLaunchPlan(ctx), ctx, noModelCap), true);
});
test("renderCli：配了模型偏好 → --model <名>", () => {
  const ctx = ctxOf({ modelOverride: "opus" });
  const rendered = renderCli(buildLaunchPlan(ctx), ctx);
  eq(rendered.includes("--model opus"), true, rendered);
});

// **#76 防线——本组测试的核心价值**：shared/ccm 的 --tmux 只有幂等 create-or-attach 一种形态，
// 没有「就地复用已存在 idle tmux、不新建」的能力。`mode==="send-into"` 的 plan 必须强制走兜底
// 渲染器，否则会让 #76（claude 已退但 tmux 还在，短路跳过 send-keys，用户 attach 进空 shell）
// 以 CLI 路径的新形式复发——且现有回归测试测不到它（它们测的是 buildResumeIntoExistingTmuxCmd
// 这个 builder 的兜底路径，不测 renderCli）。
test("canRenderCli：send-into（idle-tmux 就地复用）→ 恒 false，即便装了 ccm 且能力齐全", () => {
  const ctx = ctxOf({ container: { kind: "tmux", name: "cc-s1", nameQuoting: "raw", mode: "send-into" } });
  eq(canRenderCli(buildLaunchPlan(ctx), ctx, FULL_CAPS), false, "#76 防线：诚实放弃，不近似渲染");
});
// attach-only 不受 #76 防线约束：`ccm attach <名>` 与 `shared/ccm` 源码核对就是
// `exec tmux attach -t "=$名:"`，与兜底渲染器的 SESSION_BACKEND.attach() 逐字同构，没有
// create-or-attach vs 就地复用那种歧义（Phase D 架构审计发现：早期实现把它也挡在闸门外，
// 导致 renderCli 的 attach 分支永不可达，已收窄——见 launch-render-cli.ts 头注）。
test("canRenderCli：attach-only + 装了 ccm 且能力齐全 → true（与 create-or-attach 同等安全）", () => {
  const ctx = ctxOf({ action: { kind: "attach", name: "cc-s1" }, container: { kind: "tmux", name: "cc-s1", nameQuoting: "quoted", mode: "attach-only" } });
  eq(canRenderCli(buildLaunchPlan(ctx), ctx, FULL_CAPS), true);
});
test("canRenderCli：attach-only 但探测未装 → false（探测失败/未装的普通降级，与 #76 防线无关）", () => {
  const ctx = ctxOf({ action: { kind: "attach", name: "cc-s1" }, container: { kind: "tmux", name: "cc-s1", nameQuoting: "quoted", mode: "attach-only" } });
  eq(canRenderCli(buildLaunchPlan(ctx), ctx, NOT_INSTALLED), false);
});

test("renderCli：resume + tmux 基本形态", () => {
  const ctx = ctxOf({});
  const plan = buildLaunchPlan(ctx);
  eq(renderCli(plan, ctx), "ccm resume abc-123 --tmux=cc-abc12345 --base --cwd /p");
});
test("renderCli：new 动作不带 sid", () => {
  const ctx = ctxOf({ action: { kind: "new" }, ccmSid: undefined });
  const plan = buildLaunchPlan(ctx);
  eq(renderCli(plan, ctx), "ccm new --tmux=cc-abc12345 --base --cwd /p");
});
test("renderCli：attach 只带名字，不读其余修饰", () => {
  const ctx = ctxOf({ action: { kind: "attach", name: "cc-s1" }, container: { kind: "tmux", name: "cc-s1", nameQuoting: "quoted", mode: "attach-only" } });
  const plan = buildLaunchPlan(ctx);
  eq(renderCli(plan, ctx), "ccm attach cc-s1");
});
test("renderCli：ccmSid → --ccm-sid flag", () => {
  const ctx = ctxOf({ ccmSid: "abc-123" });
  const plan = buildLaunchPlan(ctx);
  eq(renderCli(plan, ctx), "ccm resume abc-123 --tmux=cc-abc12345 --ccm-sid=abc-123 --base --cwd /p");
});
test("renderCli：自定义 launcher（非默认才带 flag）", () => {
  const ctx = ctxOf({ launcherOverride: "cct" });
  const plan = buildLaunchPlan(ctx);
  eq(renderCli(plan, ctx), "ccm resume abc-123 --tmux=cc-abc12345 --base --cwd /p --launcher cct");
});
test("renderCli：透传参数在 -- 之后", () => {
  const ctx = ctxOf({});
  const plan = buildLaunchPlan(ctx);
  plan.args.push("--model", "opus");
  eq(renderCli(plan, ctx), "ccm resume abc-123 --tmux=cc-abc12345 --base --cwd /p -- --model opus");
});
test("renderCli：cwd 含空格 → 正确 quote", () => {
  const ctx = ctxOf({ cwd: "/home/pi/my proj" });
  const plan = buildLaunchPlan(ctx);
  eq(renderCli(plan, ctx), "ccm resume abc-123 --tmux=cc-abc12345 --base --cwd '/home/pi/my proj'");
});
// F05：具名账号 → 吐 --account <名>，不是 --base。这条同时是 R11 同型 bug 修复的直接验证：
// 以前 kind==="account" 时 canRenderCli 恒 false（永远走不到这里），F05 后要能安全走 CLI 且
// 带对 flag，不能悄悄丢账号信息。
test("renderCli：具名账号 → --account <名>", () => {
  const ctx = ctxOf({ account: { kind: "account", name: "z", configDir: "/home/u/.claude-accts/z" } });
  const plan = buildLaunchPlan(ctx);
  eq(renderCli(plan, ctx), "ccm resume abc-123 --tmux=cc-abc12345 --account z --cwd /p");
});

// ---- R04①：诚实降级从"调用约定"升级为"结构保证" ----
// 改造前：`renderCli` 里 `if (flags) tokens.push(...)` 对 `cliFlags` 返回 `null` **静默跳过**，
// 于是会渲染出一条**丢了该修饰**的命令；安全性全靠调用方记得先问 `canRenderCli`。
// 现在 `null` 在同一次遍历里直接变成 `ok:false`——**拿不到命令**。
//
// 用的是真实可达的状态（不是编造的）：`LaunchAccount` 允许"有 configDir 但没有账号名字"
// （老式 `remote-launch.ts` 直调路径只给 configDir），此时 `ACCOUNT_DIMENSION.cliFlags`
// 老实返回 `null`——见 launch-requests.ts::accountOf 的头注。
test("R04①：账号有 configDir 但无名字 → cliFlags 返回 null → tryRenderCli 拿不到命令（ok:false）", () => {
  const ctx = ctxOf({ account: { kind: "account", configDir: "/h/z" } }); // 无 name
  const r = tryRenderCli(buildLaunchPlan(ctx), ctx, FULL_CAPS);
  eq(r.ok, false, "必须降级");
  eq(r.ok === false && /account/.test(r.reason), true, `reason 应点明是哪个维度: ${JSON.stringify(r)}`);
  // 要害：不存在"渲染出来了但少了 --account"这个中间态。
  eq("cmd" in r, false, "ok:false 时不该带 cmd");
});

test("R04①：ok:false 时 reason 说得出为什么（此前这个信息是丢掉的）", () => {
  const ctx = ctxOf({ container: { kind: "tmux", name: "cc-x", nameQuoting: "raw", mode: "send-into" } });
  const r = tryRenderCli(buildLaunchPlan(ctx), ctx, FULL_CAPS);
  eq(r.ok, false);
  eq(r.ok === false && /send-into/.test(r.reason), true, `#76 防线应自述: ${JSON.stringify(r)}`);
});

// ---- R04②：能力要求下放到维度后，"条件式维度只在触发时才要求能力"是结构保证 ----
// 上面 F08 那三条（配了 model + 无 model 能力 → false；未配 model + 无 model 能力 → true）
// 现在走的是 `MODEL_DIMENSION.requiredCaps`，不再是渲染器里的特判——它们仍全绿即证明等价。
// 这里补一条 account 的对称用例：account 能力从 CLI_REQUIRED_CAPS 移到维度后，仍必须被要求。
test("R04②：account 能力从静态列表移进维度后，缺它仍强制降级（语义未松）", () => {
  const ctx = ctxOf({ account: { kind: "account", name: "z", configDir: "/h/z" } });
  const noAccountCap: CcmProbeResult = {
    installed: true,
    version: "1",
    capabilities: new Set(["new", "resume", "attach", "tmux", "cwd", "launcher", "ccm-sid", "model"]),
  };
  const r = tryRenderCli(buildLaunchPlan(ctx), ctx, noAccountCap);
  eq(r.ok, false);
  eq(r.ok === false && /account/.test(r.reason), true, `reason 应点明缺 account 能力: ${JSON.stringify(r)}`);
});

// ---- attach 豁免组（R04 Phase D 审计要求：计划 §3.1 曾声称"已补测试"，实际没补，现补上）----
// `tryRenderCli` 的 attach 分支在维度循环**之前** return，故 attach 路径上一次放宽了**三**道闸门
// （审计逐条查实，不只 account 一道）：
//   ① ACCOUNT_DIMENSION.requiredCaps → ["account"]（旧在静态 CLI_REQUIRED_CAPS 里，无条件检查）
//   ② MODEL_DIMENSION.requiredCaps → ["model"]（旧是 canRenderCli 里的针对性特判，也在 attach 之前）
//   ③ INVARIANTS §33 铁律#1 本身：cliFlags → null 在 attach 上不再强制降级
// 放宽是**刻意**的：`ccm attach <名>` 不接受任何修饰 flag，要求这些能力是过度收紧。
// ② 的可达性经审计核实**是真的**：`model` 能力是 06a9c76（F08）才加进 `shared/ccm` 的
// `capabilities=`，所以装了 F02～F08 之间任一版 ccm 的远端就处在"缺 model"状态。
// 下面三条把这个豁免钉住——它是设计，不是"碰巧没人测到"。
test("attach 豁免①：ccm 不支持 account 能力，attach 仍走 CLI（不因修饰能力缺失而降级）", () => {
  const ctx = ctxOf({
    action: { kind: "attach", name: "cc-s1" },
    container: { kind: "tmux", name: "cc-s1", nameQuoting: "quoted", mode: "attach-only" },
    account: { kind: "account", name: "z", configDir: "/h/z" },
  });
  const noAccountCap: CcmProbeResult = {
    installed: true,
    version: "1",
    capabilities: new Set(["new", "resume", "attach", "tmux", "cwd", "launcher", "ccm-sid"]),
  };
  const r = tryRenderCli(buildLaunchPlan(ctx), ctx, noAccountCap);
  eq(r.ok, true, "attach 不该因缺 account 能力而降级");
  eq(r.ok === true && r.cmd, "ccm attach cc-s1");
});

test("attach 豁免②：配了 modelOverride 但 ccm 太旧不支持 model，attach 仍走 CLI（F02~F08 版 ccm 真实可达）", () => {
  const ctx = ctxOf({
    action: { kind: "attach", name: "cc-s1" },
    container: { kind: "tmux", name: "cc-s1", nameQuoting: "quoted", mode: "attach-only" },
    modelOverride: "opus",
  });
  const noModelCap: CcmProbeResult = {
    installed: true,
    version: "1",
    capabilities: new Set(["new", "resume", "attach", "tmux", "account", "cwd", "launcher", "ccm-sid"]),
  };
  const r = tryRenderCli(buildLaunchPlan(ctx), ctx, noModelCap);
  eq(r.ok, true, "attach 不读模型修饰，故不该因缺 model 能力而降级");
  eq(r.ok === true && r.cmd, "ccm attach cc-s1");
});

test("attach 豁免③：账号有 configDir 无名字（cliFlags→null）时 attach 仍走 CLI——§33 铁律#1 在此豁免", () => {
  const ctx = ctxOf({
    action: { kind: "attach", name: "cc-s1" },
    container: { kind: "tmux", name: "cc-s1", nameQuoting: "quoted", mode: "attach-only" },
    account: { kind: "account", configDir: "/h/z" }, // 无 name → ACCOUNT_DIMENSION.cliFlags 返回 null
  });
  const r = tryRenderCli(buildLaunchPlan(ctx), ctx, FULL_CAPS);
  eq(r.ok, true, "attach 命令里没有账号槽位，故 null 不构成'表达不出'");
  eq(r.ok === true && r.cmd, "ccm attach cc-s1");
});

if (failed > 0) {
  console.error(`\n${failed} launch-render-cli test(s) failed`);
  throw new Error(`launch-render-cli.test.ts: ${failed} failed`);
}
console.log("\nall launch-render-cli tests passed");
