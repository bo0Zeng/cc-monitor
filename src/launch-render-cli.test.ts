/**
 * launch-render-cli.ts 纯函数断言：canRenderCli 的诚实边界 + renderCli 的 token 产出。
 * 跑法：`tsx src/launch-render-cli.test.ts`。
 */
import { canRenderCli, renderCli } from "./launch-render-cli.ts";
import { buildLaunchPlan } from "./launch-plan.ts";
import type { CcmProbeResult } from "./ccm-probe.ts";
import type { LaunchContext } from "./launch-plan.ts";

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

if (failed > 0) {
  console.error(`\n${failed} launch-render-cli test(s) failed`);
  throw new Error(`launch-render-cli.test.ts: ${failed} failed`);
}
console.log("\nall launch-render-cli tests passed");
