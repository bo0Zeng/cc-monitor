/**
 * 起新会话框的三问（那台后端判，界面只读）：`session-new-facts`（框打开时：最近用过的目录 · 有没有 tmux · 能起哪几家 · 分叉源会话的三格）·
 * `session-new-dir`（目录失焦：在不在 · 那台会铸的终端名）· `session-new`（点［新建］：起）。形状见协议文档那一节，成品严格收。
 */
import { chan, ChanError } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson, refusalOf } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import { saidOfTransport } from "./control-said";
import { detailOf } from "./kit/detail";
import { exactKeys, isObj } from "./ipc/decode";
import type { ForkLaunch } from "./session-writes";
import { decodeForkLaunch } from "./session-writes";
import type { SessionNew } from "./generated/SessionNew";
import type { SessionNewField } from "./generated/SessionNewField";
import type { AccountAsk } from "./generated/AccountAsk";
import type { AccountUnavailable } from "./generated/AccountUnavailable";

/** 读两问：扫记录目录 ＋ 列一次 tmux；15 s 盖住远端握手。 */
const READ_BUDGET_MS = 15_000;
/** 起：铸名 ＋ 起 ccm（那台自己的期限 15 s）＋ 回程。到了 ⇒「启动无应答」，按结果未知处理。 */
export const NEW_BUDGET_MS = 25_000;

export interface NewFacts {
  recent: { cwd: string; lastMs: number }[];
  tmux: boolean;
  agents: string[];
  fork: { agent: string; launch: ForkLaunch; turn: number | null; startText: string | null } | null;
}

const strOrNull = (v: unknown): v is string | null => v === null || typeof v === "string";

function bad(op: string): never {
  throw new Error(`${op} reply shape mismatch`);
}

export function decodeFacts(v: unknown): NewFacts {
  const op = "session-new-facts";
  if (!isObj(v) || !exactKeys(v, ["recent", "tmux", "agents", "fork"])) bad(op);
  const o = v as Record<string, unknown>;
  if (!Array.isArray(o.recent) || typeof o.tmux !== "boolean" || !Array.isArray(o.agents)) bad(op);
  const recent = (o.recent as unknown[]).map((r) => {
    if (!isObj(r) || !exactKeys(r, ["cwd", "lastMs"]) || typeof r.cwd !== "string" || typeof r.lastMs !== "number") bad(op);
    return r as { cwd: string; lastMs: number };
  });
  if (!(o.agents as unknown[]).every((a) => typeof a === "string")) bad(op);
  let fork: NewFacts["fork"] = null;
  if (o.fork !== null) {
    const f = o.fork;
    if (!isObj(f) || !exactKeys(f, ["agent", "launch", "turn", "startText"]) || typeof f.agent !== "string") bad(op);
    const ff = f as Record<string, unknown>;
    if (!(ff.turn === null || typeof ff.turn === "number") || !strOrNull(ff.startText)) bad(op);
    fork = { agent: ff.agent as string, launch: decodeForkLaunch(ff.launch), turn: ff.turn as number | null, startText: ff.startText as string | null };
  }
  return { recent, tmux: o.tmux as boolean, agents: o.agents as string[], fork };
}

export interface DirFacts {
  exists: boolean;
  tmuxName: string | null;
}

export function decodeDir(v: unknown): DirFacts {
  if (!isObj(v) || !exactKeys(v, ["exists", "tmuxName"]) || typeof v.exists !== "boolean" || !strOrNull(v.tmuxName)) bad("session-new-dir");
  return v as unknown as DirFacts;
}

const OUTCOMES = ["started", "open"];

export function decodeNew(v: unknown): SessionNew {
  const keys = ["outcome", "session", "sid", "cmd", "account", "agent", "cwd"];
  if (!isObj(v) || !exactKeys(v, keys)) bad("session-new");
  const o = v as Record<string, unknown>;
  if (
    !OUTCOMES.includes(o.outcome as string) ||
    !strOrNull(o.session) ||
    !strOrNull(o.sid) ||
    !strOrNull(o.cmd) ||
    typeof o.agent !== "string" ||
    typeof o.cwd !== "string" ||
    !(o.account === null || (isObj(o.account) && typeof o.account.name === "string" && typeof o.account.configDir === "string"))
  ) {
    bad("session-new");
  }
  return o as unknown as SessionNew;
}

