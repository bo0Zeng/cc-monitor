/**
 * 额度与轮换那几条命令的界面口：`quota-read` · `rotation-read` · `rotation-session-read/-set` · `rotation-switch`
 * （会话所在那台；本机远端同一条 `chan.call(origin, …)`）。判定全在后端：这里只按形状收、交给画的那几处。
 */
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import type { QuotaRead } from "./quota-lines";
import type { Rotation } from "./generated/Rotation";
import type { SessionRotationState } from "./generated/SessionRotationState";
import type { SwitchOutcome } from "./generated/SwitchOutcome";

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

/** `rotation-read` / `rotation-set` 的应答。 */
export interface RotationRead {
  state: "present" | "absent" | "unreadable";
  rotation: Rotation;
  followers: number;
}

export function decodeRotationRead(v: unknown): RotationRead {
  const o = obj(v, "reply");
  const r = obj(o.rotation, "rotation");
  arr(r.order, "rotation.order");
  arr(r.enabled, "rotation.enabled");
  if (r.atLimit !== "continue" && r.atLimit !== "stop") bad("rotation.atLimit");
  if (typeof o.followers !== "number") bad("followers");
  return { state: o.state as RotationRead["state"], rotation: r as unknown as Rotation, followers: o.followers };
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

// 每一问各写一处 `chan.call`，操作名是字面量（通信层判据按字面量认是哪条帧命令）。

export async function readQuota(origin: Origin): Promise<QuotaRead> {
  const body = jsonBody({});
  const budget = budgetWithin(READ_BUDGET_MS);
  return decodeQuotaRead(readJson(await chan.call(origin, "quota-read", body, budget)));
}

export async function readRotation(origin: Origin): Promise<RotationRead> {
  const body = jsonBody({});
  const budget = budgetWithin(READ_BUDGET_MS);
  return decodeRotationRead(readJson(await chan.call(origin, "rotation-read", body, budget)));
}

export async function readSessionRotation(origin: Origin, sids: string[]): Promise<SessionRotationRead> {
  const body = jsonBody({ sids });
  const budget = budgetWithin(READ_BUDGET_MS);
  return decodeSessionRotationRead(readJson(await chan.call(origin, "rotation-session-read", body, budget)));
}

/** `"follow"` 跟随默认 · `"custom"` 恢复上一份自己的 · `{custom}` 整份写。 */
export type SessionRotationWrite = "follow" | "custom" | { custom: Rotation };

export async function writeSessionRotation(origin: Origin, sids: string[], rotation: SessionRotationWrite): Promise<Record<string, SwitchOutcome>> {
  const body = jsonBody({ sids, rotation });
  const budget = budgetWithin(READ_BUDGET_MS);
  return decodeOutcomes(readJson(await chan.call(origin, "rotation-session-set", body, budget)));
}

/** 现在就换：热切换 `sessions` 是会话 id；重启切换每项是 `session-restart` 的入参（不带 `account`），`ms` 是这一趟的总期限（要等压缩 · 等报出）。 */
async function switchNow(origin: Origin, sessions: unknown[], target: string, mode: "hot" | "restart", ms: number): Promise<Record<string, SwitchOutcome>> {
  const body = jsonBody({ sessions, target, mode });
  const budget = budgetWithin(ms);
  return decodeOutcomes(readJson(await chan.call(origin, "rotation-switch", body, budget)));
}

export function switchHot(origin: Origin, sids: string[], target: string): Promise<Record<string, SwitchOutcome>> {
  return switchNow(origin, sids, target, "hot", READ_BUDGET_MS);
}

export function switchRestart(origin: Origin, sessions: Record<string, unknown>[], target: string, ms: number): Promise<Record<string, SwitchOutcome>> {
  return switchNow(origin, sessions, target, "restart", ms);
}
