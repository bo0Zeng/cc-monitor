/**
 * 轮换（第三版）那几条命令的界面口：配置表 · 策略表 · 会话打开的副本 · 手动调整 · 推演 · 试跑。
 * 会话所在那台；本机远端同一条 `chan.call(origin, …)`。判定全在后端（策略在后端的沙箱里跑）：这里只按形状收、交给画的那几处。
 */
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import { takeNames } from "./acct-words";
import type { ConfigBody } from "./generated/ConfigBody";
import type { ConfigDiff } from "./generated/ConfigDiff";
import type { ConfigItem } from "./generated/ConfigItem";
import type { CellError } from "./generated/CellError";
import type { PlanReply } from "./generated/PlanReply";
import type { SessionRotationState } from "./generated/SessionRotationState";
import type { StrategiesRead } from "./generated/StrategiesRead";
import type { SwitchOutcome } from "./generated/SwitchOutcome";
import type { StrategyError } from "./generated/StrategyError";
import type { TryReply } from "./generated/TryReply";

/** 读盘 ＋ 回程（写也是这一档：都是阻塞档的短命令）。 */
const BUDGET_MS = 15_000;

function bad(what: string): never {
  throw new Error(`rotation reply shape mismatch: ${what}`); // 程序员错误，刻意英文
}

function obj(v: unknown, what: string): Record<string, unknown> {
  return v !== null && typeof v === "object" && !Array.isArray(v) ? (v as Record<string, unknown>) : bad(`${what} is not an object`);
}

function arr(v: unknown, what: string): unknown[] {
  return Array.isArray(v) ? v : bad(`${what} is not an array`);
}

function str(v: unknown, what: string): string {
  return typeof v === "string" ? v : bad(what);
}

function num(v: unknown, what: string): number {
  return typeof v === "number" ? v : bad(what);
}

const FILE_STATES = new Set(["present", "absent", "unreadable"]);

/** `rotation-configs-read` 的应答（`reason` · `detail` 只在读不懂时有）。 */
export interface ConfigsShown {
  state: "present" | "absent" | "unreadable";
  reason: string | null;
  detail: string | null;
  configs: ConfigItem[];
  defaultConfig: string;
}

function decodeConfigItem(v: unknown, what: string): ConfigItem {
  const x = obj(v, what);
  str(x.id, `${what}.id`);
  str(x.name, `${what}.name`);
  str(x.strategy, `${what}.strategy`);
  str(x.text, `${what}.text`);
  str(x.strategyName, `${what}.strategyName`);
  num(x.rev, `${what}.rev`);
  if (typeof x.isDefault !== "boolean") bad(`${what}.isDefault`);
  arr(x.inUse, `${what}.inUse`);
  arr(x.accounts, `${what}.accounts`);
  obj(x.params, `${what}.params`);
  obj(x.acctInit, `${what}.acctInit`);
  return x as unknown as ConfigItem;
}

export function decodeConfigsRead(v: unknown): ConfigsShown {
  const o = obj(v, "reply");
  if (!FILE_STATES.has(o.state as string)) bad(`state ${JSON.stringify(o.state)}`);
  const configs = arr(o.configs, "configs").map((c, i) => decodeConfigItem(c, `configs[${i}]`));
  const s = (x: unknown): string | null => (typeof x === "string" && x !== "" ? x : null);
  takeNames(o.names);
  return {
    state: o.state as ConfigsShown["state"],
    reason: s(o.reason),
    detail: s(o.detail),
    configs,
    defaultConfig: str(o.defaultConfig, "defaultConfig"),
  };
}

/** `rotation-session-read`：每个 sid 一份。 */
export interface SessionRotationRead {
  now: number;
  sessions: Record<string, SessionRotationState>;
}

