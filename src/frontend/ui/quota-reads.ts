/**
 * 额度与轮换那几条命令的界面口：`quota-read` · `rotation-rules-read` · `rotation-session-read/-set` · `rotation-switch`
 * （会话所在那台；本机远端同一条 `chan.call(origin, …)`）。判定全在后端：这里只按形状收、交给画的那几处。
 */
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import type { QuotaRead } from "./quota-lines";
import type { Rotation } from "./generated/Rotation";
import type { SessionRotationState } from "./generated/SessionRotationState";
import type { RestartOutcome } from "./generated/RestartOutcome";
import type { SwitchOutcome } from "./generated/SwitchOutcome";
import { exactKeys } from "./ipc/decode";

/** 读一份额度账 / 轮换（读盘 ＋ 回程）。 */
const READ_BUDGET_MS = 15_000;

function bad(what: string): never {
  throw new Error(`quota reply shape mismatch: ${what}`); // 程序员错误，刻意英文
}

function obj(v: unknown, what: string): Record<string, unknown> {
  return v !== null && typeof v === "object" && !Array.isArray(v) ? (v as Record<string, unknown>) : bad(`${what} is not an object`);
}

function arr(v: unknown, what: string): unknown[] {
  return Array.isArray(v) ? v : bad(`${what} is not an array`);
}

const STATES = new Set(["present", "absent", "unreadable"]);

/** `quota-read` 的回包（形状不对 ⇒ 抛）。 */
export function decodeQuotaRead(v: unknown): QuotaRead {
  const o = obj(v, "reply");
  if (!STATES.has(o.state as string)) bad(`state ${JSON.stringify(o.state)}`);
  if (typeof o.now !== "number") bad("now");
  for (const [i, a] of arr(o.accounts, "accounts").entries()) {
    const x = obj(a, `accounts[${i}]`);
    if (typeof x.account !== "string" || typeof x.seenAt !== "number" || typeof x.state !== "string" || !Array.isArray(x.slots)) bad(`accounts[${i}]`);
  }
  for (const [i, u] of arr(o.unseen, "unseen").entries()) {
    const x = obj(u, `unseen[${i}]`);
    if (typeof x.account !== "string" || typeof x.kind !== "string") bad(`unseen[${i}]`);
  }
  arr(o.usableNow, "usableNow");
  return o as unknown as QuotaRead;
}

/** 规则表里的一条（后端算好的几格一并带来：谁在用 · 摘要 · 说明 · 这台没有的号）。 */
export interface RuleRow {
  id: string;
  name: string;
  rotation: Rotation;
  rev: number;
  updatedAt: number;
  isDefault: boolean;
  users: { live: number; ended: number; follow: number; sids: string[] };
  summary: string;
  explain: string;
  missing: string[];
  atLimitApplies: boolean;
}

/** `rotation-rules-read` 的应答。 */
export interface RulesRead {
  state: "present" | "absent" | "unreadable";
  defaultRule: string;
  rules: RuleRow[];
}

function decodeRotation(v: unknown, what: string): Rotation {
  const r = obj(v, what);
  arr(r.order, `${what}.order`);
  arr(r.enabled, `${what}.enabled`);
  if (r.atLimit !== "continue" && r.atLimit !== "stop") bad(`${what}.atLimit`);
  if (typeof r.wait !== "number") bad(`${what}.wait`);
  return r as unknown as Rotation;
}

export function decodeRuleRow(v: unknown, what: string): RuleRow {
  const x = obj(v, what);
  if (typeof x.id !== "string" || typeof x.name !== "string" || typeof x.rev !== "number" || typeof x.isDefault !== "boolean") bad(what);
  if (typeof x.summary !== "string" || typeof x.explain !== "string") bad(`${what}.summary`);
  decodeRotation(x.rotation, `${what}.rotation`);
  const u = obj(x.users, `${what}.users`);
  if (typeof u.live !== "number" || typeof u.ended !== "number") bad(`${what}.users`);
  arr(x.missing, `${what}.missing`);
  return x as unknown as RuleRow;
}

export function decodeRulesRead(v: unknown): RulesRead {
  const o = obj(v, "reply");
  if (!STATES.has(o.state as string)) bad(`state ${JSON.stringify(o.state)}`);
  if (typeof o.defaultRule !== "string") bad("defaultRule");
  const rules = arr(o.rules, "rules").map((r, i) => decodeRuleRow(r, `rules[${i}]`));
  return { state: o.state as RulesRead["state"], defaultRule: o.defaultRule, rules };
}

/** 规则表里默认那一条（表里总有它；对不上 ⇒ `null`）。 */
export function defaultRuleOf(r: RulesRead | null | undefined): RuleRow | null {
  return r?.rules.find((x) => x.id === r.defaultRule) ?? null;
}

