/**
 * **tab 栏停 / 起：交给那几台后端**（`sessions-stop` / `sessions-start`）—— 批量菜单与单个菜单同一条路，单个就是一个 sid 的一批。
 *
 * 一批里同一台机器的那几个**一次**调用发过去，那台逐个答（能不能做、怎么做都是那台判 —— 每一个同单个菜单那一项）；
 * 不同机器并发各发各的，一台失败不挡别台（那一台的几个记成失败、说清为什么）。
 * 本文件只做调用方那一侧：按台分组 · 组请求（账号跟随与单个那条同一份解析：`launch-account.ts::resolveLaunchAccount`
 * / `localFollowPlan`，每台只取一次账号清单）· 按形状收 · 把每一个的结局说成一句人话 · 开终端那一形逐个开窗。
 */
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody } from "./ipc/chan-caller";
import { exactKeys, isObj, machineName, saidOfControl, settle, unavailableSaid, unreadable, type Refusals } from "./control-said";
import { isLocalOrigin, type Origin } from "./ipc/origin";
import { copyText } from "./copy-table";
import { killRefusals } from "./tmux-control";
import type { Tab } from "./tab-model";
import { fetchAccounts } from "./account-reads";
import { lastAccounts } from "./history-reads";
import { getModelForAccount } from "./account-prefs";
import {
  followRecordName,
  localFollowPlan,
  primeLocalLaunchAccounts,
  recordLastAccount,
  recordLocalLaunchAccount,
  resolveLaunchAccount,
} from "./launch-account";
import { getBehavior } from "./behavior";
import { resolveResumeCommand } from "./remote-config";
import { AGENT_PROFILE } from "./agent-profile";
import { openTerminal } from "./terminal-open";
import { commands } from "./ipc/commands";

/** 一个 tab 的结局（`why` 是给人看的那一句；做成了 ⇒ 空串）。 */
export interface BatchOutcome {
  sid: string;
  outcome: "done" | "skipped" | "failed";
  why: string;
}

/** 后端答的一个（`sessions-*` 的 `results[i]`，形状见协议文档那一节）。`why` 是那台的原因码。 */
export interface Reply {
  sid: string;
  outcome: "done" | "skipped" | "failed";
  why: string | null;
  detail: string;
  session: string | null;
  /** 停那一条：顺手从 cc-bus 名册注销的结局（形状同 `kill` 那一格）；别的 ⇒ `null`。 */
  bus: unknown;
  cmd: string | null;
}

const KEYS = ["sid", "outcome", "why", "detail", "session", "bus", "cmd"] as const;

/** 应答 ⇒ 逐个结局；形状不对（多一格缺一格 · 个数 / 次序对不上）⇒ 抛（两边版本对不上，不猜）。 */
export function decodeBatch(origin: Origin, op: string, sids: readonly string[], v: unknown): Reply[] {
  const bad = (): never => {
    throw unreadable(origin, op, "is not exactly {results: [...]} in request order");
  };
  if (!isObj(v) || !exactKeys(v, ["results"]) || !Array.isArray(v.results) || v.results.length !== sids.length) bad();
  return (v as { results: unknown[] }).results.map((r, i) => {
    if (
      !isObj(r) ||
      !exactKeys(r, KEYS) ||
      r.sid !== sids[i] ||
      (r.outcome !== "done" && r.outcome !== "skipped" && r.outcome !== "failed") ||
      !(r.why === null || typeof r.why === "string") ||
      typeof r.detail !== "string" ||
      !(r.session === null || typeof r.session === "string") ||
      !(r.cmd === null || typeof r.cmd === "string")
    ) {
      return bad();
    }
    return r as unknown as Reply;
  });
}

/** 一台的整体失败（通道不在 · 后端太旧 · 拒了整批）⇒ 那一台的每一个都记成失败、带同一句。 */
function machineFailed(sids: readonly string[], e: unknown): BatchOutcome[] {
  const why = saidOfControl(e);
  return sids.map((sid) => ({ sid, outcome: "failed", why }));
}