export function decodeSessionRotationRead(v: unknown): SessionRotationRead {
  const o = obj(v, "reply");
  const now = num(o.now, "now");
  const ss = obj(o.sessions, "sessions");
  for (const [sid, s] of Object.entries(ss)) {
    const x = obj(s, `sessions.${sid}`);
    if (x.state === "absent") {
      str(x.inPlace, `sessions.${sid}.inPlace`);
      continue;
    }
    if (x.state !== "present") bad(`sessions.${sid}.state`);
    str(x.current, `sessions.${sid}.current`);
    str(x.start, `sessions.${sid}.start`);
    obj(x.config, `sessions.${sid}.config`);
    obj(x.strategy, `sessions.${sid}.strategy`);
    obj(x.values, `sessions.${sid}.values`);
    arr(x.rows, `sessions.${sid}.rows`);
    arr(x.history, `sessions.${sid}.history`);
    obj(x.quota, `sessions.${sid}.quota`);
  }
  takeNames(o.names);
  return { now, sessions: ss as Record<string, SessionRotationState> };
}

export function decodeStrategiesRead(v: unknown): StrategiesRead {
  const o = obj(v, "reply");
  str(obj(o.lib, "lib").source, "lib.source");
  for (const [i, s] of arr(o.strategies, "strategies").entries()) {
    const x = obj(s, `strategies[${i}]`);
    str(x.id, `strategies[${i}].id`);
    str(x.name, `strategies[${i}].name`);
    arr(x.params, `strategies[${i}].params`);
    arr(x.acctParams, `strategies[${i}].acctParams`);
  }
  return o as unknown as StrategiesRead;
}

export function decodePlan(v: unknown): PlanReply {
  const o = obj(v, "reply");
  num(o.now, "now");
  num(o.from, "from");
  num(o.until, "until");
  arr(o.plan, "plan");
  arr(o.rows, "rows");
  arr(o.grid, "grid");
  takeNames(o.names);
  return o as unknown as PlanReply;
}

// 每一问各写一处 `chan.call`，操作名是字面量（通信层判据按字面量认是哪条帧命令）。

export async function readConfigs(origin: Origin): Promise<ConfigsShown> {
  const body = jsonBody({});
  const budget = budgetWithin(BUDGET_MS);
  return decodeConfigsRead(readJson(await chan.call(origin, "rotation-configs-read", body, budget)));
}

export async function readStrategies(origin: Origin): Promise<StrategiesRead> {
  const body = jsonBody({});
  const budget = budgetWithin(BUDGET_MS);
  return decodeStrategiesRead(readJson(await chan.call(origin, "rotation-strategies-read", body, budget)));
}

export async function readSessionRotation(origin: Origin, sids: string[]): Promise<SessionRotationRead> {
  const body = jsonBody({ sids });
  const budget = budgetWithin(BUDGET_MS);
  return decodeSessionRotationRead(readJson(await chan.call(origin, "rotation-session-read", body, budget)));
}

/** 推演问哪一份：一个会话此刻那一份 · 这台的一份配置 · 一份草稿（带 `sid` ＝ 那个会话的草稿）；视窗 12h / 24h。 */
export type PlanAsk = ({ sid: string } | { config: string } | { draft: ConfigBody; sid?: string }) & { view: "12h" | "24h" };

export async function readPlan(origin: Origin, ask: PlanAsk): Promise<PlanReply> {
  const body = jsonBody(ask);
  const budget = budgetWithin(BUDGET_MS);
  return decodePlan(readJson(await chan.call(origin, "rotation-plan", body, budget)));
}

/** 试跑问哪一份（不写盘不换号）：同推演；`id` ＝ 换一份已加载的策略跑。 */
export type TryAsk = ({ sid: string } | { config: string } | { draft: ConfigBody }) & { id?: string };

export async function tryStrategy(origin: Origin, ask: TryAsk): Promise<TryReply> {
  const body = jsonBody(ask);
  const budget = budgetWithin(BUDGET_MS);
  const o = obj(readJson(await chan.call(origin, "rotation-strategy-try", body, budget)), "reply");
  arr(o.rows, "rows");
  return o as unknown as TryReply;
}

// ── 会话打开的副本 ──

/** 打开一份配置：`opened` · `dirty`（副本有没存的改动：先问，`discard` 再来一次）。 */
export type Opened = { state: "opened" } | { state: "dirty"; diff: ConfigDiff[] };

