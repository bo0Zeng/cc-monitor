/**
 * F03（unify-launch）：environment 轴的维度注册表。
 *
 * 只治理**第三条正交轴**（MASTERPLAN §2.4 的 environment 轴）——agent（哪个 AI）与 container
 * （哪个容器）已经是 `LaunchPlan` 的一等字段，不注册成维度。加一个新维度（如 F07 的 `model`）=
 * 往 `LAUNCH_DIMENSIONS` 数组追加一条注册 + `LaunchContext` 加一个可选字段，零改
 * `buildLaunchPlan`、零改两个渲染器主体结构（MASTERPLAN §0.1 成功标准②的落点）。
 *
 * **顺序即契约**：`env-reset`(10) < `account`(20) < `nested-env-reset`(30)。这条顺序对应
 * 今天代码里"账号前缀在 unset 之前"的既有事实（`buildResumePayload` 逐字如此）——错序的后果
 * 是**静默账号被抹掉**，所以钉成模块加载即崩的断言（下方 `assertDimensionOrderInvariants`），
 * 而非留作注释纪律。
 */
import { isValidConfigDir, isValidModelName, isValidSessionId } from "./shell-quote.ts";
import { AGENT_PROFILE } from "./agent-profile.ts";
import type { LaunchDimension } from "./launch-plan.ts";
import { copyText } from "./copy-table";

/** identity：身份打标。只在调用方已知道 sid 时才生效（今天只有 tmux-create-resume 这条路径
 *  设；"开新 Claude"从不设，是已知 F04 缺口，本次原样保留、不顺手"修一半"）。 */
export const IDENTITY_DIMENSION: LaunchDimension = {
  id: "identity",
  order: 5,
  applies: (ctx) => ctx.ccmSid !== undefined,
  apply: (plan, ctx) => {
    if (!isValidSessionId(ctx.ccmSid!)) {
      throw new Error(copyText("launchDimensions.bad.ccmSid", { value: JSON.stringify(ctx.ccmSid) }));
    }
    plan.identity = { ccmSid: ctx.ccmSid! };
  },
};

/** env-reset：往「已存在的 idle tmux」send-keys 复用、且未选中账号时，先清残留
 *  `CLAUDE_CONFIG_DIR`（issue #75 复用变体逃生口，今天 = `buildResumeIntoExistingTmuxCmd` 的
 *  `envReset` 局部变量）。order 必须 < `ACCOUNT_DIMENSION.order`——纵使今天两者的 `applies`
 *  互斥（永不同时触发），这条顺序是"即便未来某次改动让二者同时 applies，也不会把刚 export 的
 *  账号被后到的 unset 抹掉"的结构性保险。 */
export const ENV_RESET_DIMENSION: LaunchDimension = {
  id: "env-reset",
  order: 10,
  applies: (ctx) =>
    ctx.container.kind === "tmux" && ctx.container.mode === "send-into" && ctx.account.kind !== "account",
  apply: (plan) => {
    plan.env.push({ kind: "unset-config-dir" }); // R04③：清哪个变量由 kind 决定，不再传自由 keys
  },
};

/** account：注入选中账号的 `CLAUDE_CONFIG_DIR`。order 必须 > `ENV_RESET_DIMENSION.order`。
 *
 *  F05：`applies` 恒 `true`（不再只在 `kind==="account"` 时触发）——账号维度必须在 CLI 语境下
 *  **永远显式表态**，`base` 态也要吐 `--base`，绝不能让这个维度对"未选账号"这个最常见场景
 *  沉默。**这条不是品味问题，是 F03 遗留的一个真实 bug**：`applies` 若只在 `kind==="account"`
 *  时为真，（当时的）TS 渲染器"任一维度 `cliFlags` 返回 `null` 就降级"检查根本不会跑到这个维度
 *  （`applies` 已是 `false`，循环直接跳过）——于是一个解析成"基座"的 plan，只要满足其余 CLI
 *  渲染条件，就会被 CLI 渲染器吐成一条**既不带 `--account` 也不带 `--base` 的 `ccm resume …`**，
 *  R11 的病灶原样复现（远端 shell 若没有继承 `CLAUDE_CONFIG_DIR`，`ccm` 会静默落 manifest 默认
 *  账号，可能不是用户想要的那个）。`apply()` 内部逻辑不变（`base` 态仍无 env op，字节不变）。
 *  〔LR1〕这条教训今天由 Rust `ccm_invocation.rs` 的 `account` 维度（`applies` 恒真）承接；
 *  本维度的 `applies` 恒真仍承重 —— 它决定 `apply()` 在 base 态也被问到。 */
