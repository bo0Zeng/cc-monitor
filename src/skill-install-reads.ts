/**
 * 〔MIG-3a · `设计/99 §2.1 ⑬` · `01 §3.5` · 主会话 09-28 裁〕skill「装到这台」**只问本机一次**（本机常驻后端当枢纽），「卸」只问被卸那台：
 *
 * | 一步 | 问哪台 | 帧命令 | 成品 |
 * |---|---|---|---|
 * | 看差异（枢纽向 `from` 读、交 `to` 判） | 本机 | `skill-install-hub-preview` | `{dir, rows, target, source}` |
 * | 写（枢纽向 `from` 再读一次核对、交 `to` 判 · 写 · 记） | 本机 | `skill-install-hub-apply` | `{dir, written, chmodFailed, recordFailed}` |
 * | 卸（只删装时写的那几个；只涉一台） | `to` | `skill-uninstall-apply` | `{dir, deleted, recordFailed, dirRemoved, dirFailed}` |
 *
 * 前半那一形是界面先问来源那台读、再把原文递给被写那台 —— 经前端中继，撞 `01 §3.5`，主会话 09-28 裁改掉。
 * 判 · 写 · 记都在后端（`src/backend/assets/hub.rs` · `skill_flow.rs`）；这里零判定、按形状严格收（金样 `skill-flow.golden.json`）。
 */
