/**
 * launch-dimensions.ts / launch-plan.ts 纯函数断言：每个维度的 applies/apply/cliFlags 独立行为
 * + 顺序不变量 + buildLaunchPlan 端到端摊平。跑法：`tsx src/launch-dimensions.test.ts`。
 */
import {
  IDENTITY_DIMENSION,
  ENV_RESET_DIMENSION,
  ACCOUNT_DIMENSION,
  MODEL_DIMENSION,
  NESTED_ENV_RESET_DIMENSION,
  LAUNCH_DIMENSIONS,
  __testOnlyAssertDimensionOrderInvariants,
} from "./launch-dimensions.ts";
import { buildLaunchPlan } from "./launch-plan.ts";
import { renderFallback } from "./launch-render-fallback.ts";
import type { LaunchContext, LaunchDimension, LaunchPlan } from "./launch-plan.ts";

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
  if (JSON.stringify(a) !== JSON.stringify(b)) {
    throw new Error(`${msg ?? "eq"}: expected ${JSON.stringify(b)}, got ${JSON.stringify(a)}`);
  }
}
function throws(fn: () => void, msg?: string): void {
  try {
    fn();
  } catch {
    return;
  }
  throw new Error(msg ?? "expected throw, got none");
}

console.log("launch-dimensions.test.ts");

const baseCtx: LaunchContext = {
  transport: { kind: "ssh" },
  action: { kind: "resume", sid: "abc-123" },
  container: { kind: "tmux", name: "cc-abc12345", nameQuoting: "raw", mode: "create-or-attach" },
  cwd: "/p",
  account: { kind: "base" },
  launcherOverride: "claude",
  ccmSid: undefined,
};

test("identity：无 ccmSid → 不生效", () => {
  eq(IDENTITY_DIMENSION.applies(baseCtx), false);
});
test("identity：有 ccmSid → 设 plan.identity + 给出 --ccm-sid flag", () => {
  const ctx: LaunchContext = { ...baseCtx, ccmSid: "abc-123" };
  eq(IDENTITY_DIMENSION.applies(ctx), true);
  const plan: LaunchPlan = { transport: ctx.transport, action: ctx.action, container: ctx.container, cwd: ctx.cwd, env: [], launcher: "", args: [], wrap: [] };
  IDENTITY_DIMENSION.apply(plan, ctx);
  eq(plan.identity, { ccmSid: "abc-123" });
  eq(IDENTITY_DIMENSION.cliFlags!(ctx), ["--ccm-sid=abc-123"]);
});
test("identity：非法 ccmSid → throw（拒绝拼入命令）", () => {
  const ctx: LaunchContext = { ...baseCtx, ccmSid: "; rm -rf /" };
  const plan: LaunchPlan = { transport: ctx.transport, action: ctx.action, container: ctx.container, cwd: ctx.cwd, env: [], launcher: "", args: [], wrap: [] };
  throws(() => IDENTITY_DIMENSION.apply(plan, ctx));
});

test("env-reset：仅在 tmux send-into 且无账号时生效", () => {
  eq(ENV_RESET_DIMENSION.applies(baseCtx), false, "create-or-attach 不生效");
  const sendInto: LaunchContext = { ...baseCtx, container: { kind: "tmux", name: "cc-x", nameQuoting: "raw", mode: "send-into" } };
  eq(ENV_RESET_DIMENSION.applies(sendInto), true);
  const withAccount: LaunchContext = { ...sendInto, account: { kind: "account", name: "z", configDir: "/home/u/.claude-accts/z" } };
  eq(ENV_RESET_DIMENSION.applies(withAccount), false, "有账号时不生效（account 维度接管）");
});
test("env-reset：apply 追加 unset CLAUDE_CONFIG_DIR", () => {
  const ctx: LaunchContext = { ...baseCtx, container: { kind: "tmux", name: "cc-x", nameQuoting: "raw", mode: "send-into" } };
  const plan: LaunchPlan = { transport: ctx.transport, action: ctx.action, container: ctx.container, cwd: ctx.cwd, env: [], launcher: "", args: [], wrap: [] };
  ENV_RESET_DIMENSION.apply(plan, ctx);
  eq(plan.env, [{ kind: "unset", keys: ["CLAUDE_CONFIG_DIR"] }]);
});

