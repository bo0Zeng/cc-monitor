/**
 * 〔MIG-2 · `设计/99 §2.1 ⑬` · `01 §1.1`〕**起会话的计划与渲染问那台后端**（本机远端同一条 `chan.call(origin, …)`）：
 *
 * - `launch-render-cli` —— `ccm …` 调用行（渲不出来是诚实降级：`ok:false` ＋ 理由）；
 * - `launch-render-payload` —— 裸载荷 / 外层 tmux 三格（渲不出来是拒：坏输入，调用方不许换条路糊过去）；
 * - `launch-endpoint` —— 这一发往 `ANTHROPIC_BASE_URL` 里写哪个中转地址（`null` = 不注入；非它不可而中转不在 ⇒ 拒）；
 * - `launch-local` —— 本机起会话一整条（回要在新终端里跑的那一串 ＋ 身份 token），开窗口交 monitor（`open_local_terminal`）。
 *
 * 本文件只做调用方那一侧：发请求 · 按形状收（多一格缺一格都不收）· 失败说成一句（`control-said.ts::settle`）。
 * 原先这四样是 monitor 的 Tauri 命令（`render_ccm_launch` · `render_launch_payload` · `relay_endpoint_for_launch` ·
 * `new_local_session` / `resume_history_session` / `render_local_attach`），判定在 monitor 进程里。
 */
import { chan } from "./ipc/chan";
import { budgetWithin, jsonBody } from "./ipc/chan-caller";
import { ControlError, exactKeys, isObj, machineName, settle, unreadable, type Refusals } from "./control-said";
import type { Origin } from "./ipc/origin";
import { LOCAL_ORIGIN } from "./backend-policy";
import { commands } from "./ipc/commands";
import { AGENT_PROFILE, AGENT_ADAPTER_ID } from "./agent-profile";
import type { CliRenderRequest, PayloadRenderRequest } from "./launch-cli-wire";
import { copyText } from "./copy-table";

/** 渲染是纯函数；这个期限只挡「那台后端没在答」。 */
const RENDER_BUDGET_MS = 10_000;
/** 本机那条要探一次 `ccm`（`bash -lic`，子进程自带 5 s 期限）＋ 读凭据表 ＋ 探中转。 */
const LOCAL_BUDGET_MS = 20_000;

/** 那台后端拒了（码 `refused` / `relay_down`）：说的就是那一句，前面带上是哪台。 */
function refusals(origin: Origin): Refusals {
  return {
    byCode: (_code, detail) => copyText("launchRender.refusedOn", { machine: machineName(origin), said: detail }),
    noReason: () => copyText("remoteLaunchRun.renderCli.noReason"),
  };
}

/** 这次失败是那台后端拒了（坏输入 / 中转非它不可却不在）—— 换条路只会被同一道闸再拒一次。 */
export function isRefusal(e: unknown): boolean {
  return e instanceof ControlError && e.error?.layer === "peer" && e.error.why === "refused";
}

export type CliRendered = { ok: true; cmd: string } | { ok: false; reason: string };

/** `ccm …` 调用行。通道 / 形状上的失败抛（[`ControlError`]）；`ok:false` 是降级，不是错。 */
export async function renderCli(origin: Origin, req: CliRenderRequest): Promise<CliRendered> {
  const v = await settle(
    origin,
    "launch-render-cli",
    chan.call(origin, "launch-render-cli", jsonBody({ ...req }), budgetWithin(RENDER_BUDGET_MS)),
    refusals(origin),
  );
  if (!isObj(v) || !exactKeys(v, ["ok", "cmd", "reason"]) || typeof v.ok !== "boolean") {
    throw unreadable(origin, "launch-render-cli", "shape");
  }
  if (v.ok && typeof v.cmd === "string" && v.cmd !== "") return { ok: true, cmd: v.cmd };
  if (!v.ok && typeof v.reason === "string") return { ok: false, reason: v.reason };
  throw unreadable(origin, "launch-render-cli", "ok/cmd/reason");
}

