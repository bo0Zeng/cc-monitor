/**
 * F03（unify-launch）：LaunchPlan IR —— 把「起一个会话」的意图结构化，取代 7 个 builder 各自
 * 散拼字符串。守 MASTERPLAN §0 核心思想：「一个动作 + 若干正交修饰」，且这个模型在 IR、CLI、
 * UI 三处是同一个。
 *
 * 两型分离：`LaunchContext` = 调用方已解析好的具体意图（输入）；`LaunchPlan` = 维度注册表
 * 跑完派生（`env`/`args`/`identity`）之后、渲染就绪的结构（输出）。`buildLaunchPlan()` 是唯一
 * 桥梁——两个渲染器（`launch-render-fallback.ts`/`launch-render-cli.ts`）分别消费其中一种：
 * 兜底渲染器只吃 `LaunchPlan`（维度效果已摊平进文本原料，直接编译）；CLI 渲染器需要重新问
 * 维度一次"这在 CLI 词汇里怎么说"，故还需要 `LaunchContext`。
 *
 * **本次偏离 MASTERPLAN §2.5 草案的三处**（综合两版 Plan agent 方案后的取舍，理由见
 * `.claude/planned-build/unify-launch/features/F03-launch-plan-ir.md` §2）：
 * 1. `transport` 是零 payload 的 `{kind:"local"}|{kind:"ssh"}` 标记，不含 `origin`——`origin`
 *    是调度元数据不是渲染输入，继续作 `runLaunch`/`renderLaunchCommand` 的独立首参。
 * 2. `container.tmux.name` 配 `nameQuoting: "raw"|"quoted"`（判别式，非字符串形状嗅探）。
 * 3. `EnvOp` 用窄变体 `export-config-dir`（非通用 `{op:"export";key;value}`）——防止任何维度
 *    绕开 `isValidConfigDir` 往命令里塞任意变量名（呼应账号隔离审计 D7 的 extraEnv key 无校验风险）。
 */
import { LAUNCH_DIMENSIONS } from "./launch-dimensions.ts";

export type TmuxMode = "create-or-attach" | "send-into" | "attach-only";

export type LaunchContainer =
  | { kind: "none" }
  | { kind: "tmux"; name: string; nameQuoting: "raw" | "quoted"; mode: TmuxMode };

export type LaunchAction =
  | { kind: "new" }
  | { kind: "resume"; sid: string }
  | { kind: "attach"; name: string };

/** F05：账号名已线通——`kind==="account"` 带 `configDir`（供兜底渲染器 `export
 *  CLAUDE_CONFIG_DIR=<dir>`，这个字段自 F03 起就有）与**可选**的 `name`（供
 *  `ACCOUNT_DIMENSION.cliFlags` 吐 `--account <名>`）。`name` 可选而非必需——
 *  `remote-launch.ts` 保留的老式 builder 直调路径（`remote-launch.test.ts` 的 15 个符号，
 *  只传 `configDir` 不传名字）必须继续能触发账号注入，不能因为"不知道名字"就整个降级成
 *  `base`（那会让兜底渲染器也漏注入，是真回归，不是诚实降级）。`name` 缺失时
 *  `cliFlags` 对这一路 `null`（无法说出 `--account`，老实强制走兜底），`apply()`（兜底渲染器
 *  路径）不受影响，因为它只需要 `configDir`。**只有两态**（`account`/`base`），不存在"未决定"
 *  的第三态——上游 `resolveAccount`/`accountConfigDir` 已经替调用方做过这个决定（F05 计划
 *  §2 第3条：CLI 语境下账号维度必须恒显式表态，不能有"两者都不传"的沉默态，否则重蹈
 *  R11——`ccm` 会静默落 manifest 默认账号）。 */
export type LaunchAccount =
  | { kind: "account"; name?: string; configDir: string }
  | { kind: "base" };

/**
 * 有序、渲染时逐项原样吐出（不合并/不去重）——合并会破坏「与今天逐字节相同」：今天
 * `unset CLAUDE_CONFIG_DIR;` 与 `unset <嵌套env>;` 是两条独立语句，e2e 探针用
 * `grep -q "unset CLAUDE_CONFIG_DIR;"` 断言这个精确子串。
 */
export type EnvOp =
  | { kind: "export-config-dir"; value: string }
  | { kind: "unset"; keys: string[] };

/** `(inner) => string`：包裹而非片段追加——`( setup; exec cmd )` 这类闭括号结构，扁平的
 *  字符串追加没有槽位表达。F03 恒空数组，结构留给 F04（rbind 在兜底渲染器路径的落点）。 */
export interface WrapSpec {
  id: string;
  order: number;
  wrap: (inner: string) => string;
}

export interface LaunchPlan {
  transport: { kind: "local" } | { kind: "ssh" };
  action: LaunchAction;
  container: LaunchContainer;
  cwd: string | null;
  env: EnvOp[];
  /** 用户可配的原始启动器串（未 sanitize）——每个渲染器自己在嵌入点调用
   *  `sanitizeRemoteLauncher`，IR 只存意图，不存"已按哪种转义规则处理过"的产物。 */
  launcher: string;
  args: string[];
  identity?: { ccmSid: string };
  wrap: WrapSpec[];
}

/** `buildLaunchPlan` 的输入——调用方已解析好的具体意图，维度据此派生 `env`/`args`/`identity`。 */
export interface LaunchContext {
  transport: { kind: "local" } | { kind: "ssh" };
  action: LaunchAction;
  container: LaunchContainer;
  cwd: string | null;
  account: LaunchAccount;
  launcherOverride: string | undefined;
  ccmSid: string | undefined;
}

/**
 * 维度注册表的唯一契约。`apply` 就地改 `plan`（`env`/`args`/`identity` 等派生字段），
 * 绝不拼字符串——字符串化是渲染器的事。`cliFlags` 返回 `null` = 该维度在当前 `ctx` 下无法
 * 用 CLI 语法表达 → 强制整条 plan 降级走兜底渲染器（`canRenderCli` 消费这个信号）。
 */
export interface LaunchDimension {
  id: string;
  order: number;
  applies(ctx: LaunchContext): boolean;
  apply(plan: LaunchPlan, ctx: LaunchContext): void;
  cliFlags?(ctx: LaunchContext): string[] | null;
}

export function buildLaunchPlan(ctx: LaunchContext): LaunchPlan {
  const plan: LaunchPlan = {
    transport: ctx.transport,
    action: ctx.action,
    container: ctx.container,
    cwd: ctx.cwd,
    env: [],
    launcher: ctx.launcherOverride ?? "",
    args: [],
    wrap: [],
  };
  for (const dim of LAUNCH_DIMENSIONS) {
    if (dim.applies(ctx)) dim.apply(plan, ctx);
  }
  return plan;
}
