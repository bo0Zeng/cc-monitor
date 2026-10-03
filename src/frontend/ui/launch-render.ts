/**
 * **起会话那一行问那台后端**（本机远端同一条 `chan.call(origin, …)`）—— 回的永远是一行 `ccm …`：
 *
 * - `launch-render-cli` —— 那台机器要跑的那一行（远端每一条 · 本机就地 resume 键入的那一行）；渲不出来是拒；
 * - `launch-local` —— 本机起会话那一行 ＋ 身份 token，开窗口交 monitor（`open_local_terminal`）。
 *
 * 环境、中转地址、身份标记由那台机器上的 `ccm` 自己做。本文件只做调用方那一侧：发请求 · 按形状收（多一格缺一格都不收）·
 * 失败说成一句（`control-said.ts::settle`）。
 */
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody } from "./ipc/chan-caller";
import { ControlError, exactKeys, isObj, machineName, settle, unreadable, type Refusals } from "./control-said";
import type { Origin } from "./ipc/origin";
import { LOCAL_ORIGIN } from "./backend-policy";
import { commands } from "./ipc/commands";
import { defaultLauncherOf } from "./agent-profile";
import type { CliRenderRequest } from "./launch-cli-wire";
import { copyText } from "./copy-table";

/** 渲染是纯函数（本机那条多问一次目录在不在）；这个期限只挡「那台后端没在答」。 */
const RENDER_BUDGET_MS = 10_000;

/** 那台后端拒了（码 `refused`）：说的就是那一句，前面带上是哪台。 */
function refusals(origin: Origin): Refusals {
  return {
    byCode: (_code, detail) => copyText("launchRender.refused.onMachine", { machine: machineName(origin), said: detail }),
    noReason: () => copyText("remoteLaunchRun.renderCli.noReason"),
  };
}

/** 这次失败是那台后端拒了（坏输入）—— 重来只会被同一道闸再拒一次。 */
export function isRefusal(e: unknown): boolean {
  return e instanceof ControlError && e.error?.layer === "peer" && e.error.why === "refused";
}

/** 那一行 `ccm …`。通道 / 形状上的失败与那台后端的拒都抛（[`ControlError`]；拒 ⇒ [`isRefusal`] 为真）。 */
export async function renderCli(origin: Origin, req: CliRenderRequest): Promise<string> {
  const body = jsonBody({ ...req });
  const budget = budgetWithin(RENDER_BUDGET_MS);
  const v = await settle(origin, "launch-render-cli", chan.call(origin, "launch-render-cli", body, budget), refusals(origin));
  if (!isObj(v) || !exactKeys(v, ["cmd"]) || typeof v.cmd !== "string" || v.cmd === "") {
    throw unreadable(origin, "launch-render-cli", "cmd");
  }
  return v.cmd;
}

/** 本机起会话那一问的动作。 */
export type LocalLaunchAction = { kind: "new" } | { kind: "resume"; sid: string } | { kind: "attach" };

export interface LocalLaunchRequest {
  /** 这个会话是哪一家（线上的 kind）。 */
  agent: string;
  action: LocalLaunchAction;
  /** 只用来核「新起」那一格的目录在不在。 */
  cwd: string | null;
  /** 自定义启动命令（空 / `null` = 没设）。 */
  launcher: string | null;
  /** 缺席 = 继承（没表态）；形状见生成物 `LOCAL_LAUNCH_ACCOUNT_WIRE`。 */
  account?: Record<string, unknown>;
  tmuxName: string | null;
}

/** 本机后端出的成品：要在新终端里跑的那一行 ＋ 交给 `ccm` 放进进程环境的身份 token（接回那一格 `null`）。 */
export interface LocalLaunchPlan {
  cmd: string;
  launchId: string | null;
}

/** 问本机后端要这次拉起的那一行（不开窗口）。拒 ⇒ 抛。 */
export async function planLocalLaunch(req: LocalLaunchRequest): Promise<LocalLaunchPlan> {
  const args = {
    agent: req.agent,
    action: req.action,
    cwd: req.cwd,
    launcher: req.launcher,
    ...(req.account === undefined ? {} : { account: req.account }),
    tmuxName: req.tmuxName,
    defaultLauncher: defaultLauncherOf(req.agent),
  };
  const body = jsonBody(args);
  const budget = budgetWithin(RENDER_BUDGET_MS);
  const v = await settle(LOCAL_ORIGIN, "launch-local", chan.call(LOCAL_ORIGIN, "launch-local", body, budget), refusals(LOCAL_ORIGIN));
  if (
    !isObj(v) ||
    !exactKeys(v, ["cmd", "launchId"]) ||
    typeof v.cmd !== "string" ||
    v.cmd === "" ||
    !(v.launchId === null || (typeof v.launchId === "string" && v.launchId !== ""))
  ) {
    throw unreadable(LOCAL_ORIGIN, "launch-local", "cmd/launchId");
  }
  return { cmd: v.cmd, launchId: v.launchId };
}

/** 本机起一个会话：问本机后端要那一行，交 monitor 在 `cwd` 开一个终端窗口跑它。回身份 token。失败抛（已说成一句）。
 *  ⚠ 类型上只收「新起 / resume」：接回（attach）只许经 [`planLocalLaunch`] 产串、交调用方自己那一跳开终端
 *  （`K-R106`：接回不是一次拉起，原先 monitor `launch_local` 入口那道闸今天是这条签名）。 */
export async function launchLocal(
  req: LocalLaunchRequest & { action: Exclude<LocalLaunchAction, { kind: "attach" }> },
  terminalCwd: string,
): Promise<string | null> {
  const plan = await planLocalLaunch(req);
  await commands.open_local_terminal({ cmd: plan.cmd, cwd: terminalCwd });
  return plan.launchId;
}