test("account：注入合法 configDir", () => {
  const ctx: LaunchContext = { ...baseCtx, account: { kind: "account", name: "z", configDir: "/home/u/.claude-accts/z" } };
  eq(ACCOUNT_DIMENSION.applies(ctx), true);
  const plan: LaunchPlan = { transport: ctx.transport, action: ctx.action, container: ctx.container, cwd: ctx.cwd, env: [], launcher: "", args: [], wrap: [] };
  ACCOUNT_DIMENSION.apply(plan, ctx);
  eq(plan.env, [{ kind: "export-config-dir", value: "/home/u/.claude-accts/z" }]);
});
test("account：非法 configDir → throw", () => {
  const ctx: LaunchContext = { ...baseCtx, account: { kind: "account", name: "z", configDir: "not-absolute" } };
  const plan: LaunchPlan = { transport: ctx.transport, action: ctx.action, container: ctx.container, cwd: ctx.cwd, env: [], launcher: "", args: [], wrap: [] };
  throws(() => ACCOUNT_DIMENSION.apply(plan, ctx));
});
// F05：applies 恒真（base 态也要在 CLI 语境下显式表态，不再只在 kind==="account" 时触发——
// 这条修的是 F03 遗留的一个真实 bug，见 launch-dimensions.ts 头注/F05 计划 §2 第3条）。
test("account：applies 恒真（base 态也生效，不再「只在具名账号时才触发」）", () => {
  eq(ACCOUNT_DIMENSION.applies(baseCtx), true, "base 态也要 applies=true");
});
test("account：cliFlags 对具名账号吐 --account <名>", () => {
  const ctx: LaunchContext = { ...baseCtx, account: { kind: "account", name: "z", configDir: "/x" } };
  eq(ACCOUNT_DIMENSION.cliFlags!(ctx), ["--account", "z"]);
});
test("account：cliFlags 对 base 态吐 --base（不再返回 null——R11 同型 bug 修复）", () => {
  eq(ACCOUNT_DIMENSION.cliFlags!(baseCtx), ["--base"]);
});

