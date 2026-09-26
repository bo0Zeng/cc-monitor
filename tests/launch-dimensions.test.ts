/**
 * launch-dimensions.ts / launch-plan.ts 纯函数断言：每个维度的 applies/apply 独立行为（〔LR1〕cliFlags 那一格随 TS 渲染器删了）
 * + 顺序不变量 + buildLaunchPlan 端到端摊平。跑法：`tsx tests/launch-dimensions.test.ts`。
 *
 * 〔LR2〕维度的产品是 `plan.env` 那串 `EnvOp`（有序），**渲染成字节归 Rust**（`payload_tests.rs` ＋
 * 入库夹具 `payload-golden.json` / `tmux-outer-golden.json`）。本文件原来有几条拿 TS 兜底渲染器
 * 当量具比字节；那份渲染器零生产调用、按 `设计/00 §2.5 ④` 删了 ⇒ 那几条改比 `EnvOp` 序列（相等）。
 */
import {
  IDENTITY_DIMENSION,
  ENV_RESET_DIMENSION,
  ACCOUNT_DIMENSION,
  MODEL_DIMENSION,
  NESTED_ENV_RESET_DIMENSION,
  RBIND_TOKEN_DIMENSION,
  isValidRbindToken,
  LAUNCH_DIMENSIONS,
  __testOnlyAssertDimensionOrderInvariants,
} from "../src/launch-dimensions.ts";
import { buildLaunchPlan } from "../src/launch-plan.ts";
import type { LaunchContext, LaunchDimension, LaunchPlan } from "../src/launch-types.ts";

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
  container: { kind: "tmux", name: "cc-abc12345", nameQuoting: "raw", mode: "create" },
  cwd: "/p",
  account: { kind: "base" },
  launcherOverride: "claude",
  ccmSid: undefined,
};

test("identity：无 ccmSid → 不生效", () => {
  eq(IDENTITY_DIMENSION.applies(baseCtx), false);
});
test("identity：有 ccmSid → 设 plan.identity", () => {
  const ctx: LaunchContext = { ...baseCtx, ccmSid: "abc-123" };
  eq(IDENTITY_DIMENSION.applies(ctx), true);
  const plan: LaunchPlan = { transport: ctx.transport, action: ctx.action, container: ctx.container, cwd: ctx.cwd, env: [], launcher: "", args: [], wrap: [] };
  IDENTITY_DIMENSION.apply(plan, ctx);
  eq(plan.identity, { ccmSid: "abc-123" });
});
test("identity：非法 ccmSid → throw（拒绝拼入命令）", () => {
  const ctx: LaunchContext = { ...baseCtx, ccmSid: "; rm -rf /" };
  const plan: LaunchPlan = { transport: ctx.transport, action: ctx.action, container: ctx.container, cwd: ctx.cwd, env: [], launcher: "", args: [], wrap: [] };
  throws(() => IDENTITY_DIMENSION.apply(plan, ctx));
});

test("env-reset：仅在 tmux send-into 且无账号时生效", () => {
  eq(ENV_RESET_DIMENSION.applies(baseCtx), false, "create 不生效");
  const sendInto: LaunchContext = { ...baseCtx, container: { kind: "tmux", name: "cc-x", nameQuoting: "raw", mode: "send-into" } };
  eq(ENV_RESET_DIMENSION.applies(sendInto), true);
  const withAccount: LaunchContext = { ...sendInto, account: { kind: "account", name: "z", configDir: "/home/u/.claude-accts/z" } };
  eq(ENV_RESET_DIMENSION.applies(withAccount), false, "有账号时不生效（account 维度接管）");
});
test("env-reset：apply 追加 unset CLAUDE_CONFIG_DIR", () => {
  const ctx: LaunchContext = { ...baseCtx, container: { kind: "tmux", name: "cc-x", nameQuoting: "raw", mode: "send-into" } };
  const plan: LaunchPlan = { transport: ctx.transport, action: ctx.action, container: ctx.container, cwd: ctx.cwd, env: [], launcher: "", args: [], wrap: [] };
  ENV_RESET_DIMENSION.apply(plan, ctx);
  eq(plan.env, [{ kind: "unset-config-dir" }]); // R04③：收窄为无参变体
});

