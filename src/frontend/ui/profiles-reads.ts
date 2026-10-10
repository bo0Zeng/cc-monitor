/**
 * 设置窗「别名与配置文件」那一页的五问**走通道，那台后端出成品**（后端 `assets/aliases/page.rs`）：
 *
 * | 做什么 | 帧命令 | 成品 |
 * |---|---|---|
 * | 读回配置文件整份（每段自己写的 · 能不能用 · 链接 / 终端函数 · 表单回填 · 空态预览 · 迁移说明） | `profiles-read` | [`ProfilesBook`] |
 * | 一段合下来的合并表与「等于」那一行（可按未存的表单、假设在某个目录敲） | `profiles-resolve` | [`Resolved`] |
 * | 这几处改动会连带哪几段（改前改后） | `profiles-impact` | [`Affected`]`[]` |
 * | 「基于」下拉能选的几段 | `profiles-bases` | [`Base`]`[]` |
 * | 按条目改配置文件（带读回时的指纹；盘上被别处改过 ⇒ [`ProfilesStale`]） | `profiles-write` | [`WriteDone`] |
 *
 * 合并 · 校验 · 标签 · 值怎么说全在那台后端；这里只按形状严格收（金样 `tests/__fixtures__/profiles.golden.json`）。
 */
import { chan, ChanError } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson, refusalOf, ReplyUnreadable, saidFrom } from "./ipc/chan-caller";
import { exactKeys, isObj } from "./ipc/decode";
import type { Origin } from "./ipc/origin";

/** 配置文件变了的那一种流（与 Rust `event_replay.rs::PROFILES_CHANGED_KIND` 同一个串）。 */
export const PROFILES_CHANGED_KIND = "profiles-changed";

export type AccountPick = { kind: "account"; name: string } | { kind: "base" };
export type TmuxMode = "auto" | "fixed" | "base";
export interface TmuxPick {
  mode: TmuxMode;
  name: string;
}
export interface CwdCase {
  at: string;
  to: string;
}

/** 一段的表单（线上形状与后端 `page::ProfileForm` 逐字）：`null` / `false` ＝ 继承（没写进这一段）。 */
export interface ProfileForm {
  name: string;
  from: string | null;
  account: AccountPick | null;
  tmux: TmuxPick | null;
  cwdIf: CwdCase[] | null;
  cwd: string | null;
  agent: string | null;
  args: string | null;
  launcher: string | null;
  tmuxSize: string | null;
  detach: boolean;
  busRegister: boolean;
  busNote: string | null;
}

/** 表单里可以写的几格（`detach` / `busRegister` 是开关，其余是值）。 */
export type SlotId = "account" | "tmux" | "cwdIf" | "cwd" | "agent" | "args" | "launcher" | "tmuxSize" | "detach" | "busRegister" | "busNote";
export const SLOT_IDS: readonly SlotId[] = ["account", "tmux", "cwdIf", "cwd", "agent", "args", "launcher", "tmuxSize", "detach", "busRegister", "busNote"];

export interface OwnItem {
  key: string;
  slot: SlotId;
  vals: string[];
  line: number;
}
export interface Problem {
  line: number | null;
  message: string;
  /** 复制详情（有下层原话时才有：TOML 解析器那一句）；没有 ⇒ `null`。 */
  detail: string | null;
}
export interface ProfileRow {
  name: string;
  from: string | null;
  own: OwnItem[];
  agent: string[];
  usable: boolean;
  problem: Problem | null;
  kind: "link" | "function";
  functionWhy: string | null;
  functionLine: string | null;
  /** 树里那一行（后端按表单同一套词写好）。 */
  said: string;
  form: ProfileForm;
  /** 「账号那一形」（自己只写了号、可再加 tmux，按合并下来的算）；其余 `null`。 */
  accountShape: { account: string; tmux: boolean } | null;
}
export interface Migrated {
  count: number;
  path: string;
  skipped: string[];
}
export interface ProfilesBook {
  home: string;
  path: string;
  exists: boolean;
  fingerprint: string | null;
  /** 上次 cc-monitor 写过之后有人改过 ⇒ 那份的修改时刻（后端写好）；否则 `null`。 */
  editedAt: string | null;
  fileProblem: Problem | null;
  profiles: ProfileRow[];
  seed: ProfileRow[];
  migrated: Migrated | null;
  binDir: string;
  accounts: string[];
  /** 这台有没有 tmux（那台后端判的；查不动 ⇒ `null`）。表单 tmux 那一格旁照它提示。 */
  tmux: boolean | null;
}
export interface MergeRow {
  key: string;
  slot: SlotId;
  label: string;
  vals: string[];
  said: string;
  from: string;
  overriddenBy: string | null;
}
export interface Resolved {
  chain: string[];
  rows: MergeRow[];
  line: string | null;
  lineError: string | null;
  problem: string | null;
}
export interface Affected {
  name: string;
  changes: { slot: SlotId; label: string; before: string; after: string }[];
  problem: string | null;
}
export interface Base {
  name: string;
  from: string | null;
  said: string;
  selectable: boolean;
}
export type ProfileOp =
  | { op: "set"; was: string | null; form: ProfileForm }
  | { op: "remove"; name: string; children?: "reparent" | "cascade" }
  | { op: "init"; seed: boolean }
  | { op: "ackMigrated" };
