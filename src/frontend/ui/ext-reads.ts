/**
 * 设置「扩展」页的几问，**全走通道**：
 *
 * | 做什么 | 问哪台 | 命令 |
 * |---|---|---|
 * | 那张表（条目 × 机器：每格的点、那台上的各处、「装到…」能装到哪几处） | 本机后端 | `ext-list` |
 * | 装到一台之前那张确认卡 / 装 | 本机后端（枢纽，向来源取、交被写那台写） | `ext-hub-preview` / `ext-hub-apply` |
 * | 从一台的某一处卸之前那张卡 / 卸 | 被卸的那一台 | `ext-uninstall-preview` / `ext-uninstall-apply` |
 * | 写 / 改 / 清一个条目的备注 | 本机后端（记进它的目录，随目录同步） | `ext-note-set` |
 * | MCP 那几行每台的小标（需登录 · 断开）与抽屉里那一行的字 | 那一台（判定与写字都在它的后端：`mcp-read`） | `mcp-read` |
 *
 * 判定全在后端。「装」那一对是 typed（`callTyped`，回包形状由握手指纹核过）；其余几问这里按形状严格收（形状由后端的类型生成，`generated/Ext*.ts`；线上形状由金样
 * `tests/__fixtures__/ext-flow.golden.json` 钉），不比较、不推断。形状不对 ⇒ 抛「两端版本对不上」。
 */
