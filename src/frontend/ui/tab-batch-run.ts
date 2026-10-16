/**
 * **tab 栏停 / 起：交给那几台后端**（`sessions-stop` / `sessions-start`）—— 批量菜单与单个菜单同一条路，单个就是一个 sid 的一批。
 *
 * 一批里同一台机器的那几个**一次**调用发过去，那台逐个答（能不能做、怎么做都是那台判 —— 每一个同单个菜单那一项）；
 * 不同机器并发各发各的，一台失败不挡别台（那一台的几个记成失败、说清为什么）。
 * 本文件只做调用方那一侧：按台分组 · 组请求（每项只交 sid 与目录，用哪个号那台自己判；整批带用户设置的 resume 命令与模型偏好表原值）·
 * 按形状收 · 把每一个的结局说成一句人话 · 开终端那一形逐个开窗。
 */
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import { machineName, saidOfControl, settleTyped, unavailableSaid, type Refusals } from "./control-said";
import { isLocalOrigin, type Origin } from "./ipc/origin";
import { copyText } from "./copy-table";
import type { Tab } from "./tab-model";
import type { AccountAsk } from "./launch-account";
import type { Outcome } from "./generated/Outcome";
import { openLocalTerminal, openTerminal } from "./terminal-open";
import { failSaid } from "./kit/detail";

/** 一个 tab 的结局（`why` 是给人看的那一句；做成了 ⇒ 空串）。 */
export interface BatchOutcome {
  sid: string;
  outcome: "done" | "skipped" | "failed";
  why: string;
  /** 失败那一个的复制详情（那台写好）；没有 ⇒ 缺。 */
  detail?: string;
}

/** 后端答的一个（`sessions-*` 的 `results[i]`，类型从 Rust 生成）。`why` 是那台的原因码。 */
export type Reply = Outcome;

/** 一台的整体失败（通道不在 · 后端太旧 · 拒了整批）⇒ 那一台的每一个都记成失败、带同一句。 */
function machineFailed(sids: readonly string[], e: unknown): BatchOutcome[] {
  const why = saidOfControl(e);
  return sids.map((sid) => ({ sid, outcome: "failed", why }));
}

/** 整批被那台拒了（命令级的码）怎么说。 */
function batchRefusals(origin: Origin): Refusals {
  return {
    byCode: (_code, said) => copyText("tabBatch.why.batchRefused", { machine: machineName(origin), said }),
    noReason: () => copyText("tabBatch.why.batchRefused", { machine: machineName(origin), said: copyText("reason.io.unknown") }),
  };
}

/** 起那一项跳过 / 失败的原因码 ⇒ 人话（停的那一句那台写好，在 `said`）。机器由提示里那一行的标题说。 */
export function sayStart(r: Reply): string {
  switch (r.why) {
    case null:
      return "";
    case "no_tmux":
      return copyText("tabBatch.why.noTmux");
    case "ambiguous":
      return copyText("tabBatch.why.ambiguous", { names: r.detail });
    case "running":
      return copyText("tabBatch.why.running", { name: r.session ?? "" });
    case "session_already_live":
      return copyText("tabBatch.why.alreadyLive", { pids: r.detail });
    case "record_gone":
      return copyText("tabBatch.why.recordGone", { root: r.detail });
    case "account_unavailable":
      return copyText("tabBatch.why.accountGone", { name: r.detail });
    case "name_taken":
      return copyText("tabBatch.why.nameTaken", { name: r.session ?? "" });
    case "child_timed_out":
      return copyText("tabBatch.why.childTimedOut");
    default:
      return copyText("tabBatch.why.startFailed");
  }
}

function groupByOrigin(tabs: readonly Tab[]): Map<Origin, Tab[]> {
  const m = new Map<Origin, Tab[]>();
  for (const t of tabs) {
    const list = m.get(t.origin) ?? [];
    list.push(t);
    m.set(t.origin, list);
  }
  return m;
}

/** 那台握手时说过做不到 `op` ⇒ 那一台的每一个跳过（同单个菜单置灰的那一句）。 */
function offered(origin: Origin, op: string, sids: readonly string[]): BatchOutcome[] | null {
  const why = unavailableSaid(origin, op);
  return why === null ? null : sids.map((sid) => ({ sid, outcome: "skipped", why }));
}

/** 期限：单个那条 10 秒；一批按个数放宽（那台逐个做），每个多给 3 秒。 */
const BATCH_BASE_MS = 10_000;
const BATCH_EACH_MS = 6_000;

/** 停：问那台一次（`sids` 那几个），逐个答。问不到 / 形状不对 ⇒ 抛 `ControlError`。 */
export async function callStop(origin: Origin, sids: readonly string[]): Promise<Reply[]> {
  const budget = budgetWithin(BATCH_BASE_MS + BATCH_EACH_MS * sids.length);
  const v = await settleTyped(origin, "sessions-stop", callTyped(origin, "sessions-stop", { sids: [...sids] }, budget), batchRefusals(origin));
  return v.results;
}