/** 整批被那台拒了（命令级的码）怎么说。 */
function batchRefusals(origin: Origin): Refusals {
  return {
    byCode: (_code, detail) => copyText("tabBatch.why.batchRefused", { machine: machineName(origin), detail }),
    noReason: () => copyText("tabBatch.why.batchRefused", { machine: machineName(origin), detail: "" }),
  };
}

/** 后端答的跳过 / 失败原因 ⇒ 人话。停的失败照单个那条的说法（`tmux-control.ts::killRefusals`）。 */
export function sayReply(origin: Origin, op: "stop" | "start", r: Reply): string {
  const machine = machineName(origin);
  const target = r.session ?? "";
  switch (r.why) {
    case null:
      return "";
    case "no_tmux":
      return copyText("tabBatch.why.noTmux", { machine });
    case "not_in_tmux":
      return copyText("tabBatch.why.notInTmux", { machine });
    case "ambiguous":
      return copyText("tabBatch.why.ambiguous", { machine, names: r.detail });
    case "running":
      return copyText("tabBatch.why.running", { name: target });
    case "record_gone":
      return copyText("tabBatch.why.recordGone", { machine, root: r.detail });
    case "name_taken":
      return copyText("tabBatch.why.nameTaken", { machine, name: target });
    default:
      return op === "stop"
        ? killRefusals(target).byCode(r.why, r.detail)
        : copyText("tabBatch.why.startFailed", { machine, detail: r.detail });
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
  const body = jsonBody({ sids });
  const v = await settle(origin, "sessions-stop", chan.call(origin, "sessions-stop", body, budget), batchRefusals(origin));
  return decodeBatch(origin, "sessions-stop", sids, v);
}

/** 起：问那台一次（`items` 那几个），逐个答。问不到 / 形状不对 ⇒ 抛 `ControlError`。 */
export async function callStart(origin: Origin, mode: "tmux" | "window", items: readonly StartItem[]): Promise<Reply[]> {
  const sids = items.map((i) => i.sid);
  const budget = budgetWithin(BATCH_BASE_MS + BATCH_EACH_MS * sids.length);
  const body = jsonBody({ mode, local: isLocalOrigin(origin), items });
  const v = await settle(origin, "sessions-start", chan.call(origin, "sessions-start", body, budget), batchRefusals(origin));
  return decodeBatch(origin, "sessions-start", sids, v);
}

/** 停：每台一次 `sessions-stop`，各台并发。 */
export async function stopMany(tabs: readonly Tab[]): Promise<BatchOutcome[]> {
  const parts = await Promise.all(
    Array.from(groupByOrigin(tabs), async ([origin, list]): Promise<BatchOutcome[]> => {
      const sids = list.map((t) => t.sessionId);
      const no = offered(origin, "sessions-stop", sids);
      if (no) return no;
      try {
        return (await callStop(origin, sids)).map((r) => ({ sid: r.sid, outcome: r.outcome, why: sayReply(origin, "stop", r) }));
      } catch (e) {
        return machineFailed(sids, e);
      }
    }),
  );
  return parts.flat();
}

/** 起会话要交给那台的一个（`sessions-start` 的 `items[i]`）。 */
export interface StartItem {
  sid: string;
  cwd: string;
  account: { kind: "inherit" } | { kind: "base" } | { kind: "named"; name: string | null; configDir: string };
  model: string | null;
  launcher: string;
  defaultLauncher: string;
}

/** 一台要起的那几个：能组出请求的 ＋ 账号那一关就过不去的（跳过）＋ 起成了之后要记的「上次用的号」。 */
export interface StartPlan {
  items: StartItem[];
  skipped: BatchOutcome[];
  record: Map<string, () => void>;
}

/** 账号跟随与单个那条 Resume 同一份解析（远端：每台一次账号清单 ＋ 一次「上次用的号」）。 */
export async function planStarts(origin: Origin, list: readonly Tab[]): Promise<StartPlan> {
  const behavior = await getBehavior();
  const defaultLauncher = AGENT_PROFILE.defaultLauncher;
  const plan: StartPlan = { items: [], skipped: [], record: new Map() };
  if (isLocalOrigin(origin)) {
    primeLocalLaunchAccounts();
    const launcher = behavior.resumeCommandLocal.trim() || defaultLauncher;
    for (const t of list) {
      const p = localFollowPlan(t.sessionId);
      if (p.kind === "pinGone") {
        plan.skipped.push({ sid: t.sessionId, outcome: "skipped", why: copyText("tabBatch.why.accountGone", { name: p.pin }) });
        continue;
      }
      if (p.kind === "named") plan.record.set(t.sessionId, () => recordLocalLaunchAccount(t.sessionId, p.name));
      plan.items.push({
        sid: t.sessionId,
        cwd: t.projectDir ?? "",
        account: p.kind === "named" ? { kind: "named", name: p.name, configDir: p.configDir } : { kind: "inherit" },
        model: null,
        launcher,
        defaultLauncher,
      });
    }
    return plan;
  }
  const state = await fetchAccounts(origin).catch(() => undefined);
  const pins = await lastAccounts().catch(() => ({}) as Record<string, string>);
  const launcher = (await resolveResumeCommand(origin, behavior.resumeCommandRemote)).trim() || defaultLauncher;
  for (const t of list) {
    const prior = pins[t.sessionId] ?? null;
    const r = resolveLaunchAccount(state, null, { lastAccount: prior });
    if (r.kind === "unavailable") {
      const why =
        state?.available === true
          ? copyText("tabBatch.why.accountGone", { name: r.requestedName ?? "" })
          : copyText("tabBatch.why.accountList", { machine: machineName(origin), name: r.requestedName ?? "" });
      plan.skipped.push({ sid: t.sessionId, outcome: "skipped", why });
      continue;
    }
    const name = r.kind === "account" ? followRecordName(prior, r) : null;
    if (name) plan.record.set(t.sessionId, () => void recordLastAccount(t.sessionId, name));
    plan.items.push({
      sid: t.sessionId,
      cwd: t.projectDir ?? "",
      account: r.kind === "account" ? { kind: "named", name: r.name, configDir: r.configDir } : { kind: "base" },
      model: r.kind === "account" ? ((await getModelForAccount(origin, r.name)) ?? null) : null,
      launcher,
      defaultLauncher,
    });
  }
  return plan;
}

/** 开终端那一形：后端渲好的那一行，monitor 开窗（本机同 `launch-local` 那条的开法，远端同 `openTerminal`）。 */
async function openWindow(origin: Origin, cmd: string, cwd: string): Promise<string | null> {
  try {
    if (isLocalOrigin(origin)) await commands.open_local_terminal({ cmd, cwd });
    else await openTerminal(origin, cmd);
    return null;
  } catch (e) {
    return copyText("tabBatch.why.windowFailed", { detail: String(e) });
  }
}

/** 起：`mode` = `tmux`（在那台 tmux 里后台起，不开窗）· `window`（各开一个终端）。每台一次 `sessions-start`，各台并发。 */
export async function startMany(tabs: readonly Tab[], mode: "tmux" | "window"): Promise<BatchOutcome[]> {
  const parts = await Promise.all(
    Array.from(groupByOrigin(tabs), async ([origin, list]): Promise<BatchOutcome[]> => {
      const all = list.map((t) => t.sessionId);
      const no = offered(origin, "sessions-start", all);
      if (no) return no;
      let plan: StartPlan;
      try {
        plan = await planStarts(origin, list);
      } catch (e) {
        return machineFailed(all, e);
      }
      if (plan.items.length === 0) return plan.skipped;
      const sids = plan.items.map((i) => i.sid);
      let replies: Reply[];
      try {
        replies = await callStart(origin, mode, plan.items);
      } catch (e) {
        return [...plan.skipped, ...machineFailed(sids, e)];
      }
      const out: BatchOutcome[] = [...plan.skipped];
      for (const [i, r] of replies.entries()) {
        let o: BatchOutcome = { sid: r.sid, outcome: r.outcome, why: sayReply(origin, "start", r) };
        if (r.outcome === "done" && mode === "window" && r.cmd !== null) {
          const failed = await openWindow(origin, r.cmd, plan.items[i].cwd);
          if (failed !== null) o = { sid: r.sid, outcome: "failed", why: failed };
        }
        if (o.outcome === "done") plan.record.get(r.sid)?.();
        out.push(o);
      }
      return out;
    }),
  );
  return parts.flat();
}
