/**
 * **tab 栏停 / 起：交给后端**。
 *
 * - 批量停 · 起（`sessions-batch`）：选中的全部交本机后端一条，按台扇出 · 跳不跳过 · 合那一句都在后端，回包照摆；
 *   单个结束经 [`callStop`]（`sessions-stop`，那台逐个答）。
 * - 起那一批回包里落在窗口里的那几行（`windows`）由这里逐个开窗：开窗是这台电脑上的事；开不出来那一项另出一条，不改汇总。
 */
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import { settleTyped } from "./control-said";
import { batchRefused } from "./sessions-where";
import { isLocalOrigin, LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import type { SessionsBatchReply } from "./generated/SessionsBatchReply";
import type { SessionsOp } from "./generated/SessionsOp";
import type { SessionItem } from "./generated/SessionItem";
import { copyText } from "./copy-table";
import type { Tab } from "./tab-model";
import type { Launch } from "./generated/Launch";
import type { SessionOutcome as Outcome } from "./generated/SessionOutcome";
import { openLocalTerminal, openTerminal } from "./terminal-open";
import { failToast } from "./kit/toast";

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

/** 停的期限：单个那条 10 秒；一批按个数放宽（那台逐个做），每个多给 6 秒。 */
const BATCH_BASE_MS = 10_000;
const BATCH_EACH_MS = 6_000;

/** 停：问那台一次（`sids` 那几个），逐个答。问不到 / 形状不对 ⇒ 抛 `ControlError`。 */
export async function callStop(origin: Origin, sids: readonly string[]): Promise<Reply[]> {
  const budget = budgetWithin(BATCH_BASE_MS + BATCH_EACH_MS * sids.length);
  const v = await settleTyped(origin, "sessions-stop", callTyped(origin, "sessions-stop", { sids: [...sids] }, budget), batchRefused(origin));
  return v.results;
}

/**
 * 一批会话的那一样（停 · 起 · …）交本机后端一条 `sessions-batch`（`items` 照交进来的先后）；回包是合好的那一份。
 * 期限按个数放宽：那台逐项挨个做，每项多给 `each`。问不到本机后端 ⇒ 抛。
 */
export async function askSessions(op: SessionsOp, items: readonly SessionItem[], each: number = BATCH_EACH_MS): Promise<SessionsBatchReply> {
  const budget = budgetWithin(BATCH_BASE_MS + each * items.length);
  return settleTyped(LOCAL_ORIGIN, "sessions-batch", callTyped(LOCAL_ORIGIN, "sessions-batch", { op, items: [...items] }, budget));
}

/** 一个 tab ⇒ 一批里的那一项。 */
export const itemOf = (t: Pick<Tab, "origin" | "sessionId">): SessionItem => ({ machine: t.origin, sid: t.sessionId });

/** 批量停：选中的全部（照选中的先后）。 */
export function stopMany(tabs: readonly Tab[]): Promise<SessionsBatchReply> {
  return askSessions("stop", tabs.map(itemOf));
}

/** 起的那一项至多等那台起一次（铸名 ＋ 起 ccm ＋ 回程）。 */
const START_EACH_MS = 25_000;

/**
 * 批量起：`run` = `tmux`（在那台 tmux 里后台起，不开窗）· `direct`（各开一个终端）。选中的全部（照选中的先后）；
 * 回包里落在窗口里的那几行逐个开窗，再把回包交回（那一句照它说）。
 */
export async function startMany(tabs: readonly Tab[], run: Launch): Promise<SessionsBatchReply> {
  const reply = await askSessions({ start: { run } }, tabs.map(itemOf), START_EACH_MS);
  for (const w of reply.windows) await openWindow(w.machine, w.line, w.cwd, w.sid);
  return reply;
}

/** 开终端那一形：后端渲好的那一行，monitor 开窗（本机带起始目录，远端经 `openTerminal`，带上那条会话）。开不了 ⇒ 另出那一句。 */
async function openWindow(origin: Origin, cmd: string, cwd: string | null, sid: string): Promise<void> {
  try {
    if (isLocalOrigin(origin)) await openLocalTerminal(cmd, cwd);
    else await openTerminal(origin, cmd, sid);
  } catch (e) {
    failToast(copyText("tabBatch.why.windowFailed"), e);
  }
}
