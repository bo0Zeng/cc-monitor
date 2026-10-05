/**
 * 分叉之后**真的把会话起起来** —— `fork-start.ts` 的编排接到真依赖上（追问小窗 · `sessions-start` · 开窗 / 接回 · 等那台报出）。
 *
 * 调用点有两个（历史查看器 `views/session-viewer.ts` 与实时 tab `tabs.ts`），「分叉完怎么起」在两边必须一模一样 ⇒ 接线只有这一份，
 * 调用点只交「哪台 · 新旧两个 sid · 那台推出的三格」（`session-fork` 回复的 `launch`）。
 *
 * 起：交那台 `sessions-start` 一项（带 `fresh_terminal` ＋ `fork_of`：那台必铸新终端名，从源会话此刻的终端名铸）。`tmux` 那一形起好后开一个终端接回去；
 * `window` 那一形拿那台渲好的那一行开窗。「起了」＝ 看见那台报出分叉出来的会话（等到才说「已分叉」；没等到那一句主窗口说过了）。
 */

// 分叉只对记录树那一家（流跟的那一家）的会话做 ⇒ 分叉出来的会话也是它。
import { ACTIVE_AGENT as FORK_AGENT } from "./agent-profile";
import { isLocalOrigin, isRemoteOrigin, type Origin } from "./ipc/origin";
import { toast } from "./kit/toast";
import { isSelectable, type Account } from "./accounts";
import { fetchAccounts, fetchLocalAccounts } from "./account-reads";
import { refuseUnavailableAccount } from "./launch-account";
import { askForkLaunch } from "./fork-ask";
import { startForkedSession, type ForkStart, type ForkStartDeps, type ForkStartOutcome } from "./fork-start";
import type { ForkLaunch } from "./session-writes";
import { callStart, openWindow, sayReply, type Reply } from "./tab-batch-run";
import { saidOfControl } from "./control-said";
import { runRemoteAttach } from "./remote-launch-run";
import { awaitArrival } from "./launch-arrival";
import { copyText } from "./copy-table";

export interface ForkFlowInput {
  /** 哪台机器（本机 = `LOCAL_ORIGIN`）。 */
  origin: Origin;
  /** 刚分叉出来的**新**会话 sid。 */
  newSessionId: string;
  /** **源**会话 sid。 */
  sourceSessionId: string;
  /** 那台推出的三格（`session-fork` 回复的 `launch`）。 */
  launch: ForkLaunch;
}

/**
 * 可选的号（名字）喂给追问小窗。查不到（账号功能没启用 / 远端不可达）→ **空清单**，
 * 小窗仍然弹、仍然能选「账号 0」—— 账号列不出来不该把整条分叉路堵死。
 */
async function listForkAccounts(origin: Origin): Promise<string[]> {
  try {
    const state = isLocalOrigin(origin) ? await fetchLocalAccounts() : await fetchAccounts(origin);
    return state.accounts.filter((a: Account) => isSelectable(a) && a.configDir !== null).map((a: Account) => a.name);
  } catch {
    return [];
  }
}

/** 交那台起这一项 → 开窗 / 接回 → 等那台报出。失败那一路自己出声、回 `false`；号选不了 ⇒ 给显式选择（点了以那个号再起一次）。 */
async function startFork(origin: Origin, s: ForkStart): Promise<boolean> {
  const failed = (said: string): false => {
    toast(copyText("forkFlow.runForkFlow.failed"), said);
    return false;
  };
  let r: Reply;
  try {
    [r] = await callStart(origin, s.mode, [s.item]);
  } catch (e) {
    return failed(saidOfControl(e));
  }
  if (r.unavailable) {
    refuseUnavailableAccount({
      machine: origin,
      u: r.unavailable,
      choose: (account) => launchAndSay(origin, { ...s, item: { ...s.item, account } }),
    });
    return false;
  }
  if (r.outcome !== "done") return failed(sayReply(origin, "start", r));
  if (s.mode === "window") {
    if (r.cmd === null) return failed(sayReply(origin, "start", r));
    const unopened = await openWindow(origin, r.cmd, s.item.cwd);
    if (unopened !== null) return failed(unopened);
  } else if (r.session !== null) {
    await runRemoteAttach(origin, FORK_AGENT, r.session, { quiet: true });
  }
  return awaitArrival({ origin, match: { sid: s.item.sid }, tmuxName: r.session, arrived: null });
}

/** 起成了说一句「已分叉」。 */
async function launchAndSay(origin: Origin, s: ForkStart): Promise<boolean> {
  const ok = await startFork(origin, s);
  if (ok) {
    toast(
      copyText("forkFlow.done.title"),
      copyText("forkFlow.done.body", { id: s.item.sid.slice(0, 8) }),
      { level: "info" },
    );
  }
  return ok;
}

/** 生产依赖。 */
function productionDeps(input: ForkFlowInput): ForkStartDeps {
  return {
    ask: async (launch, slots) =>
      askForkLaunch({
        launch,
        slots,
        accounts: await listForkAccounts(input.origin),
        // 远端会话惯例住在 tmux 里（断线能 attach 回来）；本机那条路不问终端那一格。
        defaultUseTmux: isRemoteOrigin(input.origin),
      }),
    start: (s) => launchAndSay(input.origin, s),
  };
}

/**
 * **分叉之后的全部事情**：编排（不知道的那几格问一次）→ 起 → 反馈。失败**必须可见**：编排里任何一步抛出来都变成提示，绝不静默。
 */
export async function runForkFlow(input: ForkFlowInput): Promise<ForkStartOutcome> {
  try {
    return await startForkedSession(
      { newSessionId: input.newSessionId, sourceSessionId: input.sourceSessionId, origin: input.origin, launch: input.launch },
      productionDeps(input),
    );
  } catch (err) {
    // 抛出来的（IPC reject…）在这里变成提示；返回 `failed` 而不是 `cancelled` —— 调用方据此区分「出错了」与「用户自己收手」。
    toast(copyText("forkFlow.runForkFlow.failed"), String(err));
    return "failed";
  }
}
