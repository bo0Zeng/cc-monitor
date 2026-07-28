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

/** identity：身份打标。只在调用方已知道 sid 时才生效（今天只有 tmux-create-resume 这条路径
 *  设；"开新 Claude"从不设，是已知 F04 缺口，本次原样保留、不顺手"修一半"）。 */
export const IDENTITY_DIMENSION: LaunchDimension = {
  id: "identity",
  order: 5,
  applies: (ctx) => ctx.ccmSid !== undefined,
  apply: (plan, ctx) => {
    if (!isValidSessionId(ctx.ccmSid!)) {
      throw new Error(`非法 ccmSid（拒绝拼入命令）: ${JSON.stringify(ctx.ccmSid)}`);
    }
    plan.identity = { ccmSid: ctx.ccmSid! };
  },
  cliFlags: (ctx) => (ctx.ccmSid ? [`--ccm-sid=${ctx.ccmSid}`] : []),
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
    plan.env.push({ kind: "unset", keys: ["CLAUDE_CONFIG_DIR"] });
  },
  cliFlags: () => [], // ccm 内部按 --base/无--account 自行处理，无需专属 flag
};

/** account：注入选中账号的 `CLAUDE_CONFIG_DIR`。order 必须 > `ENV_RESET_DIMENSION.order`。
 *
 *  F05：`applies` 恒 `true`（不再只在 `kind==="account"` 时触发）——账号维度必须在 CLI 语境下
 *  **永远显式表态**，`base` 态也要吐 `--base`，绝不能让这个维度对"未选账号"这个最常见场景
 *  沉默。**这条不是品味问题，是 F03 遗留的一个真实 bug**：`applies` 若只在 `kind==="account"`
 *  时为真，`canRenderCli` 的"任一维度 `cliFlags` 返回 `null` 就降级"检查根本不会跑到这个维度
 *  （`applies` 已是 `false`，循环直接跳过）——于是一个解析成"基座"的 plan，只要满足其余 CLI
 *  渲染条件，就会被 `renderCli` 吐成一条**既不带 `--account` 也不带 `--base` 的 `ccm resume …`**，
 *  R11 的病灶原样复现（远端 shell 若没有继承 `CLAUDE_CONFIG_DIR`，`ccm` 会静默落 manifest 默认
 *  账号，可能不是用户想要的那个）。`apply()` 内部逻辑不变（`base` 态仍无 env op，字节不变）。 */
export const ACCOUNT_DIMENSION: LaunchDimension = {
  id: "account",
  order: 20,
  applies: () => true,
  apply: (plan, ctx) => {
    if (ctx.account.kind !== "account") return;
    if (!isValidConfigDir(ctx.account.configDir)) {
      throw new Error(`非法 CLAUDE_CONFIG_DIR（拒绝拼入命令）: ${JSON.stringify(ctx.account.configDir)}`);
    }
    plan.env.push({ kind: "export-config-dir", value: ctx.account.configDir });
  },
  // name 缺失（老式 remote-launch.ts 直调路径，只给 configDir 没给名字）→ null：老实说
  // "这个 plan 里我说不出 --account"，强制走兜底——不是遗漏，是 accountOf 的 name 参数
  // 本就是可选增强（见 LaunchAccount 类型头注）。
  cliFlags: (ctx) => {
    if (ctx.account.kind !== "account") return ["--base"];
    return ctx.account.name ? ["--account", ctx.account.name] : null;
  },
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
      throw new Error(`非法模型名（拒绝拼入命令）: ${JSON.stringify(ctx.modelOverride)}`);
    }
    plan.env.push({ kind: "export-model", value: ctx.modelOverride });
  },
  // F08：ccm 学会了 --model，关闭 R14①——不再恒 null。applies 已保证只有 modelOverride
  // truthy 时才会问到这里，`[]` 分支理论不可达，保留是防御性写法（同其余维度的既有风格）。
  cliFlags: (ctx) => (ctx.modelOverride ? ["--model", ctx.modelOverride] : []),
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
      plan.env.push({ kind: "unset", keys: [...AGENT_PROFILE.nestedEnvVars] });
    }
  },
  cliFlags: () => [], // ccm 内部恒清（agent_nested_env 按 agent 查表），无需专属 flag
};

export const LAUNCH_DIMENSIONS: LaunchDimension[] = [
  IDENTITY_DIMENSION,
  ENV_RESET_DIMENSION,
  ACCOUNT_DIMENSION,
  MODEL_DIMENSION,
  NESTED_ENV_RESET_DIMENSION,
].sort((a, b) => a.order - b.order);

/** 顺序不变量——模块加载即跑一次。顺序错了直接让进程/测试启动崩溃，不必等到某次真机 resume
 *  才发现账号被静默抹掉。 */
function assertDimensionOrderInvariants(dims: LaunchDimension[]): void {
  const seen = new Set<number>();
  for (const d of dims) {
    if (seen.has(d.order)) throw new Error(`LaunchDimension order 冲突: ${d.id} order=${d.order}`);
    seen.add(d.order);
  }
  const idx = (id: string): number => dims.findIndex((d) => d.id === id);
  if (idx("env-reset") >= idx("account")) {
    throw new Error("不变式违反：env-reset 必须排在 account 之前（防静默账号覆盖）");
  }
  if (idx("account") >= idx("nested-env-reset")) {
    throw new Error("不变式违反：account 必须排在 nested-env-reset 之前");
  }
  // F07：model 卡在 account 与 nested-env-reset 之间。
  if (idx("account") >= idx("model")) {
    throw new Error("不变式违反：account 必须排在 model 之前");
  }
  if (idx("model") >= idx("nested-env-reset")) {
    throw new Error("不变式违反：model 必须排在 nested-env-reset 之前");
  }
}
assertDimensionOrderInvariants(LAUNCH_DIMENSIONS);

// 仅供测试注入错序数组验证断言真的会 throw（见 launch-dimensions.test.ts）。
export const __testOnlyAssertDimensionOrderInvariants = assertDimensionOrderInvariants;