test("account：注入合法 configDir", () => {
  const ctx: LaunchContext = { ...baseCtx, account: { kind: "account", name: "z", configDir: "/home/u/.claude-accts/z" } };
  eq(ACCOUNT_DIMENSION.applies(ctx), true);
  const plan: LaunchPlan = { transport: ctx.transport, action: ctx.action, container: ctx.container, cwd: ctx.cwd, env: [], launcher: "", args: [], wrap: [] };
  ACCOUNT_DIMENSION.apply(plan, ctx);
  eq(plan.env, [{ kind: "export-config-dir", value: "/home/u/.claude-accts/z" }]);
});
// 〔DUP1 · `设计/90 §3` 判据 2〕这条原来是「非法 configDir → throw」—— 判它的是 TS 那份 `isValidConfigDir`〔散文墓碑〕，
// Rust 渲染侧 `config_dir_command_safe` 的逐项手抄。那份删了：前端不判、原样推，拼命令那一侧判
// （`payload_tests.rs` 逐码位钉着拒绝集；带 `REFUSE:` 标，前端照拒说出来）。这里钉「前端真的不再判」。
test("account：configDir 前端不判，原样推进 plan（判它的是 Rust 渲染侧）", () => {
  const ctx: LaunchContext = { ...baseCtx, account: { kind: "account", name: "z", configDir: "not-absolute" } };
  const plan: LaunchPlan = { transport: ctx.transport, action: ctx.action, container: ctx.container, cwd: ctx.cwd, env: [], launcher: "", args: [], wrap: [] };
  ACCOUNT_DIMENSION.apply(plan, ctx);
  eq(plan.env, [{ kind: "export-config-dir", value: "not-absolute" }]);
});
// F05：applies 恒真（base 态也要在 CLI 语境下显式表态，不再只在 kind==="account" 时触发——
// 这条修的是 F03 遗留的一个真实 bug，见 launch-dimensions.ts 头注/F05 计划 §2 第3条）。
test("account：applies 恒真（base 态也生效，不再「只在具名账号时才触发」）", () => {
  eq(ACCOUNT_DIMENSION.applies(baseCtx), true, "base 态也要 applies=true");
});
// 〔LR1 · U8c-3〕这里原来有两条测 `ACCOUNT_DIMENSION.cliFlags`（具名 ⇒ `--account <名>` · base ⇒ `--base`）。
// 那一格随 TS 渲染器删了；同一件事今天只在 Rust：`ccm_invocation_tests.rs::account_dimension_always_speaks_up_and_has_three_shapes`。

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
// 〔LR1 · U8c-3〕这里原来有两条测 `MODEL_DIMENSION.cliFlags`（`--model <名>` · 无偏好不被问到）。
// 前者随 TS 渲染器删了（今天在 `ccm_invocation_tests.rs::model_dimension_is_conditional_by_design`），
// 后者与上面「applies 恒假当无 modelOverride」逐字重复。
// F07 §4 步骤2：账号 ＋ 模型偏好 ⇒ `export-model`（order=25）排在 `export-config-dir` 之后、
// 嵌套 env 清理之前。〔LR2〕原来比的是 TS 兜底渲染器的字节位置；渲染归 Rust 之后改比 `EnvOp` 序列（相等）。
test("账号 + 模型偏好 → EnvOp 序列恰好是 [config-dir, model, nested-env]（order=25 的实际落点）", () => {
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
  eq(buildLaunchPlan(ctx).env, [
    { kind: "export-config-dir", value: "/home/u/.claude-accts/z" },
    { kind: "export-model", value: "opus" },
    { kind: "unset-nested-env" },
  ]);
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
    { kind: "unset-nested-env" },
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
    { kind: "unset-config-dir" },
    { kind: "unset-nested-env" },
  ]);
});
test("buildLaunchPlan：账号 + 模型偏好 → env 顺序是 export-config-dir → export-model → nested unset", () => {
  const ctx: LaunchContext = {
    transport: { kind: "ssh" },
    action: { kind: "resume", sid: "s1" },
    container: { kind: "tmux", name: "cc-s1", nameQuoting: "raw", mode: "create" },
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
    { kind: "unset-nested-env" },
  ]);
});
test("buildLaunchPlan：新建 + 已知 sid → identity 生效", () => {
  const ctx: LaunchContext = {
    transport: { kind: "ssh" },
    action: { kind: "resume", sid: "s1" },
    container: { kind: "tmux", name: "cc-s1", nameQuoting: "raw", mode: "create" },
    cwd: "/p",
    account: { kind: "base" },
    launcherOverride: "claude",
    ccmSid: "s1",
  };
  const plan = buildLaunchPlan(ctx);
  eq(plan.identity, { ccmSid: "s1" });
});