export async function openConfig(origin: Origin, sid: string, config: string, discard = false): Promise<Opened> {
  const body = jsonBody({ sid, config, ...(discard ? { discard: true } : {}) });
  const budget = budgetWithin(BUDGET_MS);
  const o = obj(readJson(await chan.call(origin, "rotation-session-open", body, budget)), "reply");
  if (o.state === "opened") return { state: "opened" };
  if (o.state === "dirty") return { state: "dirty", diff: arr(o.diff, "diff") as ConfigDiff[] };
  return bad(`state ${JSON.stringify(o.state)}`);
}

/** 一批会话各打开同一份（多选右键「轮换配置 ▸」）：有没存改动的那几个不动（`skipped`，码 `dirty`）。 */
export async function openConfigMany(origin: Origin, sids: string[], config: string): Promise<Record<string, SwitchOutcome>> {
  const out: Record<string, SwitchOutcome> = {};
  for (const sid of sids) {
    try {
      const got = await openConfig(origin, sid, config);
      out[sid] = got.state === "opened" ? { state: "done" } : { state: "skipped", code: "dirty" };
    } catch (e) {
      out[sid] = { state: "failed", code: e instanceof Error ? e.message : String(e) };
    }
  }
  return out;
}

/** 改副本的一格：`acct` 缺 ＝ 整份参数 · `@start` · `@rest` · `@init` · 号 id。 */
export interface CellSet {
  acct?: string;
  k: string;
  v: unknown;
}

/** 改副本：几格 · 加号 · 不参与 · 换策略。回改没改过、差哪几格、收不下的格。 */
export interface EditAsk {
  set?: CellSet[];
  add?: string[];
  drop?: string[];
  strategy?: string;
}

export interface Edited {
  dirty: boolean;
  diff: ConfigDiff[];
  errors: CellError[];
}

export async function editSession(origin: Origin, sid: string, ask: EditAsk): Promise<Edited> {
  const body = jsonBody({ sid, ...ask });
  const budget = budgetWithin(BUDGET_MS);
  const o = obj(readJson(await chan.call(origin, "rotation-session-edit", body, budget)), "reply");
  if (typeof o.dirty !== "boolean") bad("dirty");
  return { dirty: o.dirty, diff: arr(o.diff, "diff") as ConfigDiff[], errors: (o.errors === undefined ? [] : arr(o.errors, "errors")) as CellError[] };
}

/** 存副本：写回（带打开时的 `rev`）· 另存为（`as`）；撞版本 ⇒ 两边各改了哪几格。 */
export type Saved =
  | { state: "saved"; id: string; rev: number }
  | { state: "conflict"; rev: number; theirs: ConfigDiff[]; mine: ConfigDiff[] }
  | { state: "refused"; errors: CellError[] };

export async function saveSession(origin: Origin, sid: string, ask: { ifRev: number; as?: { name?: string }; force?: boolean }): Promise<Saved> {
  const body = jsonBody({ sid, ...ask });
  const budget = budgetWithin(BUDGET_MS);
  const o = obj(readJson(await chan.call(origin, "rotation-session-save", body, budget)), "reply");
  if (o.state === "saved") return { state: "saved", id: str(o.id, "id"), rev: num(o.rev, "rev") };
  if (o.state === "conflict") return { state: "conflict", rev: num(o.rev, "rev"), theirs: arr(o.theirs, "theirs") as ConfigDiff[], mine: arr(o.mine, "mine") as ConfigDiff[] };
  if (o.state === "refused") return { state: "refused", errors: arr(o.errors, "errors") as CellError[] };
  return bad(`state ${JSON.stringify(o.state)}`);
}

/** 还原 · 载入新版本（同一件事：副本丢掉、版本跟上）。 */
export async function revertSession(origin: Origin, sid: string, toLatest = false): Promise<void> {
  const body = jsonBody({ sid, ...(toLatest ? { toLatest: true } : {}) });
  const budget = budgetWithin(BUDGET_MS);
  readJson(await chan.call(origin, "rotation-session-revert", body, budget));
}

/** 手动调整的结局：`done` · 没换成（`why` 同 `rotation-switch` 的码）。 */
export interface Placed {
  state: "done" | "skipped" | "failed";
  why: string | null;
}