// F07：模型偏好维度——applies 是条件式（不是恒真，见 launch-dimensions.ts 头注对比 F05 的教训）。
test("model：applies 恒假当无 modelOverride", () => {
  eq(MODEL_DIMENSION.applies(baseCtx), false);
});
test("model：applies 真当有 modelOverride", () => {
  const ctx: LaunchContext = { ...baseCtx, modelOverride: "opus" };
  eq(MODEL_DIMENSION.applies(ctx), true);
});
test("model：apply 对合法模型名推入 export-model", () => {
  const ctx: LaunchContext = { ...baseCtx, modelOverride: "claude-opus-4-5-20260101" };
  const plan: LaunchPlan = { transport: ctx.transport, action: ctx.action, container: ctx.container, cwd: ctx.cwd, env: [], launcher: "", args: [], wrap: [] };
  MODEL_DIMENSION.apply(plan, ctx);
  eq(plan.env, [{ kind: "export-model", value: "claude-opus-4-5-20260101" }]);
});
test("model：非法模型名 → throw（拒绝拼入命令）", () => {
  const ctx: LaunchContext = { ...baseCtx, modelOverride: "opus; rm -rf /" };
  const plan: LaunchPlan = { transport: ctx.transport, action: ctx.action, container: ctx.container, cwd: ctx.cwd, env: [], launcher: "", args: [], wrap: [] };
  throws(() => MODEL_DIMENSION.apply(plan, ctx));
});
// F08：ccm 学会了 --model，关闭 R14①——cliFlags 从恒 null 改成真吐 flag。
test("model：cliFlags 对配了偏好的会话吐 --model <名>", () => {
  const ctx: LaunchContext = { ...baseCtx, modelOverride: "opus" };
  eq(MODEL_DIMENSION.cliFlags!(ctx), ["--model", "opus"]);
});
test("model：applies 恒假时 cliFlags 不会被问到（无偏好场景不受影响）", () => {
  eq(MODEL_DIMENSION.applies(baseCtx), false);
});
// F07 §4 步骤2：renderFallback 整体黄金串——不只锁孤立的 apply() 输出，锁 order=25 在真实渲染
// 管线里的实际效果（子串位置在 export CLAUDE_CONFIG_DIR 之后、启动命令之前）。
test("renderFallback：账号 + 模型偏好 → 渲染出的字符串精确含 export ANTHROPIC_MODEL='opus'; ", () => {
  const ctx: LaunchContext = {
    transport: { kind: "ssh" },
    action: { kind: "resume", sid: "s1" },
    container: { kind: "none" },
    cwd: null,
    account: { kind: "account", name: "z", configDir: "/home/u/.claude-accts/z" },
    launcherOverride: "claude",
    ccmSid: undefined,
    modelOverride: "opus",
  };
  const plan = buildLaunchPlan(ctx);
  const rendered = renderFallback(plan);
  eq(rendered.includes("export ANTHROPIC_MODEL='opus'; "), true, `rendered=${rendered}`);
  const configDirIdx = rendered.indexOf("export CLAUDE_CONFIG_DIR");
  const modelIdx = rendered.indexOf("export ANTHROPIC_MODEL");
  // 用 "; claude --resume" 精确锚定启动命令本身——不能只找子串 "claude"（configDir 的路径
  // "/home/u/.claude-accts/z" 本身就含小写 "claude"，会假命中）。
  const argvIdx = rendered.indexOf("; claude --resume");
  eq(configDirIdx >= 0 && modelIdx > configDirIdx && argvIdx > modelIdx, true, `order wrong: ${rendered}`);
});

test("nested-env-reset：new/resume 生效，attach 不生效", () => {
  eq(NESTED_ENV_RESET_DIMENSION.applies(baseCtx), true);
  eq(NESTED_ENV_RESET_DIMENSION.applies({ ...baseCtx, action: { kind: "attach", name: "cc-x" } }), false);
  eq(NESTED_ENV_RESET_DIMENSION.applies({ ...baseCtx, action: { kind: "new" } }), true);
});

test("顺序不变量：env-reset < account < model < nested-env-reset（真实注册表）", () => {
  const idx = (id: string): number => LAUNCH_DIMENSIONS.findIndex((d) => d.id === id);
  eq(idx("env-reset") < idx("account"), true);
  eq(idx("account") < idx("model"), true);
  eq(idx("model") < idx("nested-env-reset"), true);
});
test("顺序不变量：故意错序 → 断言真的 throw（证明它不是摆设）", () => {
  const bad: LaunchDimension[] = [ACCOUNT_DIMENSION, ENV_RESET_DIMENSION, MODEL_DIMENSION, NESTED_ENV_RESET_DIMENSION];
  throws(() => __testOnlyAssertDimensionOrderInvariants(bad), "错序（account 排在 env-reset 前）必须 throw");
  const dup: LaunchDimension[] = [ENV_RESET_DIMENSION, { ...ACCOUNT_DIMENSION, order: ENV_RESET_DIMENSION.order }];
  throws(() => __testOnlyAssertDimensionOrderInvariants(dup), "order 冲突必须 throw");
});
// F07：新增两条顺序不变量（account < model < nested-env-reset）各自故意错序验证真 throw。
test("顺序不变量：model 排在 account 之前 → throw", () => {
  const bad: LaunchDimension[] = [MODEL_DIMENSION, IDENTITY_DIMENSION, ENV_RESET_DIMENSION, ACCOUNT_DIMENSION, NESTED_ENV_RESET_DIMENSION];
  throws(() => __testOnlyAssertDimensionOrderInvariants(bad), "model 排在 account 前必须 throw");
});
test("顺序不变量：model 排在 nested-env-reset 之后 → throw", () => {
  const bad: LaunchDimension[] = [IDENTITY_DIMENSION, ENV_RESET_DIMENSION, ACCOUNT_DIMENSION, NESTED_ENV_RESET_DIMENSION, MODEL_DIMENSION];
  throws(() => __testOnlyAssertDimensionOrderInvariants(bad), "model 排在 nested-env-reset 后必须 throw");
});