// ---- R04③：`unset` 侧收窄为无参变体（〔LR2〕字节那一半归 Rust 的 `payload-golden.json`「账号 0」那条）----
test("R04③：unset-config-dir / unset-nested-env 两格都出现，且顺序是 config-dir 在前", () => {
  // base 态 + resume 动作 → 两个 unset 都会触发（env-reset 清 CLAUDE_CONFIG_DIR、
  // nested-env-reset 清嵌套 env 全套）。收窄前是维度递 `keys: string[]`，现在由 kind 查表。
  const ctx: LaunchContext = {
    transport: { kind: "ssh" },
    action: { kind: "resume", sid: "abc-123" },
    // env-reset 只在「send-into 复用 idle tmux + 未选账号」时触发（见 ENV_RESET_DIMENSION.applies）
    // ——我初稿用 container:none 写错了前提，测试如实报红，据实修正。
    container: { kind: "tmux", name: "cc-x", nameQuoting: "raw", mode: "send-into" },
    cwd: null,
    account: { kind: "base" },
    launcherOverride: "claude",
    ccmSid: undefined,
  };
  eq(buildLaunchPlan(ctx).env, [{ kind: "unset-config-dir" }, { kind: "unset-nested-env" }]);
  // 类型层：EnvOp 的 unset 侧已无自由 keys 字段——任何维度都无法再往里塞任意变量名。
  const ops = buildLaunchPlan(ctx).env.filter((o) => o.kind.startsWith("unset"));
  eq(ops.length, 2);
  // 注：这条是**文档性断言，不计入守护**（R04 Phase D 审计建议 8）——给 `EnvOp` 加 `keys`
  // 会被 TS 的 excess property check 拦在编译期，构造不出任何类型安全的变异让它转红。
  eq(
    ops.every((o) => !("keys" in o)),
    true,
    "unset 变体不该再带自由 keys 字段",
  );
});

// ---- R04④：`WrapSpec` 折叠 ----
// 〔LR2〕这里原来有一条「wrap 纯数据折叠 —— order 升序由内向外，exec 不丢」，比的是 TS 兜底渲染器的字节。
// 折叠是**渲染**那一侧的事，今天只在 Rust（`payload.rs`），由入库夹具 `payload-golden.json`
// 「wrap 折叠（order 乱序给，必须按升序由内向外）」那条逐字节钉着；`plan.wrap` 生产恒空（零生产者）。

// ═══ `设计/80 §8` 步 1：启动期令牌（`CCM_RBIND_TOKEN`）══════════════════════════
//
// 🔴 这一族判据买的是三件**分开的**事，别把它们读成一件：
//   ① 形状闸真的会拦（大写 / 长度差一 / 非 hex / 空）——不是「看起来校验了」；
//   ② 顺序契约（令牌排在全部 unset 之后）是**模块加载即崩**的断言，不是注释纪律；
//   ③ 令牌在 `EnvOp` 序列里的落点（相等）。〔LR2〕渲染成字节那一半归 Rust，
//      由入库金标准（`payload-golden.json` / `tmux-outer-golden.json`）钉着。