export const ACCOUNT_DIMENSION: LaunchDimension = {
  id: "account",
  order: 20,
  applies: () => true,
  apply: (plan, ctx) => {
    if (ctx.account.kind !== "account") return;
    if (!isValidConfigDir(ctx.account.configDir)) {
      throw new Error(copyText("launchDimensions.bad.configDir", { value: JSON.stringify(ctx.account.configDir) }));
    }
    plan.env.push({ kind: "export-config-dir", value: ctx.account.configDir });
  },
  // 〔LR1 · U8c-3〕这里原来还有 `cliFlags`（`--base` / `--account <名>` / 名字缺失 ⇒ `null`）与
  // `requiredCaps`（`["account"]`）两格 —— 它们唯一的读者是 TS 那份 `ccm …` 渲染器，随它删了。
  // 同一件事今天只有一份：`src/bridge/src/backend/control/ccm_invocation.rs` 的 `account` 维度
  // （三形与 attach 豁免由 `ccm_invocation_tests.rs` 钉，`--base` 的跨语言契约由
  // `tests/base-flag-contract-guard.vitest.ts` 钉在那一份上）。
};

/** model（F07）：注入该账号配置的默认模型偏好（`ANTHROPIC_MODEL`）——**架构验收**：第一个真实
 *  新维度，验证「加一个新维度 = 注册一条 + `LaunchContext` 加一个可选字段，零改 `buildLaunchPlan`/
 *  两个渲染器主体结构」这条 MASTERPLAN §0.1 成功标准②的承诺。order 卡在 `account`(20) 与
 *  `nested-env-reset`(30) 之间——语义上"模型是账号的一个细化"，导出顺序上"账号目录先、模型
 *  偏好次、嵌套清理最后"。
 *
 *  `applies` 是**条件式**（`!!ctx.modelOverride`），不是像 `ACCOUNT_DIMENSION` 那样恒真——这两
 *  者看似同构（都是"账号相关的维度"）但问题结构不同，不能机械照搬 F05 的教训：F05 的坑是"最
 *  常见场景（未选账号=base）静默不表态，导致远端 `ccm` 落到 manifest 默认账号——一个和用户
 *  期望不同的身份"，危险在于"沉默 = 意外身份切换"。模型偏好没有这个坑：`applies` 为 `false`
 *  （没配偏好）时，远端 `claude` 直接用它自己已经配置好的默认模型——这**正是**用户没配置
 *  override 时应该发生的事，不是"意外切换成了别的模型"。一个维度该不该恒真，取决于"这个维度
 *  的默认态（不触发）是否等价于用户的期望"，不是"这个维度是不是账号相关"。 */
export const MODEL_DIMENSION: LaunchDimension = {
  id: "model",
  order: 25, // account(20) < model(25) < nested-env-reset(30)
  applies: (ctx) => !!ctx.modelOverride,
  apply: (plan, ctx) => {
    if (!ctx.modelOverride) return;
    if (!isValidModelName(ctx.modelOverride)) {
      throw new Error(copyText("launchDimensions.bad.model", { value: JSON.stringify(ctx.modelOverride) }));
    }
    plan.env.push({ kind: "export-model", value: ctx.modelOverride });
  },
  // 〔LR1 · U8c-3〕`cliFlags`（`--model <名>`）与 `requiredCaps`（`["model"]`）两格随 TS 渲染器删了；
  // 「条件式维度只在触发时才要能力」今天住 `ccm_invocation.rs` 的 `model` 维度。
};

/** nested-env-reset：resume/new 前清 Claude 嵌套会话标记（tmux server env 可能带毒，issue #24）。
 *  attach 不需要（不启动 agent）。order 必须 > `ACCOUNT_DIMENSION.order`（今天 export 恒在这条
 *  unset 之前，`buildResumePayload`/`buildLauncherCmd` 逐字如此）。 */
export const NESTED_ENV_RESET_DIMENSION: LaunchDimension = {
  id: "nested-env-reset",
  order: 30,
  applies: (ctx) => ctx.action.kind === "new" || ctx.action.kind === "resume",
  apply: (plan) => {
    if (AGENT_PROFILE.nestedEnvVars.length > 0) {
      plan.env.push({ kind: "unset-nested-env" }); // R04③：键表由 AGENT_PROFILE.nestedEnvVars 定，渲染器查
    }
  },
};