import { chan, ChanError, type Budget } from "../../comms/inward/chan";
import { budgetWithin, callTyped, jsonBody, readJson, refusalOf, saidFrom, unreadableFrom } from "./ipc/chan-caller";
import { LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import type { ExtBring } from "./generated/ExtBring";
import type { ExtBuiltin } from "./generated/ExtBuiltin";
import type { ExtCard } from "./generated/ExtCard";
import type { ExtCell } from "./generated/ExtCell";
import type { ExtDone } from "./generated/ExtDone";
import type { ExtKind } from "./generated/ExtKind";
import type { ExtList } from "./generated/ExtList";
import type { ExtLoc } from "./generated/ExtLoc";
import type { ExtMachine } from "./generated/ExtMachine";
import type { ExtPlace } from "./generated/ExtPlace";
import type { ExtRow } from "./generated/ExtRow";
import type { ExtScope } from "./generated/ExtScope";
import type { ExtSlot } from "./generated/ExtSlot";
import type { ExtTokens } from "./generated/ExtTokens";
import type { HubApplied } from "./generated/HubApplied";
import type { HubPreview } from "./generated/HubPreview";
import type { ExtTarget } from "./generated/ExtTarget";
import type { ExtUninstallCard } from "./generated/ExtUninstallCard";
import { exactKeys, isObj } from "./ipc/decode";

export type { ExtBring, ExtBuiltin, ExtCard, ExtCell, ExtDone, ExtKind, ExtList, ExtLoc, ExtMachine, ExtPlace, ExtRow, ExtScope, ExtSlot, ExtTarget, ExtUninstallCard };

type Obj = Record<string, unknown>;
const bad = (): Error => unreadableFrom(LOCAL_ORIGIN, "ext reply shape");
function obj(v: unknown, keys: readonly string[]): Obj {
  if (!isObj(v) || !exactKeys(v, keys)) throw bad();
  return v;
}
const str = (v: unknown): string => {
  if (typeof v !== "string") throw bad();
  return v;
};
const optStr = (v: unknown): string | null => (v === null ? null : str(v));
const bool = (v: unknown): boolean => {
  if (typeof v !== "boolean") throw bad();
  return v;
};
function arr<T>(v: unknown, each: (x: unknown) => T): T[] {
  if (!Array.isArray(v)) throw bad();
  return v.map(each);
}
function oneOf<T extends string>(v: unknown, set: readonly T[]): T {
  if (typeof v !== "string" || !(set as readonly string[]).includes(v)) throw bad();
  return v as T;
}

const KINDS = ["skill", "mcp"] as const;
const STATES = ["same", "differs", "missing", "project"] as const;

function loc(v: unknown): ExtLoc {
  if (isObj(v) && v.level === "user") {
    obj(v, ["level"]);
    return { level: "user" };
  }
  const o = obj(v, ["level", "dir"]);
  if (o.level !== "project") throw bad();
  return { level: "project", dir: str(o.dir) };
}

function scope(v: unknown): ExtScope {
  const o = obj(v, ["from", "to"]);
  return { from: loc(o.from), to: loc(o.to) };
}

function target(v: unknown): ExtTarget {
  const o = obj(v, ["at", "ok", "note"]);
  return { at: loc(o.at), ok: bool(o.ok), note: optStr(o.note) };
}

function bring(v: unknown): ExtBring | null {
  if (v === null) return null;
  const o = obj(v, ["from", "fromName", "scope", "targets"]);
  return { from: optStr(o.from), fromName: str(o.fromName), scope: scope(o.scope), targets: arr(o.targets, target) };
}

function place(v: unknown): ExtPlace {
  const o = obj(v, ["at", "state", "dir", "uninstall", "note"]);
  return { at: loc(o.at), state: oneOf(o.state, STATES), dir: optStr(o.dir), uninstall: bool(o.uninstall), note: optStr(o.note) };
}

function cell(v: unknown): ExtCell {
  const o = obj(v, ["state", "places", "bring", "note"]);
  return { state: oneOf(o.state, STATES), places: arr(o.places, place), bring: bring(o.bring), note: optStr(o.note) };
}

function builtin(v: unknown): ExtBuiltin | null {
  if (v === null) return null;
  const o = obj(v, ["note", "hooks"]);
  return { note: str(o.note), hooks: bool(o.hooks) };
}

function machine(v: unknown): ExtMachine {
  const o = obj(v, ["key", "here", "reachable", "name", "projects"]);
  return { key: optStr(o.key), here: bool(o.here), reachable: bool(o.reachable), name: str(o.name), projects: arr(o.projects, str) };
}

function row(v: unknown, width: number): ExtRow {
  const o = obj(v, ["kind", "name", "about", "detail", "new", "builtin", "note", "cells"]);
  const cells = arr(o.cells, cell);
  if (cells.length !== width) throw bad();
  const detail = arr(o.detail, (d) => {
    const x = obj(d, ["label", "value"]);
    return { label: str(x.label), value: str(x.value) };
  });
  return { kind: oneOf(o.kind, KINDS), name: str(o.name), about: optStr(o.about), detail, new: bool(o.new), builtin: builtin(o.builtin), note: optStr(o.note), cells };
}

/** `ext-list` 的成品。严格收：每行的格数必须与机器数相等。 */
export function decodeExtList(v: unknown): ExtList {
  const o = obj(v, ["machines", "rows", "problems"]);
  const machines = arr(o.machines, machine);
  return { machines, rows: arr(o.rows, (r) => row(r, machines.length)), problems: arr(o.problems, str) };
}

/** `ext-uninstall-preview` 的成品。 */
export function decodeExtUninstallCard(v: unknown): ExtUninstallCard {
  const o = obj(v, ["kind", "name", "path", "recorded", "files", "backup", "said", "token"]);
  return {
    kind: oneOf(o.kind, KINDS),
    name: str(o.name),
    path: str(o.path),
    recorded: bool(o.recorded),
    files: arr(o.files, str),
    backup: optStr(o.backup),
    said: str(o.said),
    token: str(o.token),
  };
}

/** `ext-uninstall-apply` 的成品。 */
export function decodeExtDone(v: unknown): ExtDone {
  const o = obj(v, ["path", "changed", "note"]);
  return { path: str(o.path), changed: arr(o.changed, str), note: optStr(o.note) };
}

/** 一问没成：那句话 ＋ 后端的码（`stale` ⇒ 看过之后又变了，卡上让人重看）。 */
export class ExtRefused extends Error {
  constructor(
    message: string,
    readonly code: string | null,
  ) {
    super(message);
  }
}

function refused(e: unknown): ExtRefused {
  const err = e instanceof ChanError ? e.error : null;
  const code = err && err.layer === "peer" && err.why === "refused" ? (refusalOf(err.body)?.code ?? null) : null;
  return new ExtRefused(saidFrom(e, LOCAL_ORIGIN), code);
}

/** 读一趟：表在本机后端，远端慢时同步那一趟另走（`assets-sync-reads.ts`）。 */
const LIST_BUDGET_MS = 30_000;
/** 装 / 卸：远端那一跳要拨号、读写几个文件，给 120 秒。 */
const WRITE_BUDGET_MS = 120_000;

/** 发一问、按形状收；没成 ⇒ [`ExtRefused`]（带后端的码）。 */
async function ask<T>(go: (body: Uint8Array, budget: Budget) => Promise<Uint8Array>, args: Record<string, unknown>, ms: number, decode: (v: unknown) => T): Promise<T> {
  let reply: Uint8Array;
  try {
    const body = jsonBody(args);
    const budget = budgetWithin(ms);
    reply = await go(body, budget);
  } catch (e) {
    throw refused(e);
  }
  return decode(readJson(reply));
}

/** 那张表。`visit` = 这一问算「来看了一次」（页面每次变可见时的第一问）。 */
export function extList(visit: boolean): Promise<ExtList> {
  return ask((body, budget) => chan.call(LOCAL_ORIGIN, "ext-list", body, budget), { visit }, LIST_BUDGET_MS, decodeExtList);
}

/** 「装到 N 台」那一张卡（本机后端当枢纽，向各台问完并好）：落点交集 · 共用的那一处 · 并好的要填格 · 各台的卡（那台没拼成 ⇒ `card` 为 `null`、`error` 是那台说的那一句）。 */
export type ExtManyCard = HubPreview;

/** 各台各自结局。 */
export type ExtManyDone = HubApplied;

/** typed 那一对：入参 · 回包的类型从后端生成，形状由握手指纹核过；没成 ⇒ [`ExtRefused`]（带后端的码）。 */
async function askTyped<T>(go: (budget: Budget) => Promise<T>, ms: number): Promise<T> {
  try {
    return await go(budgetWithin(ms));
  } catch (e) {
    throw refused(e);
  }
}

/** 「装到 N 台」那一张卡：勾上的几台（`null` = 本机后端自己）＋ 用户选的那一处（没选 ⇒ 后端建议）。 */
export function extHubPreview(kind: ExtKind, name: string, to: (string | null)[], place: ExtLoc | null): Promise<ExtManyCard> {
  return askTyped((budget) => callTyped(LOCAL_ORIGIN, "ext-hub-preview", { kind, name, to, place }, budget), WRITE_BUDGET_MS);
}

/** 装到那几台：卡上那一处 ＋ 各台卡上的记号原样交回；`fill` 只填一次。 */
export function extHubApply(kind: ExtKind, name: string, card: ExtManyCard, fill: Record<string, Record<string, string>>): Promise<ExtManyDone> {
  const tokens: Record<string, ExtTokens> = {};
  for (const m of card.machines) if (m.card) tokens[m.to ?? ""] = m.card.tokens;
  const to = card.machines.map((m) => m.to);
  return askTyped((budget) => callTyped(LOCAL_ORIGIN, "ext-hub-apply", { kind, name, to, place: card.place, tokens, fill }, budget), WRITE_BUDGET_MS);
}

/** 卸之前那张卡（问被卸的那一台）。 */
export function extUninstallPreview(on: Origin, kind: ExtKind, name: string, at: ExtLoc): Promise<ExtUninstallCard> {
  return ask((body, budget) => chan.call(on, "ext-uninstall-preview", body, budget), { kind, name, at }, WRITE_BUDGET_MS, decodeExtUninstallCard);
}

/** 卸：卡上的记号原样交回。 */
export function extUninstallApply(on: Origin, card: ExtUninstallCard, at: ExtLoc): Promise<ExtDone> {
  const args = { kind: card.kind, name: card.name, at, token: card.token };
  return ask((body, budget) => chan.call(on, "ext-uninstall-apply", body, budget), args, WRITE_BUDGET_MS, decodeExtDone);
}

/** 写 / 改 / 清（空串）一个条目的备注（记进本机后端的目录，随目录同步到别的后端）：回现在生效的那一份（清掉了 ⇒ `null`）。 */
export async function extNoteSet(kind: ExtKind, name: string, text: string): Promise<string | null> {
  const got = await askTyped((budget) => callTyped(LOCAL_ORIGIN, "ext-note-set", { kind, name, text }, budget), LIST_BUDGET_MS);
  return got.note;
}

/** `mcp-read` 一条的「需登录」那几格（后端写好的字：抽屉那一句 · 悬停那一截 · ［去登录］复制的那条命令）。 */
export interface McpLoginMark {
  said: string;
  tip: string;
  copy: string;
}

/** `mcp-read` 一条的「断开」那几格（后端从这台活会话里最近说它的那一条写好：抽屉那一句 · 悬停那一截 · 原话）。 */
export interface McpFailedMark {
  said: string;
  tip: string;
  detail: string | null;
}

/** `mcp-read` 的一条（形状由金样 `tests/__fixtures__/mcp-read.golden.json` 钉）。`mark` = 小标画哪一种（后端定）。 */
export interface McpEntry {
  scope: "user" | "local" | "project";
  name: string;
  server: unknown;
  sourcePath: string;
  status: "needsLogin" | "disabled" | "unknown";
  loginIn: string[];
  seenAt: number | null;
  mark: "needsLogin" | "failed" | null;
  login: McpLoginMark | null;
  failed: McpFailedMark | null;
}

export interface McpRead {
  entries: McpEntry[];
  dirs: string[];
  problems: string[];
}

const MCP_SCOPES = ["user", "local", "project"] as const;
const MCP_STATUSES = ["needsLogin", "disabled", "unknown"] as const;
const MCP_MARKS = ["needsLogin", "failed"] as const;
const optNum = (v: unknown): number | null => {
  if (v === null) return null;
  if (typeof v !== "number") throw bad();
  return v;
};

/** `mcp-read` 的成品 → {@link McpRead}：按恰好的键集合收，不解释。 */
export function decodeMcpRead(v: unknown): McpRead {
  const o = obj(v, ["entries", "dirs", "problems"]);
  return {
    entries: arr(o.entries, (x) => {
      const e = obj(x, ["scope", "name", "server", "sourcePath", "status", "loginIn", "seenAt", "mark", "login", "failed"]);
      const login = e.login === null ? null : obj(e.login, ["said", "tip", "copy"]);
      const failed = e.failed === null ? null : obj(e.failed, ["said", "tip", "detail"]);
      return {
        scope: oneOf(e.scope, MCP_SCOPES),
        name: str(e.name),
        server: e.server,
        sourcePath: str(e.sourcePath),
        status: oneOf(e.status, MCP_STATUSES),
        loginIn: arr(e.loginIn, str),
        seenAt: optNum(e.seenAt),
        mark: e.mark === null ? null : oneOf(e.mark, MCP_MARKS),
        login: login && { said: str(login.said), tip: str(login.tip), copy: str(login.copy) },
        failed: failed && { said: str(failed.said), tip: str(failed.tip), detail: optStr(failed.detail) },
      };
    }),
    dirs: arr(o.dirs, str),
    problems: arr(o.problems, str),
  };
}

/** 问那台它的 MCP 列表（全局那一段；每条带后端写好的需登录 / 断开那几格）。本机远端同一条命令。 */
export function mcpRead(on: Origin): Promise<McpRead> {
  return ask((body, budget) => chan.call(on, "mcp-read", body, budget), {}, LIST_BUDGET_MS, decodeMcpRead);
}
