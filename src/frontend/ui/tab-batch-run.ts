/**
 * **tab 栏停 / 起：交给后端**。
 *
 * - 批量停（`sessions-batch`）：选中的全部交本机后端一条，按台扇出 · 跳不跳过 · 合那一句都在后端，回包照摆；
 *   单个结束经 [`callStop`]（`sessions-stop`，那台逐个答）。
 * - 起（`session-open`，与单个恢复同一问）：不同机器并发、一台失败不挡别台；同一台逐项挨个问，每项只交 sid 与放在哪（号 · 目录 · 哪一家那台从记录里读）；
 *   每项的结局取那台那一句；落在窗口里的那几项逐个开窗（[`BACKGROUND_OPENS`]）。
 */
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import { settleTyped, unavailableSaid } from "./control-said";
import { batchRefused } from "./sessions-where";
import { isLocalOrigin, LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import type { SessionsBatchReply } from "./generated/SessionsBatchReply";
import { copyText } from "./copy-table";
import type { Tab } from "./tab-model";
import type { Launch } from "./generated/Launch";
import { askOpen, BACKGROUND_OPENS } from "./session-open";
import type { SessionOutcome as Outcome } from "./generated/SessionOutcome";
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

/** 批量停：选中的全部（照选中的先后）交本机后端一条 `sessions-batch`；回包是合好的那一份。问不到本机后端 ⇒ 抛。 */
export async function stopMany(tabs: readonly Tab[]): Promise<SessionsBatchReply> {
  const budget = budgetWithin(BATCH_BASE_MS + BATCH_EACH_MS * tabs.length);
  const req = { op: "stop" as const, items: tabs.map((t) => ({ machine: t.origin, sid: t.sessionId })) };
  return settleTyped(LOCAL_ORIGIN, "sessions-batch", callTyped(LOCAL_ORIGIN, "sessions-batch", req, budget));
}

/** 开终端那一形：后端渲好的那一行，monitor 开窗（本机带起始目录，远端经 `openTerminal`，带上那条会话）。开不了 ⇒ 那一句。 */
async function openWindow(origin: Origin, cmd: string, cwd: string | null, sid: string): Promise<string | null> {
  try {
    if (isLocalOrigin(origin)) await openLocalTerminal(cmd, cwd);
    else await openTerminal(origin, cmd, sid);
    return null;
  } catch (e) {
    return failSaid(copyText("tabBatch.why.windowFailed"), e);
  }
}

/** 起这一项：问那台；落在窗口里的开那一行（批量只开这一种）；本来就在跑的记跳过。 */
async function startOne(origin: Origin, sid: string, run: Launch): Promise<BatchOutcome> {
  const end = await askOpen(origin, { what: { kind: "resume", sid }, run, local: isLocalOrigin(origin) });
  if (end.kind !== "opened") {
    return { sid, outcome: "failed", why: end.said, ...(end.detail !== "" ? { detail: end.detail } : {}) };
  }
  const o = end.opened;
  if (o.landed === "running") return { sid, outcome: "skipped", why: copyText("tabBatch.why.running", { name: o.session ?? "" }) };
  if (BACKGROUND_OPENS.has(o.landed) && o.line !== null) {
    const failed = await openWindow(origin, o.line, o.cwd, o.sid ?? sid);
    if (failed !== null) return { sid, outcome: "failed", why: failed };
  }
  return { sid, outcome: "done", why: "" };
}

/** 起：`run` = `tmux`（在那台 tmux 里后台起，不开窗）· `direct`（各开一个终端）。同一台逐项挨个，各台并发。 */
export async function startMany(tabs: readonly Tab[], run: Launch): Promise<BatchOutcome[]> {
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