/**
 * 令牌形状的**唯一判据（TS 侧）**：32 个小写十六进制字符。
 *
 * **不收大写、不收 `0x` 前缀、不收连字符**——形状只有一种，为的是省掉「同一个令牌两种写法」
 * 这件事：本地那张 `token → HWND` 表与后端从 `environ` 读回来的串要能直接相等比较，
 * 中间不许有归一化步骤（归一化是「两侧各写一遍、各写错一遍」的经典落点）。
 *
 * ⚠ **它不做转义、也不替代转义**：渲染器照样 `posixQuote` 一遍（同 `export-model`）。
 * 这条校验买的是「变量值的形状」，不是「拼进 shell 安不安全」——
 * 那两件事在本仓是两道闸，不许合并成一道。
 *
 * Rust 同侧是 `src/bridge/src/backend/control/payload.rs::rbind_token_shape_ok`
 * （那边是 fail-closed 的 `Err`，不是 `throw`）。两侧形状由
 * `tests/rbind-token-shape-parity.vitest.ts` 的对拍钉住：**改 Rust 的长度或字符集，TS 这边会红**。
 */
export function isValidRbindToken(token: string): boolean {
  return /^[0-9a-f]{32}$/.test(token);
}

/**
 * 🔴 **rbind-token（`设计/80 §8` 步 1，2026-09-23）：启动期令牌 `CCM_RBIND_TOKEN`。**
 *
 * 「买到什么 / **买不到什么**」逐字住 `launch-plan.ts::EnvOp` 那一段
 * （要害一句：**令牌不许承载任何权限语义** —— 它会进 `/proc/<pid>/environ`、
 * `cmdline` 与 shell 历史）。这里只记三件与**维度机制**有关的事：
 *
 * **① `order` 是 40 —— 排在全部 `unset` 之后，刻意的。**
 * 今天没有任何 `unset` 变体碰得到 `CCM_RBIND_TOKEN`（`unset-config-dir` 清的是
 * `CLAUDE_CONFIG_DIR`，`unset-nested-env` 的键表是 `AGENT_PROFILE.nestedEnvVars`），
 * 所以排哪儿在**今天**都渲得对。排最后是为了**明天**：这一族的既有教训逐字是
 * 「即便未来某次改动让二者同时 applies，也不会把刚 export 的值被后到的 unset 抹掉」
 * （见 `ENV_RESET_DIMENSION` 那条注释）。⇒ 顺序钉成模块加载即崩的断言，不留作注释纪律。
 *
 * **② `applies` 是条件式，而且 attach 不带。**
 * 没令牌 ⇒ 这个维度整格不出现，载荷**逐字节等于今天**（这是步 1 能独立回滚的支点）。
 * `attach` 那一档一个 agent 进程都不起，而令牌的唯一消费者就是 agent 进程的 `environ`
 * （`§8.2` 那张图）⇒ 注进去没人会读它，只会白白多一处敏感值的落点。
 * 照 `NESTED_ENV_RESET_DIMENSION` 的同一条口径写成 `new`/`resume` 两档。
 *
 * **③ `ccm …` 调用行说不出它 —— 带令牌的 plan 一律走载荷渲染器，这是「诚实放弃」，不是缺口。**
 * `ccm` 今天没有承接这个令牌的 flag（能力清单里也没有对应的 cap），
 * 而 `INVARIANTS §33` 的铁律是**表达不了就必须放弃、不得近似**：
 * 若照渲，CLI 渲染器会吐出一条**丢了令牌**的 `ccm …` —— 那正是 R11/R08 那族
 * 「看起来生效了，只是少带了一样东西」的形状，而且是静默的。
 * 闸在 `remote-launch-run.ts::renderLaunchCommand`（按载荷里有没有这条 `EnvOp` 决定要不要试
 * CLI 那条路），判据在 `tests/remote-launch-run.vitest.ts`。生产的 CLI 渲染在 Rust 的
 * `backend::control::ccm_invocation`，它的 `CliSpec` 里没有这个维度 —— 等 `ccm` 学会了，
 * 那边加一个维度、这里的闸跟着撤。
 * 〔LR1 · U8c-3〕原来本维度还有一格 `cliFlags: () => null`（TS 渲染器那一侧的第二道闸），
 * 它唯一的读者是 TS 那份 `ccm …` 渲染器、本来就不在生产路上，随它删了 ⇒ **今天只有上面那一道**。
 */