/**
 * 起：问那台一次（`items` 那几个），逐个答。问不到 / 形状不对 ⇒ 抛 `ControlError`。
 * `agent` 缺 ＝ 流跟的那一家（标签页里的会话）；历史页里的会话按那一行自己的一家起。
 */
export async function callStart(origin: Origin, mode: "tmux" | "window", items: readonly StartItem[], agent: string): Promise<Reply[]> {
  const sids = items.map((i) => i.sid);
  const budget = budgetWithin(BATCH_BASE_MS + BATCH_EACH_MS * sids.length);
  // 启动命令 · 模型偏好表那台自己读（`prefs.json`），整批只交哪一家。
  const req = { mode, local: isLocalOrigin(origin), agent, items: [...items] };
  const v = await settleTyped(origin, "sessions-start", callTyped(origin, "sessions-start", req, budget), batchRefusals(origin));
  return v.results;
}

/** 停：每台一次 `sessions-stop`，各台并发。 */
export async function stopMany(tabs: readonly Tab[]): Promise<BatchOutcome[]> {
  const parts = await Promise.all(
    Array.from(groupByOrigin(tabs), async ([origin, list]): Promise<BatchOutcome[]> => {
      const sids = list.map((t) => t.sessionId);
      const no = offered(origin, "sessions-stop", sids);
      if (no) return no;
      try {
        return (await callStop(origin, sids)).map((r) => ({ sid: r.sid, outcome: r.outcome, why: r.said ?? "", ...(r.copyDetail !== "" ? { detail: r.copyDetail } : {}) }));
      } catch (e) {
        return machineFailed(sids, e);
      }
    }),
  );
  return parts.flat();
}

/**
 * 起会话要交给那台的一个（`sessions-start` 的 `items[i]`）：`account` 缺 ＝ 跟随（那台判）；
 * `fresh_terminal` ＝ 那台必铸新终端名、`fork_of` ＝ 源会话 sid（分叉出来的那一条：新名从源会话此刻的终端名铸）。
 */
export interface StartItem {
  sid: string;
  cwd: string;
  account?: AccountAsk;
}

/** 开终端那一形：后端渲好的那一行，monitor 开窗（本机同 `launch-local` 那条的开法，远端同 `openTerminal`）。开不了 ⇒ 那一句。 */
export async function openWindow(origin: Origin, cmd: string, cwd: string): Promise<string | null> {
  try {
    if (isLocalOrigin(origin)) await openLocalTerminal(cmd, cwd);
    else await openTerminal(origin, cmd);
    return null;
  } catch (e) {
    return failSaid(copyText("tabBatch.why.windowFailed"), e);
  }
}

/** 起：`mode` = `tmux`（在那台 tmux 里后台起，不开窗）· `window`（各开一个终端）。每台一次 `sessions-start`，各台并发。 */
export async function startMany(tabs: readonly Tab[], mode: "tmux" | "window"): Promise<BatchOutcome[]> {
  const parts = await Promise.all(
    Array.from(groupByOrigin(tabs), async ([origin, list]): Promise<BatchOutcome[]> => {
      const all = list.map((t) => t.sessionId);
      const no = offered(origin, "sessions-start", all);
      if (no) return no;
      // 一次 `sessions-start` 起一家：同一台上按会话的那一家分开交；还不知道是哪一家的那几个跳过（不落哪一家）。
      const out: BatchOutcome[] = list
        .filter((t) => t.agent === null)
        .map((t) => ({ sid: t.sessionId, outcome: "skipped" as const, why: copyText("tabBatch.why.agentUnknown") }));
      const byAgent = new Map<string, Tab[]>();
      for (const t of list) if (t.agent !== null) byAgent.set(t.agent, [...(byAgent.get(t.agent) ?? []), t]);
      for (const [agent, same] of byAgent) {
        const items: StartItem[] = same.map((t) => ({ sid: t.sessionId, cwd: t.projectDir ?? "" }));
        let replies: Reply[];
        try {
          replies = await callStart(origin, mode, items, agent);
        } catch (e) {
          out.push(...machineFailed(same.map((t) => t.sessionId), e));
          continue;
        }
        for (const [i, r] of replies.entries()) {
          let o: BatchOutcome = { sid: r.sid, outcome: r.outcome, why: sayStart(r), ...(r.copyDetail !== "" ? { detail: r.copyDetail } : {}) };
          if (r.outcome === "done" && mode === "window" && r.cmd !== null) {
            const failed = await openWindow(origin, r.cmd, items[i].cwd);
            if (failed !== null) o = { sid: r.sid, outcome: "failed", why: failed };
          }
          out.push(o);
        }
      }
      return all.map((sid) => out.find((o) => o.sid === sid)!).filter(Boolean);
    }),
  );
  return parts.flat();
}