const TOK = "0f1e2d3c4b5a69788796a5b4c3d2e1f0"; // 32 个小写 hex

/** 造一个只差 `rbindToken` 的 ctx —— 其余一律照 `baseCtx`，好让对照组只差这一维。 */
const tokCtx = (over: Partial<LaunchContext> = {}): LaunchContext => ({
  ...baseCtx,
  rbindToken: TOK,
  ...over,
});

test("rbind-token：没令牌 → 整格不生效（载荷逐字节等于今天，这是步 1 能独立回滚的支点）", () => {
  eq(RBIND_TOKEN_DIMENSION.applies(baseCtx), false);
  const env = buildLaunchPlan(baseCtx).env;
  eq(env.some((op) => op.kind === "export-rbind-token"), false, `不该出现令牌: ${JSON.stringify(env)}`);
});

// 🔴 **空值 ≠ 未设**（Z01 的支点）。这一条是本族**第一次跑就逮到东西**的那条：
//    `applies` 初版写的是 `!!ctx.rbindToken`，于是 `rbindToken: ""` 被读成
//    「这次不带令牌」—— 一次铸币 bug 会静默地变成「↗ 不明原因失效」。
//    现在 `""` 会让这个维度**触发**、在 `apply` 里 throw。
test("rbind-token：`\"\"` 是坏数据不是「没有」—— applies 为真、apply 当场 throw", () => {
  eq(RBIND_TOKEN_DIMENSION.applies(tokCtx({ rbindToken: "" })), true);
  // 对照组：`undefined` 才是「诚实的没有」，它**不**触发（否则上面那条只是「恒触发」）。
  eq(RBIND_TOKEN_DIMENSION.applies(tokCtx({ rbindToken: undefined })), false);
  throws(() => buildLaunchPlan(tokCtx({ rbindToken: "" })), "空令牌必须 throw");
});

test("rbind-token：有令牌 → applies 为真，且推出 export-rbind-token", () => {
  const ctx = tokCtx();
  eq(RBIND_TOKEN_DIMENSION.applies(ctx), true);
  const plan: LaunchPlan = { transport: ctx.transport, action: ctx.action, container: ctx.container, cwd: ctx.cwd, env: [], launcher: "", args: [], wrap: [] };
  RBIND_TOKEN_DIMENSION.apply(plan, ctx);
  eq(plan.env, [{ kind: "export-rbind-token", value: TOK }]);
});

test("rbind-token：attach 那一档不带（一个 agent 进程都不起，注进去没人读）", () => {
  eq(RBIND_TOKEN_DIMENSION.applies(tokCtx({ action: { kind: "attach", name: "cc-x" } })), false);
  // 对照组：new 与 resume 两档都带 —— 否则上面那条只是「所有档都不带」。
  eq(RBIND_TOKEN_DIMENSION.applies(tokCtx({ action: { kind: "new" } })), true);
  eq(RBIND_TOKEN_DIMENSION.applies(tokCtx({ action: { kind: "resume", sid: "s1" } })), true);
});

test("rbind-token：形状闸逐格 —— 大写 / 长度差一 / 非 hex / 空 一律拒（不是「看起来校验了」）", () => {
  // 正控先立：合法的那一个必须过（否则下面全红也说明不了什么）。
  eq(isValidRbindToken(TOK), true);
  const bad = [
    TOK.toUpperCase(),          // 大写：刻意不收（本地表与 environ 读回来的串要能直接相等比较）
    TOK.slice(0, 31),           // 31 位
    `${TOK}0`,                  // 33 位
    `${TOK.slice(0, 31)}g`,     // 非 hex
    "",                         // 空
    ` ${TOK}`,                  // 前导空白（`^…$` 锚点在不在）
    `${TOK}\n`,                 // 尾随换行（JS 正则 `$` 的经典陷阱）
    "0x0f1e2d3c4b5a69788796a5b",// 带 0x 前缀
  ];
  for (const b of bad) {
    eq(isValidRbindToken(b), false, `这个本该被拒: ${JSON.stringify(b)}`);
    const plan: LaunchPlan = { transport: baseCtx.transport, action: baseCtx.action, container: baseCtx.container, cwd: null, env: [], launcher: "", args: [], wrap: [] };
    throws(
      () => RBIND_TOKEN_DIMENSION.apply(plan, tokCtx({ rbindToken: b })),
      `形状不对必须 throw，不许静默降级成「这次不带令牌」: ${JSON.stringify(b)}`,
    );
  }
});

