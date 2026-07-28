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
  | { kind: "export-model"; value: string } // F07：每账号默认模型（ANTHROPIC_MODEL）
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

/**
 * R03：**正交修饰的传递载体**。
 *
 * 这三个字段是同一族东西——都是"修饰"（account 维度 + model 维度），此前却被摊成三个平级
 * 位置参数、在 4 个 `planXxx` + 5 个 `runXxx` 的**尾部逐字重复**。三个后果：
 *
 * 1. **MASTERPLAN §0.1 成功标准② 的"零改调用点"这一半**。那条标准要求"加一个新维度 =
 *    注册 dimension + CLI 加 flag + UI 加修饰项，零改 builder/renderer/调用点"。F07 做架构
 *    验收时渲染器主体确实零改，但 `modelOverride` 要从 UI 一路手动透传下来，于是 9 处签名
 *    同时改动（F07 的 commit message 自己记了这件事）。收进 bag 后，**其余 8 个函数签名与
 *    全部透传调用点零改**——实测口径：拿"加第 4 个维度"当尺子，
 *    `remote-launch-run.ts`(14 行)/`tabs.ts`(10)/`views/history.ts`(4)/`settings/remote-section.ts`(4)
 *    这 4 个文件共 32 行纯透传编辑归零。
 *
 *    **但"零改 builder"那一半没达成，别把这里读成"只需三处"**（R03 Phase D 对抗审计指出，
 *    此前本注释确实这么写过）：`launch-requests.ts` 的 4 个 `planXxx` 仍要各改 2 行
 *    （解构 + ctx 字面量），`LaunchContext` 也要同步加字段；若新维度需要新的 `EnvOp` 种类，
 *    `launch-render-fallback.ts` 还要加一个 switch 分支。真要闭合这一半，得让 `LaunchContext`
 *    持有一个**纯透传子集**（只搬不需要解析的字段——绝不能把 `configDir`/`accountName` 也搬进去，
 *    那会让"未解析的原始字段"与"已解析的 `account` 判别联合"并存，未来某个维度读了原始字段
 *    就绕过 `accountOf` 的解析，正是 R11 那一族病）。未做，登记在案。
 * 2. 消掉了"三个同类型可选尾参"这种签名形状。（**订正**：此前本注释说生产调用点"已经出现
 *    `undefined, undefined, "opus"` 这种占位实参"——审计核实 HEAD 上生产代码里**一个都没有**，
 *    只在测试代码里有。这条理由只对测试成立，不该拿来给方案抬价。）
 * 3. **最要紧的一条**：三个字段类型全是 `string | undefined` 且相邻，**传错顺序 tsc 抓不到**。
 *    `configDir` 与 `accountName` 互换编译照过，运行时行为却是"账号选择静默失效"——
 *    正是 R11/R08 那一族"看起来生效了，只是用了错的号"的形状。改成命名字段后，
 *    这一整类错误在编译期不可表达。
 *
 * 与 `LaunchContext` 不是重复：本接口是**解析前**的原始形态（调用方手上的一个目录 / 一个名字 /
 * 一个模型串），`LaunchContext.account` 是**解析后**的判别联合（`{kind:"account"|"base"}`）。
 * `planXxx` 正是这个转换发生的地方。
 *
 * 刻意**不**收 `name`（tmux 会话名）：容器轴按 R12 已决策维持为一等硬编码字段、不进维度注册表
 * （`doc/INVARIANTS.md` §38），混进"修饰 bag"会与那条决策矛盾。
 */
export interface LaunchModifiers {
  /** A4：账号目录，兜底渲染器据此 `export CLAUDE_CONFIG_DIR`。 */
  configDir?: string;
  /** F05：与 `configDir` 成对——CLI 渲染器据此吐 `--account <名>`；只有 `configDir` 没有名字时
   *  `ACCOUNT_DIMENSION.cliFlags` 诚实返回 `null` 强制降级（见 `LaunchAccount` 头注）。 */
  accountName?: string;
  /** F07：该账号配置的默认模型偏好（本机 `config.json`）。 */
  modelOverride?: string;
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
  /** F07：该账号配置的默认模型（本机 `config.json` 偏好，见 `accounts.ts::getModelForAccount`）。
   *  未设置 = `undefined`，`MODEL_DIMENSION.applies` 据此判断是否要注入——不是恒真，见
   *  `features/F07-per-account-model.md` §2 第1条：这个维度的默认态（不触发）就是用户的期望
   *  （该账号自身已配置好的默认模型），不是 F05 修的那种"沉默=意外身份切换"。 */
  modelOverride?: string;
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
