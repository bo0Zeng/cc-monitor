/**
 * **tab 栏停 / 起：交给那几台后端** —— 不同机器并发各发各的，一台失败不挡别台（那一台的几个记成失败、说清为什么）。
 *
 * - 停（`sessions-stop`）：同一台的那几个一次调用，那台逐个答；单个菜单就是一个 sid 的一批。
 * - 起（`session-open`，与单个恢复同一问）：同一台逐项挨个问，每项只交 sid 与放在哪（号 · 目录 · 哪一家那台从记录里读）；
 *   每项的结局取那台那一句；落在窗口里的那几项逐个开窗（[`BACKGROUND_OPENS`]）。
 */
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import { settleTyped, unavailableSaid } from "./control-said";
import { batchRefused } from "./sessions-where";
import { isLocalOrigin, type Origin } from "./ipc/origin";
import { copyText } from "./copy-table";
import type { Tab } from "./tab-model";
import type { Run } from "./generated/Run";
import { askOpen, BACKGROUND_OPENS } from "./session-open";
import type { Outcome } from "./generated/Outcome";
import { openLocalTerminal, openTerminal } from "./terminal-open";
import { failSaid, saidOf } from "./kit/detail";

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
  const why = saidOf(e);
  return sids.map((sid) => ({ sid, outcome: "failed", why }));
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

/** 停的期限：单个那条 10 秒；一批按个数放宽（那台逐个做），每个多给 6 秒。 */
const BATCH_BASE_MS = 10_000;
const BATCH_EACH_MS = 6_000;

/** 停：问那台一次（`sids` 那几个），逐个答。问不到 / 形状不对 ⇒ 抛 `ControlError`。 */
export async function callStop(origin: Origin, sids: readonly string[]): Promise<Reply[]> {
  const budget = budgetWithin(BATCH_BASE_MS + BATCH_EACH_MS * sids.length);
  const v = await settleTyped(origin, "sessions-stop", callTyped(origin, "sessions-stop", { sids: [...sids] }, budget), batchRefused(origin));
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

/** 开终端那一形：后端渲好的那一行，monitor 开窗（本机带起始目录，远端经 `openTerminal`）。开不了 ⇒ 那一句。 */
async function openWindow(origin: Origin, cmd: string, cwd: string | null): Promise<string | null> {
  try {
    if (isLocalOrigin(origin)) await openLocalTerminal(cmd, cwd);
    else await openTerminal(origin, cmd);
    return null;
  } catch (e) {
    return failSaid(copyText("tabBatch.why.windowFailed"), e);
  }
}

/** 起这一项：问那台；落在窗口里的开那一行（批量只开这一种）；本来就在跑的记跳过。 */
async function startOne(origin: Origin, sid: string, run: Run): Promise<BatchOutcome> {
  const end = await askOpen(origin, { what: { kind: "resume", sid }, run, local: isLocalOrigin(origin) });
  if (end.kind !== "opened") {
    return { sid, outcome: "failed", why: end.said, ...(end.detail !== "" ? { detail: end.detail } : {}) };
  }
  const o = end.opened;
  if (o.landed === "running") return { sid, outcome: "skipped", why: copyText("tabBatch.why.running", { name: o.session ?? "" }) };
  if (BACKGROUND_OPENS.has(o.landed) && o.line !== null) {
    const failed = await openWindow(origin, o.line, o.cwd);
    if (failed !== null) return { sid, outcome: "failed", why: failed };
  }
  return { sid, outcome: "done", why: "" };
}

/** 起：`run` = `tmux`（在那台 tmux 里后台起，不开窗）· `direct`（各开一个终端）。同一台逐项挨个，各台并发。 */
export async function startMany(tabs: readonly Tab[], run: Run): Promise<BatchOutcome[]> {
  const parts = await Promise.all(
    Array.from(groupByOrigin(tabs), async ([origin, list]): Promise<BatchOutcome[]> => {
      const no = offered(origin, "session-open", list.map((t) => t.sessionId));
      if (no) return no;
      const out: BatchOutcome[] = [];
      for (const t of list) out.push(await startOne(origin, t.sessionId, run));
      return out;
    }),
  );
  return parts.flat();
}