export const RBIND_TOKEN_DIMENSION: LaunchDimension = {
  id: "rbind-token",
  order: 40, // 排在 nested-env-reset(30) 之后：export 不许被后到的 unset 抹掉
  // 🔴 **`!== undefined` 不是 `!!`**（本条判据第一次跑就把 `!!` 逮住了，如实记下）：
  // `""` 用 `!!` 判是**假**，于是一个空令牌会静默地读成「这次不带令牌」——
  // 那正是本仓 Z01 起的支点「**空值 ≠ 未设**」要防的形状（`payload.rs` 里
  // 「空串**不是**账号 0，是坏数据」是同一条）。`undefined` = 诚实的没有；
  // `""` = 铸币出了 bug，必须在 `apply` 里 throw，不许当没有。
  applies: (ctx) =>
    ctx.rbindToken !== undefined && (ctx.action.kind === "new" || ctx.action.kind === "resume"),
  apply: (plan, ctx) => {
    if (ctx.rbindToken === undefined) return;
    if (!isValidRbindToken(ctx.rbindToken)) {
      // 形状不对**不许降级成"这次不带令牌"** —— 那会把一次铸币 bug 变成一次
      // 「↗ 不明原因失效」，而 `§8.5 ②` 买的恰恰是「归因从四档猜变成一个布尔」。
      throw new Error(
        copyText("launchDimensions.bad.rbindToken", { value: JSON.stringify(ctx.rbindToken) }),
      );
    }
    plan.env.push({ kind: "export-rbind-token", value: ctx.rbindToken });
  },
};

export const LAUNCH_DIMENSIONS: LaunchDimension[] = [
  IDENTITY_DIMENSION,
  ENV_RESET_DIMENSION,
  ACCOUNT_DIMENSION,
  MODEL_DIMENSION,
  NESTED_ENV_RESET_DIMENSION,
  RBIND_TOKEN_DIMENSION,
].sort((a, b) => a.order - b.order);

/** 顺序不变量——模块加载即跑一次。顺序错了直接让进程/测试启动崩溃，不必等到某次真机 resume
 *  才发现账号被静默抹掉。 */
function assertDimensionOrderInvariants(dims: LaunchDimension[]): void {
  const seen = new Set<number>();
  for (const d of dims) {
    if (seen.has(d.order)) throw new Error(`bug: LaunchDimension order clash: ${d.id} order=${d.order}`); // 〔CP2b〕程序员错误，刻意英文（不是对外文案）
    seen.add(d.order);
  }
  const idx = (id: string): number => dims.findIndex((d) => d.id === id);
  if (idx("env-reset") >= idx("account")) {
    throw new Error("invariant: env-reset must come before account (no silent account override)");
  }
  if (idx("account") >= idx("nested-env-reset")) {
    throw new Error("invariant: account must come before nested-env-reset");
  }
  // F07：model 卡在 account 与 nested-env-reset 之间。
  if (idx("account") >= idx("model")) {
    throw new Error("invariant: account must come before model");
  }
  if (idx("model") >= idx("nested-env-reset")) {
    throw new Error("invariant: model must come before nested-env-reset");
  }
  // `设计/80 §8` 步 1：rbind-token 排在**全部 unset 之后**（`nested-env-reset` 是今天最后
  // 那个 unset）。理由见 `RBIND_TOKEN_DIMENSION` 头注 ①：防「明天多一个 unset 变体，
  // 把刚 export 的令牌抹掉」。⚠ 同上面四条：本函数只对**完整注册表**有意义
  // （缺项时 `findIndex` 回 -1，会报一条与真实病因无关的不变式违反）。
  if (idx("nested-env-reset") >= idx("rbind-token")) {
    throw new Error("invariant: rbind-token must come after nested-env-reset (unset would wipe the token)");
  }
}
assertDimensionOrderInvariants(LAUNCH_DIMENSIONS);

// 仅供测试注入错序数组验证断言真的会 throw（见 launch-dimensions.test.ts）。
export const __testOnlyAssertDimensionOrderInvariants = assertDimensionOrderInvariants;
