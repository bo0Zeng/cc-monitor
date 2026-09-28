/**
 * 〔MIG-3a · `设计/99 §2.1 ⑬`〕skill 接入面（收件箱）**走通道，那台后端出成品**：
 *
 * | 做什么 | 帧命令 | 成品 |
 * |---|---|---|
 * | 列接进来的 skill | `skill-host-list` | `{skills: [SkillView]}` |
 * | 读那份可编辑文件 | `skill-host-read` | `{text}` |
 * | 写（CAS = 打开时读到的那一份） | `skill-host-write` | `{path}` |
 *
 * 从前是 monitor 的三条 Tauri 命令（`skill_host.rs`）；声明与三道围栏进了那台后端（`agents/claudecode/skill_host.rs`），
 * 这里只按形状严格收。**前端不做安全判断**。
 */
import { chan } from "./ipc/chan";
import { budgetWithin, jsonBody, readJson, saidOf } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import { copyText } from "./copy-table";

/** 一个接入的 skill 在那个项目里的样子（线上名与后端 `skill_host::views` 逐字）。 */
export interface SkillView {
  id: string;
  label: string;
  /** `null` = 在场；否则是带身份的缺席原因（哪个 skill 的哪条前提没满足）。 */
  missing_reason: string | null;
  /** 实例名（planned-build 的工作区名…）。 */
  instances: string[];
  /** 可编辑文件的绝对路径（后端算好的，UI 直接拿去请求读/写）。 */
  editable: string[];
}

const isObj = (v: unknown): v is Record<string, unknown> =>
  v !== null && typeof v === "object" && !Array.isArray(v);
const sameKeys = (o: Record<string, unknown>, want: readonly string[]): boolean => {
  const got = Object.keys(o).sort();
  const w = [...want].sort();
  return got.length === w.length && got.every((k, i) => k === w[i]);
};
const strs = (v: unknown): v is string[] => Array.isArray(v) && v.every((x) => typeof x === "string");
const bad = (): Error => new Error(copyText("skillInboxReads.reply.badShape"));

/** `skill-host-list` 的成品。严格收。 */
export function decodeSkillViews(v: unknown): SkillView[] {
  if (!isObj(v) || !sameKeys(v, ["skills"]) || !Array.isArray(v.skills)) throw bad();
  return v.skills.map((s) => {
    if (
      !isObj(s) ||
      !sameKeys(s, ["id", "label", "missing_reason", "instances", "editable"]) ||
      typeof s.id !== "string" ||
      typeof s.label !== "string" ||
      !(s.missing_reason === null || typeof s.missing_reason === "string") ||
      !strs(s.instances) ||
      !strs(s.editable)
    )
      throw bad();
    return { id: s.id, label: s.label, missing_reason: s.missing_reason, instances: s.instances, editable: s.editable };
  });
}

/** 一问的期限：列目录 / 读写一份小文件，秒级；给 30 秒。 */
const INBOX_BUDGET_MS = 30_000;
const said = (e: unknown): Error => new Error(saidOf(e, copyText("mcpReads.backend.tooOld")));

/** 那台机器上 `cwd` 那个项目里接进来的 skill。 */
export async function listSkills(origin: Origin, cwd: string): Promise<SkillView[]> {
  try {
    const body = jsonBody({ cwd });
    const budget = budgetWithin(INBOX_BUDGET_MS);
    return decodeSkillViews(readJson(await chan.call(origin, "skill-host-list", body, budget)));
  } catch (e) {
    throw said(e);
  }
}

/** 读那份可编辑文件（那台后端过围栏）。 */
export async function readSkillFile(origin: Origin, cwd: string, skillId: string, path: string): Promise<string> {
  try {
    const body = jsonBody({ cwd, skillId, path });
    const budget = budgetWithin(INBOX_BUDGET_MS);
    const v = readJson(await chan.call(origin, "skill-host-read", body, budget));
    if (!isObj(v) || !sameKeys(v, ["text"]) || typeof v.text !== "string") throw bad();
    return v.text;
  } catch (e) {
    throw said(e);
  }
}

/** 写那份可编辑文件；`expected` = 打开时读到的那一份（盘上那份在这之后被改过 ⇒ 那台后端拒、一个字节不写）。 */
export async function writeSkillFile(
  origin: Origin,
  cwd: string,
  skillId: string,
  path: string,
  content: string,
  expected: string,
): Promise<void> {
  try {
    const body = jsonBody({ cwd, skillId, path, content, expected });
    const budget = budgetWithin(INBOX_BUDGET_MS);
    const v = readJson(await chan.call(origin, "skill-host-write", body, budget));
    if (!isObj(v) || !sameKeys(v, ["path"]) || typeof v.path !== "string") throw bad();
  } catch (e) {
    throw said(e);
  }
}