export interface WriteDone {
  wrote: boolean;
  fingerprint: string | null;
  reload: string | null;
}

/** 存的那一刻盘上那份不是读回时那一份（被别处改过）⇒ 一个字节没写；界面重读、表单留着。 */
export class ProfilesStale extends Error {}

const strs = (v: unknown): v is string[] => Array.isArray(v) && v.every((x) => typeof x === "string");
const optStr = (v: unknown): v is string | null => v === null || typeof v === "string";
const optNum = (v: unknown): v is number | null => v === null || typeof v === "number";
const bad = (): Error => new ReplyUnreadable("profiles reply shape");
const isSlot = (v: unknown): v is SlotId => typeof v === "string" && (SLOT_IDS as readonly string[]).includes(v);

function decodeForm(v: unknown): ProfileForm {
  const keys = ["name", "from", "account", "tmux", "cwdIf", "cwd", "agent", "args", "launcher", "tmuxSize", "detach", "busRegister", "busNote"];
  if (!isObj(v) || !exactKeys(v, keys) || typeof v.name !== "string" || typeof v.detach !== "boolean" || typeof v.busRegister !== "boolean") throw bad();
  const texts = ["from", "cwd", "agent", "args", "launcher", "tmuxSize", "busNote"] as const;
  if (!texts.every((k) => optStr(v[k]))) throw bad();
  let account: AccountPick | null = null;
  if (v.account !== null) {
    const a = v.account;
    if (isObj(a) && a.kind === "base" && exactKeys(a, ["kind"])) account = { kind: "base" };
    else if (isObj(a) && a.kind === "account" && exactKeys(a, ["kind", "name"]) && typeof a.name === "string") account = { kind: "account", name: a.name };
    else throw bad();
  }
  let tmux: TmuxPick | null = null;
  if (v.tmux !== null) {
    const t = v.tmux;
    if (!isObj(t) || !exactKeys(t, ["mode", "name"]) || typeof t.name !== "string" || (t.mode !== "auto" && t.mode !== "fixed" && t.mode !== "base")) throw bad();
    tmux = { mode: t.mode, name: t.name };
  }
  let cwdIf: CwdCase[] | null = null;
  if (v.cwdIf !== null) {
    if (!Array.isArray(v.cwdIf)) throw bad();
    cwdIf = v.cwdIf.map((c) => {
      if (!isObj(c) || !exactKeys(c, ["at", "to"]) || typeof c.at !== "string" || typeof c.to !== "string") throw bad();
      return { at: c.at, to: c.to };
    });
  }
  const s = (k: (typeof texts)[number]): string | null => v[k] as string | null;
  return {
    name: v.name,
    from: s("from"),
    account,
    tmux,
    cwdIf,
    cwd: s("cwd"),
    agent: s("agent"),
    args: s("args"),
    launcher: s("launcher"),
    tmuxSize: s("tmuxSize"),
    detach: v.detach,
    busRegister: v.busRegister,
    busNote: s("busNote"),
  };
}

function decodeProblem(v: unknown): Problem | null {
  if (v === null) return null;
  if (!isObj(v) || !exactKeys(v, ["line", "message", "detail"]) || !optNum(v.line) || typeof v.message !== "string" || !(v.detail === null || typeof v.detail === "string")) throw bad();
  return { line: v.line, message: v.message, detail: v.detail };
}