/** 手动调整：把一个号放到第 `at` 位（`0` ＝ 现在就换过去）· 刷新策略（清掉全部手动调整）。 */
export async function placeSession(origin: Origin, sid: string, ask: { place: { account: string; at: number } } | { refresh: true }): Promise<Placed> {
  const body = jsonBody({ sid, ...ask });
  const budget = budgetWithin(BUDGET_MS);
  const o = obj(readJson(await chan.call(origin, "rotation-session-set", body, budget)), "reply");
  if (o.state !== "done" && o.state !== "skipped" && o.state !== "failed") bad(`state ${JSON.stringify(o.state)}`);
  return { state: o.state, why: typeof o.why === "string" ? o.why : null };
}

// ── 配置表（设置窗）──

export type ConfigSaved =
  | { state: "saved"; id: string; rev: number; savedAtText: string }
  | { state: "refused"; errors: CellError[] }
  | { state: "conflict"; rev: number; theirs: ConfigDiff[] };

function decodeConfigSaved(v: unknown): ConfigSaved {
  const o = obj(v, "reply");
  if (o.state === "saved") return { state: "saved", id: str(o.id, "id"), rev: num(o.rev, "rev"), savedAtText: str(o.savedAtText, "savedAtText") };
  if (o.state === "refused") return { state: "refused", errors: arr(o.errors, "errors") as CellError[] };
  if (o.state === "conflict") return { state: "conflict", rev: num(o.rev, "rev"), theirs: (o.theirs === undefined ? [] : arr(o.theirs, "theirs")) as ConfigDiff[] };
  return bad(`state ${JSON.stringify(o.state)}`);
}

/** 新建（不给 `id`）或整份写一份配置。 */
export async function saveConfig(origin: Origin, args: { id?: string; config: ConfigBody; ifRev?: number; force?: boolean }): Promise<ConfigSaved> {
  const body = jsonBody(args);
  const budget = budgetWithin(BUDGET_MS);
  return decodeConfigSaved(readJson(await chan.call(origin, "rotation-config-save", body, budget)));
}

export async function renameConfig(origin: Origin, args: { id: string; name: string; ifRev: number }): Promise<ConfigSaved> {
  const body = jsonBody(args);
  const budget = budgetWithin(BUDGET_MS);
  return decodeConfigSaved(readJson(await chan.call(origin, "rotation-config-rename", body, budget)));
}

export type Dropped = { state: "done" } | { state: "refused"; said: string };

export async function deleteConfig(origin: Origin, id: string): Promise<Dropped> {
  const body = jsonBody({ id });
  const budget = budgetWithin(BUDGET_MS);
  const o = obj(readJson(await chan.call(origin, "rotation-config-delete", body, budget)), "reply");
  if (o.state === "done") return { state: "done" };
  if (o.state === "refused") return { state: "refused", said: str(o.said, "said") };
  return bad(`state ${JSON.stringify(o.state)}`);
}

export async function setDefaultConfig(origin: Origin, id: string): Promise<void> {
  const body = jsonBody({ id });
  const budget = budgetWithin(BUDGET_MS);
  readJson(await chan.call(origin, "rotation-default-set", body, budget));
}

// ── 策略表 ──

export type StrategyChanged = { state: "done"; id: string } | { state: "refused"; said: string; error?: StrategyError };

function decodeStrategyChanged(v: unknown): StrategyChanged {
  const o = obj(v, "reply");
  if (o.state === "done") return { state: "done", id: str(o.id, "id") };
  if (o.state === "refused") return { state: "refused", said: str(o.said, "said"), ...(o.error ? { error: o.error as StrategyError } : {}) };
  return bad(`state ${JSON.stringify(o.state)}`);
}

export async function addStrategy(origin: Origin, id: string, source: string): Promise<StrategyChanged> {
  const body = jsonBody({ id, source });
  const budget = budgetWithin(BUDGET_MS);
  return decodeStrategyChanged(readJson(await chan.call(origin, "rotation-strategy-add", body, budget)));
}

export async function deleteStrategy(origin: Origin, id: string): Promise<StrategyChanged> {
  const body = jsonBody({ id });
  const budget = budgetWithin(BUDGET_MS);
  return decodeStrategyChanged(readJson(await chan.call(origin, "rotation-strategy-delete", body, budget)));
}
