/**
 * F03（unify-launch）：CLI 渲染器——把 `LaunchPlan`/`LaunchContext` 翻译成对 `ccm`（F02 统一
 * 启动 CLI）的一次调用。与兜底渲染器（`launch-render-fallback.ts`）的关键不对称：兜底渲染器
 * 只吃 `LaunchPlan`（维度效果已摊平进文本原料，直接编译）；本渲染器需要 `LaunchContext`——
 * 因为它要向每个已触发的维度**重新问一次**"这在 CLI 词汇里怎么说"（`cliFlags`），而不是编译
 * 已经摊平的文本。
 *
 * **`canRenderCli` 的诚实边界**（MASTERPLAN §2 已写明，这里是落地）：
 *  - 任一已触发维度的 `cliFlags(ctx)` 返回 `null` → 强制走兜底（今天只有 `account` 维度会，
 *    因为 F03 阶段调用方只有 `configDir` 没有账号「名字」，`ccm --account` 要名字——F05 移交点）。
 *  - `container.mode === "send-into"` → 强制走兜底。**这条是防 #76 复发的关键**：`shared/ccm`
 *    的 `--tmux` 只有幂等 create-or-attach 一种形态，没有「就地复用已存在 idle tmux、不新建」
 *    的模式；硬套会让 #76（claude 已退出但 tmux 还在时短路跳过 send-keys、把用户 attach 进空
 *    shell）以 CLI 路径的新形式复发。**诚实放弃，不近似**。
 *    **`attach-only` 不在此列**——`ccm attach <名>` 与 `shared/ccm` 源码核对，就是
 *    `exec tmux attach -t "=$名:"`，与兜底渲染器的 `SESSION_BACKEND.attach()` 逐字同构，没有
 *    #76 那种「幂等 create-or-attach vs 就地复用」的歧义，可以安全走 CLI 渲染器（F03 Phase D
 *    架构审计发现：早期实现把这两种模式并入同一把闸门，导致 `renderCli` 的 attach 分支和
 *    `CLI_REQUIRED_CAPS` 里的 `"attach"` 在生产路径上永不可达——已收窄）。
 */
import type { LaunchContext, LaunchPlan } from "./launch-plan.ts";
import { LAUNCH_DIMENSIONS } from "./launch-dimensions.ts";
import { sanitizeRemoteLauncher } from "./shell-quote.ts";
import { AGENT_PROFILE } from "./agent-profile.ts";
import type { CcmProbeResult } from "./ccm-probe.ts";

/** shell-safe 的 argv token quoting——**不做 denylist**（与 `sanitizeRemoteLauncher` 的语义
 *  刻意不同）：`ccm` 内部 `exec "${argv[@]}"` 是数组 exec，字符串里的 `;` 不构成注入面，
 *  这里只需要保证 outer 一整条 remoteCmd 字符串里这个 token 不越界。 */
function argv(token: string): string {
  return /^[A-Za-z0-9_@%+=:,./-]+$/.test(token) ? token : `'${token.replace(/'/g, `'\\''`)}'`;
}

/** CLI 语法覆盖面（对齐 `shared/ccm` 的 `--ccm-probe` 输出 `capabilities=`）。**不含 "account"**
 *  ——`ACCOUNT_DIMENSION.cliFlags` 恒 `null`，账号维度生效时 `canRenderCli` 已经因为那条规则
 *  返回 `false`，列不列 "account" 都不影响判定，但故意不列以文档化"F03 未覆盖账号 CLI 化"。 */
const CLI_REQUIRED_CAPS = ["new", "resume", "attach", "tmux", "cwd", "launcher", "ccm-sid"] as const;

export function canRenderCli(plan: LaunchPlan, ctx: LaunchContext, probe: CcmProbeResult): boolean {
  if (!probe.installed) return false;
  if (plan.transport.kind !== "ssh") return false; // local = F06，未实现
  if (!CLI_REQUIRED_CAPS.every((c) => probe.capabilities.has(c))) return false;
  if (plan.container.kind === "tmux" && plan.container.mode === "send-into") return false; // #76 防线：仅挡 idle-tmux 就地复用（attach-only 与 create-or-attach 都安全）
  for (const dim of LAUNCH_DIMENSIONS) {
    if (dim.applies(ctx) && dim.cliFlags && dim.cliFlags(ctx) === null) return false;
  }
  return true;
}

export function renderCli(plan: LaunchPlan, ctx: LaunchContext, ccmPath = "ccm"): string {
  const tokens: string[] = [ccmPath];

  if (plan.action.kind === "attach") {
    if (plan.container.kind !== "tmux") throw new Error("attach 必须是 tmux 容器");
    tokens.push("attach", plan.container.name);
    return tokens.map(argv).join(" "); // attach 分支不读其余修饰
  }

  tokens.push(plan.action.kind === "resume" ? "resume" : "new");
  if (plan.action.kind === "resume") tokens.push(plan.action.sid);

  if (plan.container.kind === "tmux") tokens.push(`--tmux=${plan.container.name}`);
  for (const dim of LAUNCH_DIMENSIONS) {
    if (!dim.applies(ctx)) continue;
    const flags = dim.cliFlags?.(ctx);
    if (flags) tokens.push(...flags);
  }
  if (plan.cwd) tokens.push("--cwd", plan.cwd);

  // 账号维度不在此处理：`canRenderCli` 已经对任何账号存在的 plan 返回 `false`（F03 阶段
  // 调用方只有 configDir 没有账号「名字」，见 ACCOUNT_DIMENSION.cliFlags）。F05 把账号名线通
  // 进 LaunchContext 后，这里要**恒显式传** `--account <名>` 或 `--base`——绝不依赖"不传等于
  // 基座"：R11 已证实 `ccm` 在两者都不传时会去查 manifest 的 isDefault 账号并静默覆盖调用方
  // 的意图（那是给交互终端用户的便利，不是给程序化调用方的默认值）。

  const safeLauncher = sanitizeRemoteLauncher(plan.launcher); // 与兜底渲染器同一函数、同一时机
  if (safeLauncher !== AGENT_PROFILE.defaultLauncher) tokens.push("--launcher", safeLauncher);
  if (plan.args.length > 0) tokens.push("--", ...plan.args);
  return tokens.map(argv).join(" ");
}
