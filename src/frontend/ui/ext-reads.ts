/**
 * 设置「扩展」页的几问，**全走通道**：
 *
 * | 做什么 | 问哪台 | 命令 |
 * |---|---|---|
 * | 那张表（条目 × 机器：每格的点、那台上的各处、「装到…」能装到哪几处） | 本机后端 | `ext-list` |
 * | 装到一台之前那张确认卡 / 装 | 本机后端（枢纽，向来源取、交被写那台写） | `ext-hub-preview` / `ext-hub-apply` |
 * | 从一台的某一处卸之前那张卡 / 卸 | 被卸的那一台 | `ext-uninstall-preview` / `ext-uninstall-apply` |
 * | 写 / 改 / 清一个条目的备注 | 本机后端（记进它的目录，随目录同步） | `ext-note-set` |
 *
 * 判定全在后端：这里只按形状严格收（形状由后端的类型生成，`generated/Ext*.ts`；线上形状由金样
 * `tests/__fixtures__/ext-flow.golden.json` 钉），不比较、不推断。形状不对 ⇒ 抛「两端版本对不上」。
 */
import { chan, ChanError, type Budget } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson, refusalOf, saidFrom, unreadableFrom } from "./ipc/chan-caller";
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

/** `ext-hub-preview` 的成品（确认卡）。 */
export function decodeExtCard(v: unknown): ExtCard {
  const o = obj(v, ["kind", "name", "path", "writes", "unchanged", "suspects", "stop", "config", "slots", "tokens"]);
  const t = obj(o.tokens, ["source", "target"]);
  return {
    kind: oneOf(o.kind, KINDS),
    name: str(o.name),
    path: str(o.path),
    writes: arr(o.writes, str),
    unchanged: bool(o.unchanged),
    suspects: arr(o.suspects, str),
    stop: optStr(o.stop),
    config: optStr(o.config),
    slots: arr(o.slots, (s) => {
      const x = obj(s, ["field", "key", "kept"]);
      return { field: str(x.field), key: str(x.key), kept: bool(x.kept) };
    }),
    tokens: { source: str(t.source), target: optStr(t.target) },
  };
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

/** `ext-hub-apply` / `ext-uninstall-apply` 的成品。 */
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

/** 「装到 N 台」那一张卡（本机后端当枢纽，向各台问完并好）：落点交集 · 共用的那一处 · 并好的要填格 · 各台的卡。 */
export interface ExtManyCard {
  places: ExtTarget[];
  place: ExtLoc | null;
  slots: { field: string; key: string; kept: string[] }[];
  /** 那台没拼成 ⇒ `card` 为 `null`、`error` 是那台说的那一句。 */
  machines: { to: string | null; name: string; card: ExtCard | null; files: string[]; error: string | null }[];
}

/** 各台各自结局。 */
export interface ExtManyDone {
  machines: { to: string | null; name: string; done: ExtDone | null; error: string | null }[];
}

const keyOrNull = (v: unknown): string | null => (v === null ? null : str(v));
/** `ext-hub-preview` 的成品。严格收。 */
export function decodeExtManyCard(v: unknown): ExtManyCard {
  const o = obj(v, ["places", "place", "slots", "machines"]);
  return {
    places: arr(o.places, target),
    place: o.place === null ? null : loc(o.place),
    slots: arr(o.slots, (x) => {
      const s = obj(x, ["field", "key", "kept"]);
      return { field: str(s.field), key: str(s.key), kept: arr(s.kept, str) };
    }),
    machines: arr(o.machines, (x) => {
      const m = obj(x, ["to", "name", "card", "files", "error"]);
      return { to: keyOrNull(m.to), name: str(m.name), card: m.card === null ? null : decodeExtCard(m.card), files: arr(m.files, str), error: optStr(m.error) };
    }),
  };
}

/** `ext-hub-apply` 的成品。严格收。 */
export function decodeExtManyDone(v: unknown): ExtManyDone {
  const o = obj(v, ["machines"]);
  return {
    machines: arr(o.machines, (x) => {
      const m = obj(x, ["to", "name", "done", "error"]);
      return { to: keyOrNull(m.to), name: str(m.name), done: m.done === null ? null : decodeExtDone(m.done), error: optStr(m.error) };
    }),
  };
}

/** 「装到 N 台」那一张卡：勾上的几台（`null` = 本机后端自己）＋ 用户选的那一处（没选 ⇒ 后端建议）。 */
export function extHubPreview(kind: ExtKind, name: string, to: (string | null)[], place: ExtLoc | null): Promise<ExtManyCard> {
  return ask((body, budget) => chan.call(LOCAL_ORIGIN, "ext-hub-preview", body, budget), { kind, name, to, place }, WRITE_BUDGET_MS, decodeExtManyCard);
}

/** 装到那几台：卡上那一处 ＋ 各台卡上的记号原样交回；`fill` 只填一次。 */
export function extHubApply(kind: ExtKind, name: string, card: ExtManyCard, fill: Record<string, Record<string, string>>): Promise<ExtManyDone> {
  const tokens: Record<string, unknown> = {};
  for (const m of card.machines) if (m.card) tokens[m.to ?? ""] = m.card.tokens;
  const to = card.machines.map((m) => m.to);
  return ask((body, budget) => chan.call(LOCAL_ORIGIN, "ext-hub-apply", body, budget), { kind, name, to, place: card.place, tokens, fill }, WRITE_BUDGET_MS, decodeExtManyDone);
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

/** `ext-note-set` 的成品：现在生效的那一份备注（清掉了 ⇒ `null`）。 */
export function decodeExtNote(v: unknown): string | null {
  return optStr(obj(v, ["note"]).note);
}

/** 写 / 改 / 清（空串）一个条目的备注（记进本机后端的目录，随目录同步到别的后端）。 */
export function extNoteSet(kind: ExtKind, name: string, text: string): Promise<string | null> {
  return ask((body, budget) => chan.call(LOCAL_ORIGIN, "ext-note-set", body, budget), { kind, name, text }, LIST_BUDGET_MS, decodeExtNote);
}
