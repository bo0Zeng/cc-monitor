/**
 * **起会话那一行问那台后端**（本机远端同一条 `chan.call(origin, …)`）—— 回的永远是一行 `ccm …`：
 *
 * - `launch-render-cli` —— 那台机器要跑的那一行（远端每一条 · 本机就地 resume 键入的那一行）；渲不出来是拒；
 * - `launch-local` —— 本机起会话那一行 ＋ 身份 token，开窗口交 monitor（`open_local_terminal`）。
 *
 * 环境、中转地址、身份标记由那台机器上的 `ccm` 自己做。本文件只做调用方那一侧：发请求 · 失败说成一句（`control-said.ts::settleTyped`）。
 */
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import { machineName, settleTyped, type Refusals } from "./control-said";
import type { Origin } from "./ipc/origin";
import { LOCAL_ORIGIN } from "./backend-policy";
import { openLocalTerminal } from "./terminal-open";
import type { CliRenderRequest, CliRendered } from "./launch-cli-wire";
import type { AccountAsk } from "./generated/AccountAsk";
import type { Rendered } from "./generated/Rendered";
import { copyText } from "./copy-table";

/** 渲染是纯函数（本机那条多问一次目录在不在）；这个期限只挡「那台后端没在答」。 */
const RENDER_BUDGET_MS = 10_000;

/** 那台后端拒了（码 `refused`）：说的就是那一句，前面带上是哪台。 */
function refusals(origin: Origin): Refusals {
  return {
    byCode: (_code, detail) => copyText("launchRender.refused.onMachine", { machine: machineName(origin), said: detail }),
    noReason: () => copyText("reason.io.unknown"),
  };
}

/** 那一行 `ccm …` ＋ 那台判出来实际用的号（typed 帧命令：回包类型从 Rust 生成，形状由握手指纹核过）。通道上的失败与那台后端的拒都抛
 *  （[`ControlError`]；要的号选不了 ⇒ `launch-account.ts::accountUnavailableOf` 认得出）。 */
export async function renderCli(origin: Origin, req: CliRenderRequest): Promise<CliRendered> {
  const budget = budgetWithin(RENDER_BUDGET_MS);
  return settleTyped(origin, "launch-render-cli", callTyped(origin, "launch-render-cli", req, budget), refusals(origin));
}

/** 本机起会话那一问的动作（新起走 `session-new` 那一个请求，不经这里）。 */
export type LocalLaunchAction = { kind: "resume"; sid: string } | { kind: "attach" };

export interface LocalLaunchRequest {
  /** 这个会话是哪一家（线上的 kind）。 */
  agent: string;
  action: LocalLaunchAction;
  cwd: string | null;
  /** 缺席 = 不表态（继承；接回那一形）；跟随 / 账号 0 / 用户点名由本机后端判。 */
  account?: AccountAsk;
  tmuxName: string | null;
}

/** 本机后端出的成品：要在新终端里跑的那一行 ＋ 实际用的号。 */
export type LocalLaunchPlan = Rendered;

/** 问本机后端要这次拉起的那一行（不开窗口）。拒 ⇒ 抛。 */
export async function planLocalLaunch(req: LocalLaunchRequest): Promise<LocalLaunchPlan> {
  const args = {
    agent: req.agent,
    action: req.action,
    cwd: req.cwd,
    ...(req.account === undefined ? {} : { account: req.account }),
    tmuxName: req.tmuxName,
  };
  const budget = budgetWithin(RENDER_BUDGET_MS);
  return settleTyped(LOCAL_ORIGIN, "launch-local", callTyped(LOCAL_ORIGIN, "launch-local", args, budget), refusals(LOCAL_ORIGIN));
}

/** 本机起一个会话：问本机后端要那一行，交 monitor 在 `cwd` 开一个终端窗口跑它。回那份成品（`preflight` 说不起 ⇒ `null`，没开窗）。
 *  `preflight(configDir)`：开窗之前问一句（收的是本机后端判出来的那个号的目录；账号 0 / 不指定 ⇒ `undefined`）。失败抛（已说成一句）。
 *  ⚠ 类型上只收 resume：接回（attach）只许经 [`planLocalLaunch`] 产串、交调用方自己那一跳开终端
 *  （`K-R106`：接回不是一次拉起，原先 monitor `launch_local` 入口那道闸今天是这条签名）。 */
export async function launchLocal(
  req: LocalLaunchRequest & { action: Exclude<LocalLaunchAction, { kind: "attach" }> },
  terminalCwd: string,
  preflight?: (configDir: string | undefined) => Promise<boolean>,
): Promise<LocalLaunchPlan | null> {
  const plan = await planLocalLaunch(req);
  if (preflight && !(await preflight(plan.account?.configDir))) return null;
  await openLocalTerminal(plan.cmd, terminalCwd);
  return plan;
}