function decodeShape(v: unknown): { account: string; tmux: boolean } | null {
  if (v === null) return null;
  if (!isObj(v) || !exactKeys(v, ["account", "tmux"]) || typeof v.account !== "string" || typeof v.tmux !== "boolean") throw bad();
  return { account: v.account, tmux: v.tmux };
}

function decodeRow(v: unknown): ProfileRow {
  const keys = ["name", "from", "own", "agent", "usable", "problem", "kind", "functionWhy", "functionLine", "said", "form", "accountShape"];
  if (
    !isObj(v) ||
    !exactKeys(v, keys) ||
    typeof v.name !== "string" ||
    !optStr(v.from) ||
    !Array.isArray(v.own) ||
    !strs(v.agent) ||
    typeof v.usable !== "boolean" ||
    (v.kind !== "link" && v.kind !== "function") ||
    !optStr(v.functionWhy) ||
    !optStr(v.functionLine) ||
    typeof v.said !== "string"
  )
    throw bad();
  const own = v.own.map((o) => {
    if (!isObj(o) || !exactKeys(o, ["key", "slot", "vals", "line"]) || typeof o.key !== "string" || !isSlot(o.slot) || !strs(o.vals) || typeof o.line !== "number")
      throw bad();
    return { key: o.key, slot: o.slot, vals: o.vals, line: o.line };
  });
  return {
    name: v.name,
    from: v.from,
    own,
    agent: v.agent,
    usable: v.usable,
    problem: decodeProblem(v.problem),
    kind: v.kind,
    functionWhy: v.functionWhy,
    functionLine: v.functionLine,
    said: v.said,
    form: decodeForm(v.form),
    accountShape: decodeShape(v.accountShape),
  };
}

/** `profiles-read` 的成品。严格收。 */
export function decodeBook(v: unknown): ProfilesBook {
  const keys = ["home", "path", "exists", "fingerprint", "editedAt", "fileProblem", "profiles", "seed", "migrated", "binDir", "accounts", "tmux"];
  if (
    !isObj(v) ||
    !exactKeys(v, keys) ||
    typeof v.home !== "string" ||
    typeof v.path !== "string" ||
    typeof v.exists !== "boolean" ||
    !optStr(v.fingerprint) ||
    !optStr(v.editedAt) ||
    !Array.isArray(v.profiles) ||
    !Array.isArray(v.seed) ||
    typeof v.binDir !== "string" ||
    !strs(v.accounts) ||
    (v.tmux !== null && typeof v.tmux !== "boolean")
  )
    throw bad();
  let migrated: Migrated | null = null;
  if (v.migrated !== null) {
    const m = v.migrated;
    if (!isObj(m) || !exactKeys(m, ["count", "path", "skipped"]) || typeof m.count !== "number" || typeof m.path !== "string" || !strs(m.skipped)) throw bad();
    migrated = { count: m.count, path: m.path, skipped: m.skipped };
  }
  return {
    home: v.home,
    path: v.path,
    exists: v.exists,
    fingerprint: v.fingerprint,
    editedAt: v.editedAt,
    fileProblem: decodeProblem(v.fileProblem),
    profiles: v.profiles.map(decodeRow),
    seed: v.seed.map(decodeRow),
    migrated,
    binDir: v.binDir,
    accounts: v.accounts,
    tmux: v.tmux as boolean | null,
  };
}

/** `profiles-resolve` 的成品。严格收。 */
export function decodeResolved(v: unknown): Resolved {
  if (!isObj(v) || !exactKeys(v, ["chain", "rows", "line", "lineError", "problem"]) || !strs(v.chain) || !Array.isArray(v.rows) || !optStr(v.line) || !optStr(v.lineError) || !optStr(v.problem))
    throw bad();
  const rows = v.rows.map((r) => {
    if (
      !isObj(r) ||
      !exactKeys(r, ["key", "slot", "label", "vals", "said", "from", "overriddenBy"]) ||
      typeof r.key !== "string" ||
      !isSlot(r.slot) ||
      typeof r.label !== "string" ||
      !strs(r.vals) ||
      typeof r.said !== "string" ||
      typeof r.from !== "string" ||
      !optStr(r.overriddenBy)
    )
      throw bad();
    return { key: r.key, slot: r.slot, label: r.label, vals: r.vals, said: r.said, from: r.from, overriddenBy: r.overriddenBy };
  });
  return { chain: v.chain, rows, line: v.line, lineError: v.lineError, problem: v.problem };
}