import { chan } from "./comms/inward/chan";
import { budgetWithin, jsonBody, readJson, saidOf } from "./ipc/chan-caller";
import { LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import { hubMachine } from "./mcp-sync-reads";
import { copyText } from "./copy-table";

/** 来源那台的一个文件（原样；`text` 为 `null` ⇒ 读不出，`why` 说为什么）。 */
export interface SkillFile {
  path: string;
  text: string | null;
  exec: boolean;
  why: string | null;
}

/** 这台上一个文件的现有原文（写 / 删时的 CAS 期望）。 */
export interface SkillTargetText {
  path: string;
  text: string;
}

export interface SkillInstallSuspect {
  kind: string;
  value: string;
  there: string | null;
}

export interface SkillInstallRow {
  path: string;
  state: string;
  suspects: SkillInstallSuspect[];
  blocked: string | null;
}

export interface SkillInstallPreview {
  dir: string;
  rows: SkillInstallRow[];
  source: SkillFile[];
  target: SkillTargetText[];
}

export interface SkillInstallApplied {
  dir: string;
  written: string[];
  chmodFailed: string[];
  recordFailed: string | null;
}

export interface SkillUninstallApplied {
  dir: string;
  deleted: string[];
  recordFailed: string | null;
  dirRemoved: boolean;
  dirFailed: string | null;
}

const isObj = (v: unknown): v is Record<string, unknown> =>
  v !== null && typeof v === "object" && !Array.isArray(v);
const sameKeys = (o: Record<string, unknown>, want: readonly string[]): boolean => {
  const got = Object.keys(o).sort();
  const w = [...want].sort();
  return got.length === w.length && got.every((k, i) => k === w[i]);
};
const strs = (v: unknown): v is string[] => Array.isArray(v) && v.every((x) => typeof x === "string");
const optStr = (v: unknown): v is string | null => v === null || typeof v === "string";
const bad = (): Error => new Error(copyText("skillInstallReads.reply.badShape"));

function fileOf(v: unknown): SkillFile {
  // `skill-read` 每个文件还带 `bytes`（给人看的大小）；这里只收装要的四格。
  if (!isObj(v) || typeof v.path !== "string" || !optStr(v.text) || typeof v.exec !== "boolean" || !optStr(v.why))
    throw bad();
  return { path: v.path, text: v.text, exec: v.exec, why: v.why };
}

function targetOf(v: unknown): SkillTargetText {
  if (!isObj(v) || !sameKeys(v, ["path", "text"]) || typeof v.path !== "string" || typeof v.text !== "string")
    throw bad();
  return { path: v.path, text: v.text };
}

function rowOf(v: unknown): SkillInstallRow {
  if (
    !isObj(v) ||
    !sameKeys(v, ["path", "state", "suspects", "blocked"]) ||
    typeof v.path !== "string" ||
    typeof v.state !== "string" ||
    !Array.isArray(v.suspects) ||
    !optStr(v.blocked)
  )
    throw bad();
  return {
    path: v.path,
    state: v.state,
    suspects: v.suspects.map((s) => {
      if (!isObj(s) || typeof s.kind !== "string" || typeof s.value !== "string" || !optStr(s.there)) throw bad();
      return { kind: s.kind, value: s.value, there: s.there };
    }),
    blocked: v.blocked,
  };
}

/** `skill-install-apply` 的成品。严格收。 */
export function decodeSkillInstalled(v: unknown): SkillInstallApplied {
  if (
    !isObj(v) ||
    !sameKeys(v, ["dir", "written", "chmodFailed", "recordFailed"]) ||
    typeof v.dir !== "string" ||
    !strs(v.written) ||
    !strs(v.chmodFailed) ||
    !optStr(v.recordFailed)
  )
    throw bad();
  return { dir: v.dir, written: v.written, chmodFailed: v.chmodFailed, recordFailed: v.recordFailed };
}

/** `skill-uninstall-apply` 的成品。严格收。 */
export function decodeSkillUninstalled(v: unknown): SkillUninstallApplied {
  if (
    !isObj(v) ||
    !sameKeys(v, ["dir", "deleted", "recordFailed", "dirRemoved", "dirFailed"]) ||
    typeof v.dir !== "string" ||
    !strs(v.deleted) ||
    !optStr(v.recordFailed) ||
    typeof v.dirRemoved !== "boolean" ||
    !optStr(v.dirFailed)
  )
    throw bad();
  return {
    dir: v.dir,
    deleted: v.deleted,
    recordFailed: v.recordFailed,
    dirRemoved: v.dirRemoved,
    dirFailed: v.dirFailed,
  };
}

/** 一问的期限：走一遍 skill 目录、逐个写 / 删，一个 skill 是几个到几十个文件；给 60 秒（与从前 monitor 那一跳同值）。 */
const SKILL_BUDGET_MS = 60_000;
const said = (e: unknown): Error => new Error(saidOf(e, copyText("mcpReads.backend.tooOld")));

/** 看差异：`from` 那台的 skill `name` 装到 `to` 那台会发生什么（枢纽向来源那台读、交被写那台判）。 */
export async function skillInstallPreview(a: { from: Origin; to: Origin; name: string }): Promise<SkillInstallPreview> {
  try {
    const budget = budgetWithin(SKILL_BUDGET_MS);
    const body = jsonBody({ from: hubMachine(a.from), to: hubMachine(a.to), name: a.name });
    const v = readJson(await chan.call(LOCAL_ORIGIN, "skill-install-hub-preview", body, budget));
    if (
      !isObj(v) ||
      !sameKeys(v, ["dir", "rows", "target", "source"]) ||
      typeof v.dir !== "string" ||
      !Array.isArray(v.rows) ||
      !Array.isArray(v.target) ||
      !Array.isArray(v.source)
    )
      throw bad();
    return { dir: v.dir, rows: v.rows.map(rowOf), source: v.source.map(fileOf), target: v.target.map(targetOf) };
  } catch (e) {
    throw said(e);
  }
}

/** 写：把勾的那几个写进 `to` 那台。`source` / `target` 是看差异时那两份，只当期望送回（写的内容由枢纽向来源那台再读）。 */
export async function skillInstallApply(a: {
  from: Origin;
  to: Origin;
  name: string;
  source: SkillFile[];
  target: SkillTargetText[];
  take: string[];
  overwrite: string[];
}): Promise<SkillInstallApplied> {
  try {
    const body = jsonBody({
      from: hubMachine(a.from),
      to: hubMachine(a.to),
      name: a.name,
      expectSource: a.source,
      target: a.target,
      take: a.take,
      overwrite: a.overwrite,
    });
    const budget = budgetWithin(SKILL_BUDGET_MS);
    return decodeSkillInstalled(readJson(await chan.call(LOCAL_ORIGIN, "skill-install-hub-apply", body, budget)));
  } catch (e) {
    throw said(e);
  }
}

/** 卸：`to` 那台装记录里 `dir` 那一条，把勾的那几个删掉（`seen` = 看的时候那台回的现有原文）。 */
export async function skillUninstallApply(a: {
  to: Origin;
  dir: string;
  seen: SkillTargetText[];
  take: string[];
  confirm: string[];
}): Promise<SkillUninstallApplied> {
  try {
    const body = jsonBody({ dir: a.dir, seen: a.seen, take: a.take, confirm: a.confirm });
    const budget = budgetWithin(SKILL_BUDGET_MS);
    return decodeSkillUninstalled(readJson(await chan.call(a.to, "skill-uninstall-apply", body, budget)));
  } catch (e) {
    throw said(e);
  }
}