test("buildLaunchPlan：账号 + 就地复用（env-reset 不生效，因为有账号）→ 只有 account 的 export + nested unset", () => {
  const ctx: LaunchContext = {
    transport: { kind: "ssh" },
    action: { kind: "resume", sid: "s1" },
    container: { kind: "tmux", name: "cc-s1", nameQuoting: "raw", mode: "send-into" },
    cwd: null,
    account: { kind: "account", name: "z", configDir: "/home/u/.claude-accts/z" },
    launcherOverride: "claude",
    ccmSid: undefined,
  };
  const plan = buildLaunchPlan(ctx);
  eq(plan.env, [
    { kind: "export-config-dir", value: "/home/u/.claude-accts/z" },
    { kind: "unset", keys: ["CLAUDECODE", "CLAUDE_CODE_ENTRYPOINT", "CLAUDE_CODE_SESSION_ID", "CLAUDE_CODE_CHILD_SESSION"] },
  ]);
});
test("buildLaunchPlan：无账号 + 就地复用 → env-reset 的 unset 排在 nested unset 之前", () => {
  const ctx: LaunchContext = {
    transport: { kind: "ssh" },
    action: { kind: "resume", sid: "s1" },
    container: { kind: "tmux", name: "cc-s1", nameQuoting: "raw", mode: "send-into" },
    cwd: null,
    account: { kind: "base" },
    launcherOverride: "claude",
    ccmSid: undefined,
  };
  const plan = buildLaunchPlan(ctx);
  eq(plan.env, [
    { kind: "unset", keys: ["CLAUDE_CONFIG_DIR"] },
    { kind: "unset", keys: ["CLAUDECODE", "CLAUDE_CODE_ENTRYPOINT", "CLAUDE_CODE_SESSION_ID", "CLAUDE_CODE_CHILD_SESSION"] },
  ]);
});
test("buildLaunchPlan：账号 + 模型偏好 → env 顺序是 export-config-dir → export-model → nested unset", () => {
  const ctx: LaunchContext = {
    transport: { kind: "ssh" },
    action: { kind: "resume", sid: "s1" },
    container: { kind: "tmux", name: "cc-s1", nameQuoting: "raw", mode: "create-or-attach" },
    cwd: null,
    account: { kind: "account", name: "z", configDir: "/home/u/.claude-accts/z" },
    launcherOverride: "claude",
    ccmSid: undefined,
    modelOverride: "opus",
  };
  const plan = buildLaunchPlan(ctx);
  eq(plan.env, [
    { kind: "export-config-dir", value: "/home/u/.claude-accts/z" },
    { kind: "export-model", value: "opus" },
    { kind: "unset", keys: ["CLAUDECODE", "CLAUDE_CODE_ENTRYPOINT", "CLAUDE_CODE_SESSION_ID", "CLAUDE_CODE_CHILD_SESSION"] },
  ]);
});
test("buildLaunchPlan：新建 + 已知 sid → identity 生效", () => {
  const ctx: LaunchContext = {
    transport: { kind: "ssh" },
    action: { kind: "resume", sid: "s1" },
    container: { kind: "tmux", name: "cc-s1", nameQuoting: "raw", mode: "create-or-attach" },
    cwd: "/p",
    account: { kind: "base" },
    launcherOverride: "claude",
    ccmSid: "s1",
  };
  const plan = buildLaunchPlan(ctx);
  eq(plan.identity, { ccmSid: "s1" });
});

if (failed > 0) {
  console.error(`\n${failed} launch-dimensions test(s) failed`);
  throw new Error(`launch-dimensions.test.ts: ${failed} failed`);
}
console.log("\nall launch-dimensions tests passed");