/** `rotation-session-read`：每个 sid 一份。 */
export interface SessionRotationRead {
  now: number;
  sessions: Record<string, SessionRotationState>;
}

export function decodeSessionRotationRead(v: unknown): SessionRotationRead {
  const o = obj(v, "reply");
  if (typeof o.now !== "number") bad("now");
  const ss = obj(o.sessions, "sessions");
  for (const [sid, s] of Object.entries(ss)) {
    const x = obj(s, `sessions.${sid}`);
    if (x.state === "absent") {
      if (typeof x.inPlace !== "string") bad(`sessions.${sid}.inPlace`);
      continue;
    }
    if (x.state !== "present") bad(`sessions.${sid}.state`);
    obj(x.account, `sessions.${sid}.account`);
    obj(x.quota, `sessions.${sid}.quota`);
  }
  return { now: o.now, sessions: ss as Record<string, SessionRotationState> };
}

/** `rotation-switch` / `rotation-session-set` 的应答：每个 sid 一个结局。 */
export function decodeOutcomes(v: unknown): Record<string, SwitchOutcome> {
  const ss = obj(obj(v, "reply").sessions, "sessions");
  for (const [sid, s] of Object.entries(ss)) {
    const x = obj(s, `sessions.${sid}`);
    if (x.state !== "done" && typeof x.code !== "string") bad(`sessions.${sid}.code`);
  }
  return ss as Record<string, SwitchOutcome>;
}

/** 重启换那一支的应答：每个 sid `{state:"done", terminal}` 或 `{state:"failed", code, old}`，多一格 / 缺一格 / 类型不对 ⇒ 抛。 */
export function decodeRestartOutcomes(v: unknown): Record<string, RestartOutcome> {
  const ss = obj(obj(v, "reply").sessions, "sessions");
  for (const [sid, s] of Object.entries(ss)) {
    const x = obj(s, `sessions.${sid}`);
    const ok =
      x.state === "done"
        ? exactKeys(x, ["state", "terminal"]) && typeof x.terminal === "string"
        : x.state === "failed" && exactKeys(x, ["state", "code", "old"]) && typeof x.code === "string" && (x.old === "kept" || x.old === "ended");
    if (!ok) bad(`sessions.${sid}`);
  }
  return ss as Record<string, RestartOutcome>;
}

// 每一问各写一处 `chan.call`，操作名是字面量（通信层判据按字面量认是哪条帧命令）。

export async function readQuota(origin: Origin): Promise<QuotaRead> {
  const body = jsonBody({});
  const budget = budgetWithin(READ_BUDGET_MS);
  return decodeQuotaRead(readJson(await chan.call(origin, "quota-read", body, budget)));
}

export async function readRules(origin: Origin): Promise<RulesRead> {
  const body = jsonBody({});
  const budget = budgetWithin(READ_BUDGET_MS);
  return decodeRulesRead(readJson(await chan.call(origin, "rotation-rules-read", body, budget)));
}

export async function readSessionRotation(origin: Origin, sids: string[]): Promise<SessionRotationRead> {
  const body = jsonBody({ sids });
  const budget = budgetWithin(READ_BUDGET_MS);
  return decodeSessionRotationRead(readJson(await chan.call(origin, "rotation-session-read", body, budget)));
}

/** `"follow"` 跟随默认 · `{rule}` 用某条规则 · `"custom"` 恢复上一份自己的 · `"detach"` 照此刻生效的那份拷成本会话 · `{custom}` 整份写。 */
export type SessionRotationWrite = "follow" | "custom" | "detach" | { rule: string } | { custom: Rotation };

export async function writeSessionRotation(origin: Origin, sids: string[], rotation: SessionRotationWrite): Promise<Record<string, SwitchOutcome>> {
  const body = jsonBody({ sids, rotation });
  const budget = budgetWithin(READ_BUDGET_MS);
  return decodeOutcomes(readJson(await chan.call(origin, "rotation-session-set", body, budget)));
}

/** 现在就换：热切换 `sessions` 是会话 id；重启切换每项是 `session-restart` 的入参（不带 `account`），`ms` 是这一趟的总期限（要等压缩 · 等报出）。 */
async function switchNow(origin: Origin, sessions: unknown[], target: string, mode: "hot" | "restart", ms: number): Promise<unknown> {
  const body = jsonBody({ sessions, target, mode });
  const budget = budgetWithin(ms);
  return readJson(await chan.call(origin, "rotation-switch", body, budget));
}

export async function switchHot(origin: Origin, sids: string[], target: string): Promise<Record<string, SwitchOutcome>> {
  return decodeOutcomes(await switchNow(origin, sids, target, "hot", READ_BUDGET_MS));
}

export async function switchRestart(origin: Origin, sessions: Record<string, unknown>[], target: string, ms: number): Promise<Record<string, RestartOutcome>> {
  return decodeRestartOutcomes(await switchNow(origin, sessions, target, "restart", ms));
}