/** 裸载荷 / 外层 tmux 三格。拒 ⇒ 抛（[`isRefusal`] 为真）。 */
export async function renderPayload(origin: Origin, req: PayloadRenderRequest): Promise<string> {
  const v = await settle(
    origin,
    "launch-render-payload",
    chan.call(origin, "launch-render-payload", jsonBody({ ...req }), budgetWithin(RENDER_BUDGET_MS)),
    refusals(origin),
  );
  if (!isObj(v) || !exactKeys(v, ["cmd"]) || typeof v.cmd !== "string" || v.cmd === "") {
    throw unreadable(origin, "launch-render-payload", "cmd");
  }
  return v.cmd;
}

/** 全量注入开关是 monitor 进程环境的一格（`设计/20 §3.2`「随入参交给那台后端」）。 */
async function allSessions(): Promise<boolean> {
  return commands.relay_all_sessions_switch();
}

/** 这一发的中转地址（`null` = 不注入）。拒 ⇒ 抛。`account` 是本机 / 远端载荷里「哪个号」那一格的线上形状。 */
export async function launchEndpoint(origin: Origin, account: Record<string, unknown>): Promise<string | null> {
  const body = { agent: AGENT_ADAPTER_ID, account, allSessions: await allSessions() };
  const v = await settle(
    origin,
    "launch-endpoint",
    chan.call(origin, "launch-endpoint", jsonBody(body), budgetWithin(RENDER_BUDGET_MS)),
    refusals(origin),
  );
  if (!isObj(v) || !exactKeys(v, ["baseUrl"]) || !(v.baseUrl === null || (typeof v.baseUrl === "string" && v.baseUrl !== ""))) {
    throw unreadable(origin, "launch-endpoint", "baseUrl");
  }
  return v.baseUrl;
}

/** 本机起会话那一问的动作。 */
export type LocalLaunchAction = { kind: "new" } | { kind: "resume"; sid: string } | { kind: "attach" };

export interface LocalLaunchRequest {
  action: LocalLaunchAction;
  /** 只用来核「新起」那一格的目录在不在。 */
  cwd: string | null;
  /** 自定义启动命令（空 / `null` = 没设）。 */
  launcher: string | null;
  /** 缺席 = 继承（没表态）；形状见生成物 `LOCAL_LAUNCH_ACCOUNT_WIRE`。 */
  account?: Record<string, unknown>;
  tmuxName: string | null;
}

/** 本机后端出的成品：要在新终端里跑的那一串 ＋ 铸进进程环境的身份 token（接回那一格 `null`）。 */
export interface LocalLaunchPlan {
  cmd: string;
  launchId: string | null;
}

/** 问本机后端要这次拉起的那一串（不开窗口）。拒 ⇒ 抛。 */
export async function planLocalLaunch(req: LocalLaunchRequest): Promise<LocalLaunchPlan> {
  const body = {
    action: req.action,
    cwd: req.cwd,
    launcher: req.launcher,
    ...(req.account === undefined ? {} : { account: req.account }),
    tmuxName: req.tmuxName,
    agent: {
      id: AGENT_ADAPTER_ID,
      defaultLauncher: AGENT_PROFILE.defaultLauncher,
      launcherAlias: AGENT_PROFILE.launcherAlias,
      resumeFlag: AGENT_PROFILE.resumeFlag,
    },
    allSessions: await allSessions(),
  };
  const v = await settle(
    LOCAL_ORIGIN,
    "launch-local",
    chan.call(LOCAL_ORIGIN, "launch-local", jsonBody(body), budgetWithin(LOCAL_BUDGET_MS)),
    refusals(LOCAL_ORIGIN),
  );
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

/** 本机起一个会话：问本机后端要那一串，交 monitor 在 `cwd` 开一个终端窗口跑它。回身份 token。失败抛（已说成一句）。 */
export async function launchLocal(req: LocalLaunchRequest, terminalCwd: string): Promise<string | null> {
  const plan = await planLocalLaunch(req);
  await commands.open_local_terminal({ cmd: plan.cmd, cwd: terminalCwd });
  return plan.launchId;
}
