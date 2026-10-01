/**
 * F03（unify-launch）：LaunchPlan IR —— 把「起一个会话」的意图结构化，取代 7 个 builder 各自
 * 散拼字符串。守核心思想：「一个动作 + 若干正交修饰」，且这个模型在 IR、CLI、
 * UI 三处是同一个。
 *
 * 两型分离：`LaunchContext` = 调用方已解析好的具体意图（输入）；`LaunchPlan` = 维度注册表
 * 跑完派生（`env`/`args`/`identity`）之后、渲染就绪的结构（输出）。`buildLaunchPlan()` 是唯一
 * 桥梁。生产的两份渲染器都在 Rust：载荷那份吃 `LaunchPlan` 摊成的上线请求
 * （`remote-launch-run.ts::buildPayloadRenderRequest`），`ccm …` 调用行那份吃 `{ctx, plan, probe}`
 * 摊成的上线请求（`buildCliRenderRequest`，它要 `LaunchContext` 里的账号名 / 模型 / ccmSid）。
 * 原先 TS 这边也有一份 CLI 渲染器（`launch-render-cli.ts`，向维度现问
 * 「这在 CLI 词汇里怎么说」），已删。
 *
 * **本次偏离草案的三处**（综合两版 Plan agent 方案后的取舍，理由见
 * `.claude/planned-build/unify-launch/features/F03-launch-plan-ir.md` §2）：
 * 1. `transport` 是零 payload 的 `{kind:"local"}|{kind:"ssh"}` 标记，不含 `origin`——`origin`
 *    是调度元数据不是渲染输入，继续作 `runLaunch`/`renderLaunchCommand` 的独立首参。
 * 2. `container.tmux.name` 配 `nameQuoting: "raw"|"quoted"`（判别式，非字符串形状嗅探）。
 * 3. `EnvOp` 用窄变体 `export-config-dir`（非通用 `{op:"export";key;value}`）——防止任何维度
 *    绕开 configDir 那道校验（今天住渲染侧 `payload.rs::config_dir_command_safe`）往命令里塞任意变量名（呼应账号隔离审计 D7 的 extraEnv key 无校验风险）。
 */
import { LAUNCH_DIMENSIONS } from "./launch-dimensions.ts";
// IR 的类型拆进纯类型叶子 `launch-types.ts`（断 `launch-plan ⇄ launch-dimensions` 那个类型环）。
import type { LaunchContext, LaunchPlan } from "./launch-types.ts";

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