/** `profiles-impact` 的成品。严格收。 */
export function decodeImpact(v: unknown): Affected[] {
  if (!isObj(v) || !exactKeys(v, ["affected"]) || !Array.isArray(v.affected)) throw bad();
  return v.affected.map((a) => {
    if (!isObj(a) || !exactKeys(a, ["name", "changes", "problem"]) || typeof a.name !== "string" || !Array.isArray(a.changes) || !optStr(a.problem)) throw bad();
    const changes = a.changes.map((c) => {
      if (!isObj(c) || !exactKeys(c, ["slot", "label", "before", "after"]) || !isSlot(c.slot) || typeof c.label !== "string" || typeof c.before !== "string" || typeof c.after !== "string")
        throw bad();
      return { slot: c.slot, label: c.label, before: c.before, after: c.after };
    });
    return { name: a.name, changes, problem: a.problem };
  });
}

/** `profiles-bases` 的成品。严格收。 */
export function decodeBases(v: unknown): Base[] {
  if (!isObj(v) || !exactKeys(v, ["bases"]) || !Array.isArray(v.bases)) throw bad();
  return v.bases.map((b) => {
    if (!isObj(b) || !exactKeys(b, ["name", "from", "said", "selectable"]) || typeof b.name !== "string" || !optStr(b.from) || typeof b.said !== "string" || typeof b.selectable !== "boolean")
      throw bad();
    return { name: b.name, from: b.from, said: b.said, selectable: b.selectable };
  });
}

/** `profiles-write` 的成品。严格收。 */
export function decodeWriteDone(v: unknown): WriteDone {
  if (!isObj(v) || !exactKeys(v, ["wrote", "fingerprint", "reload"]) || typeof v.wrote !== "boolean" || !optStr(v.fingerprint) || !optStr(v.reload))
    throw bad();
  return { wrote: v.wrote, fingerprint: v.fingerprint, reload: v.reload };
}

/** 五问的期限：读一份小文件、合并几层、写一份（秒级）。给 30 秒。 */
const PROFILES_BUDGET_MS = 30_000;

const said = (e: unknown, origin: Origin): Error => new Error(saidFrom(e, origin));

export async function readProfiles(origin: Origin): Promise<ProfilesBook> {
  try {
    const body = jsonBody({});
    const budget = budgetWithin(PROFILES_BUDGET_MS);
    return decodeBook(readJson(await chan.call(origin, "profiles-read", body, budget)));
  } catch (e) {
    throw said(e, origin);
  }
}

export async function resolveProfile(origin: Origin, name: string, edit: ProfileForm | null, at: string | null): Promise<Resolved> {
  try {
    const body = jsonBody({ name, edit, at });
    const budget = budgetWithin(PROFILES_BUDGET_MS);
    return decodeResolved(readJson(await chan.call(origin, "profiles-resolve", body, budget)));
  } catch (e) {
    throw said(e, origin);
  }
}

export async function profileImpact(origin: Origin, changes: ProfileOp[]): Promise<Affected[]> {
  try {
    const body = jsonBody({ changes });
    const budget = budgetWithin(PROFILES_BUDGET_MS);
    return decodeImpact(readJson(await chan.call(origin, "profiles-impact", body, budget)));
  } catch (e) {
    throw said(e, origin);
  }
}

export async function profileBases(origin: Origin, name: string): Promise<Base[]> {
  try {
    const body = jsonBody({ name });
    const budget = budgetWithin(PROFILES_BUDGET_MS);
    return decodeBases(readJson(await chan.call(origin, "profiles-bases", body, budget)));
  } catch (e) {
    throw said(e, origin);
  }
}

/** 按条目改：盘上被别处改过 ⇒ [`ProfilesStale`]（一个字节没写）。 */
export async function writeProfiles(origin: Origin, changes: ProfileOp[], fingerprint: string | null): Promise<WriteDone> {
  try {
    const body = jsonBody({ changes, fingerprint });
    const budget = budgetWithin(PROFILES_BUDGET_MS);
    return decodeWriteDone(readJson(await chan.call(origin, "profiles-write", body, budget)));
  } catch (e) {
    const err = e instanceof ChanError ? e.error : null;
    if (err && err.layer === "peer" && err.why === "refused" && refusalOf(err.body)?.code === "stale") {
      throw new ProfilesStale(saidFrom(e, origin));
    }
    throw said(e, origin);
  }
}