export async function askFacts(origin: Origin, fork: { sid: string; uuid: string } | null): Promise<NewFacts> {
  const body = jsonBody(fork ? { forkOf: fork.sid, at: fork.uuid } : {});
  const budget = budgetWithin(READ_BUDGET_MS);
  return decodeFacts(readJson(await chan.call(origin, "session-new-facts", body, budget)));
}

export async function askDir(origin: Origin, cwd: string, forkOf: string | null): Promise<DirFacts> {
  const body = jsonBody(forkOf ? { cwd, forkOf } : { cwd });
  const budget = budgetWithin(READ_BUDGET_MS);
  return decodeDir(readJson(await chan.call(origin, "session-new-dir", body, budget)));
}

/** 点［新建］交给那台的那一份。 */
export interface NewRequest {
  agent: string;
  cwd: string;
  account?: AccountAsk;
  place: "tmux" | "window";
  tmuxName?: string;
  command?: string;
  forkFrom?: { sid: string; uuid: string };
  models?: Record<string, string>;
  local: boolean;
  /** 这一趟的票（一个框一张）：期限到了再问一次带同一张 ⇒ 那台认出同一趟，起好了回原样那一份、不起第二个。 */
  ticket?: string;
  /** 轮换来源：缺 ＝ 跟随默认；`{rule}` ＝ 那台起之前先定 sid、按它写好来源（会话一报到就是那条规则）。 */
  rotation?: { rule: string };
}

/**
 * 起的结局：起了 · 某一格不行 / 整体不行（那台说的码与那一句）· 同一张票那一趟还在起 · 期限到（结果未知）· 够不着那台。
 * 不行的几形都带复制详情（`detail`，没有 ⇒ 空串）。
 */
export type NewResult =
  | { kind: "ok"; reply: SessionNew }
  | { kind: "refused"; code: string; said: string; field: SessionNewField | null; unavailable: AccountUnavailable | null; detail: string }
  | { kind: "pending"; said: string; detail: string }
  | { kind: "timeout"; detail: string }
  | { kind: "unreachable"; said: string; detail: string };

/** 拒绝体的 `data`（`{field, unavailable}`）；读不出 ⇒ 当整体不行。 */
function refusalData(d: unknown): { field: SessionNewField | null; unavailable: AccountUnavailable | null } {
  if (!isObj(d) || !exactKeys(d, ["field", "unavailable"])) return { field: null, unavailable: null };
  const f = d.field;
  const fields: SessionNewField[] = ["agent", "command", "cwd", "account", "place", "tmuxName"];
  const u = d.unavailable;
  return {
    field: typeof f === "string" && (fields as string[]).includes(f) ? (f as SessionNewField) : null,
    unavailable: isObj(u) && typeof u.requested === "string" ? (u as unknown as AccountUnavailable) : null,
  };
}

export async function askNew(origin: Origin, req: NewRequest): Promise<NewResult> {
  const body = jsonBody({ ...req });
  const budget = budgetWithin(NEW_BUDGET_MS);
  let reply: Uint8Array;
  try {
    reply = await chan.call(origin, "session-new", body, budget);
  } catch (e) {
    const detail = detailOf(e);
    if (!(e instanceof ChanError)) return { kind: "unreachable", said: String(e), detail };
    const err = e.error;
    if (err.layer === "peer" && err.why === "refused") {
      const r = refusalOf(err.body);
      if (r?.code === "launch_pending") return { kind: "pending", said: r.message, detail };
      if (r) return { kind: "refused", code: r.code, said: r.message, ...refusalData(r.data), detail };
    }
    // 发出去了、没等到回话（期限到 · 半路断了）⇒ 结果未知：同一张票再问一次（那台认得出是不是同一趟），不说失败。
    if (err.layer === "hop" && err.reach !== "NotSent") return { kind: "timeout", detail };
    return { kind: "unreachable", said: saidOfTransport(origin, err), detail };
  }
  return { kind: "ok", reply: decodeNew(readJson(reply)) };
}