// 〔LR1 · U8c-3〕这里原来有一条「rbind-token：cliFlags 恒 null」。那一格是 TS 渲染器那一侧的闸，
// 随它删了；生产那道闸（带令牌就不试 `ccm …`）由 `tests/remote-launch-run.vitest.ts`
// 「带令牌 ⇒ 一次 `render_ccm_launch` 都不发」钉着。

test("顺序不变量：rbind-token 排在 nested-env-reset 之后（真实注册表）", () => {
  const idx = (id: string): number => LAUNCH_DIMENSIONS.findIndex((d) => d.id === id);
  eq(idx("rbind-token") >= 0, true, "rbind-token 不在注册表里了");
  eq(idx("nested-env-reset") < idx("rbind-token"), true);
});
test("顺序不变量：rbind-token 排到 nested-env-reset 之前 → throw（证明它不是摆设）", () => {
  const bad: LaunchDimension[] = [IDENTITY_DIMENSION, ENV_RESET_DIMENSION, ACCOUNT_DIMENSION, MODEL_DIMENSION, RBIND_TOKEN_DIMENSION, NESTED_ENV_RESET_DIMENSION];
  throws(
    () => __testOnlyAssertDimensionOrderInvariants(bad),
    "rbind-token 排在 nested-env-reset 前必须 throw（否则明天多一个 unset 变体就能抹掉令牌）",
  );
});

test("buildLaunchPlan：五种 EnvOp 同时出现时，令牌排在全部 unset 之后（顺序即契约）", () => {
  const ctx: LaunchContext = {
    transport: { kind: "ssh" },
    action: { kind: "resume", sid: "s1" },
    container: { kind: "tmux", name: "cc-s1", nameQuoting: "raw", mode: "create" },
    cwd: null,
    account: { kind: "account", name: "z", configDir: "/home/u/.claude-accts/z" },
    launcherOverride: "claude",
    ccmSid: undefined,
    modelOverride: "opus",
    rbindToken: TOK,
  };
  eq(buildLaunchPlan(ctx).env, [
    { kind: "export-config-dir", value: "/home/u/.claude-accts/z" },
    { kind: "export-model", value: "opus" },
    { kind: "unset-nested-env" },
    { kind: "export-rbind-token", value: TOK },
  ]);
});

// 〔LR2〕「令牌渲成的字节」那一半归 Rust：`payload-golden.json`「只有启动期令牌」逐字节钉着。
test("令牌那一格在 EnvOp 序列里恰好排在嵌套 env 清理之后（相等，不是包含）", () => {
  // `container:"none"` 那一档 —— `设计/80 §8.4` 表里「今天做不到 ↗ 的那一档」。
  const ctx: LaunchContext = {
    transport: { kind: "ssh" },
    action: { kind: "resume", sid: "s1" },
    container: { kind: "none" },
    cwd: null,
    account: { kind: "base" },
    launcherOverride: "claude",
    ccmSid: undefined,
    rbindToken: TOK,
  };
  eq(buildLaunchPlan(ctx).env, [
    { kind: "unset-nested-env" },
    { kind: "export-rbind-token", value: TOK },
  ]);
});

if (failed > 0) {
  console.error(`\n${failed} launch-dimensions test(s) failed`);
  throw new Error(`launch-dimensions.test.ts: ${failed} failed`);
}
console.log("\nall launch-dimensions tests passed");
