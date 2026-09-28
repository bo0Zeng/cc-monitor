/**
 * 〔MIG-3a · `设计/99 §2.1 ⑬`〕这台机器上的 `cc-acct-iso` 两问**走通道，那台后端出成品**：
 *
 * | 问什么 | 帧命令 | 成品 |
 * |---|---|---|
 * | 装没装、装在哪 | `acct-iso-status` | `{installed, path, looked}` |
 * | rc 片段（围栏已由那台后端校验过） | `acct-iso-shellinit` | `{snippet}` |
 * | 落进用户目录（部署推完字节之后） | `acct-iso-install` | `{link, linked, config, configWritten, recordFailed}` |
 *
 * 从前是 monitor 的两条 Tauri 命令（本机远端合一的 `acct_iso_status` / `acct_iso_shellinit`），它们在 monitor 里判读应答、
 * 校验围栏（本机远端各一份话）；那一份判读进了后端（`accounts/iso.rs`），这里只按形状严格收。
 * 〔09-28 裁 2〕落进用户目录那一步（`~/.local/bin` 链接 ＋ 配置样例）从前是部署命令经 ssh 跑安装脚本，今天是那台后端的
 * `acct-iso-install`（链接走写面 `files-link`，装卸账记 skill 装记录）；这里一样只按形状收。
 */
import { chan } from "./ipc/chan";
import { budgetWithin, jsonBody, readJson, saidOf } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import { copyText } from "./copy-table";

/** 那台上的 `cc-acct-iso`：`installed` 为假时 `looked` 说查过哪儿。 */
export interface AcctIsoStatus {
  installed: boolean;
  path: string | null;
  looked: string | null;
}

const isObj = (v: unknown): v is Record<string, unknown> =>
  v !== null && typeof v === "object" && !Array.isArray(v);
const sameKeys = (o: Record<string, unknown>, want: readonly string[]): boolean => {
  const got = Object.keys(o).sort();
  const w = [...want].sort();
  return got.length === w.length && got.every((k, i) => k === w[i]);
};
const optStr = (v: unknown): v is string | null => v === null || typeof v === "string";
const bad = (): Error => new Error(copyText("acctIsoReads.reply.badShape"));

/** `acct-iso-status` 的成品。严格收（「没装」是答案，缺格才是契约坏了）。 */
export function decodeAcctIsoStatus(v: unknown): AcctIsoStatus {
  if (!isObj(v) || !sameKeys(v, ["installed", "path", "looked"]) || typeof v.installed !== "boolean" || !optStr(v.path) || !optStr(v.looked))
    throw bad();
  return { installed: v.installed, path: v.path, looked: v.looked };
}

/** `acct-iso-shellinit` 的成品 ⇒ 片段。严格收。 */
export function decodeAcctIsoSnippet(v: unknown): string {
  if (!isObj(v) || !sameKeys(v, ["snippet"]) || typeof v.snippet !== "string") throw bad();
  return v.snippet;
}

/** 一次落进用户目录的结果。`linked` / `configWritten` 为假 = 那儿已有东西、没动它。 */
export interface AcctIsoInstalled {
  link: string;
  linked: boolean;
  config: string;
  configWritten: boolean;
  /** 装好了但没记进装记录时那一句；`null` = 记上了 / 没东西要记。 */
  recordFailed: string | null;
}

/** `acct-iso-install` 的成品。严格收。 */
export function decodeAcctIsoInstalled(v: unknown): AcctIsoInstalled {
  if (
    !isObj(v) ||
    !sameKeys(v, ["link", "linked", "config", "configWritten", "recordFailed"]) ||
    typeof v.link !== "string" ||
    typeof v.linked !== "boolean" ||
    typeof v.config !== "string" ||
    typeof v.configWritten !== "boolean" ||
    !optStr(v.recordFailed)
  )
    throw bad();
  return { link: v.link, linked: v.linked, config: v.config, configWritten: v.configWritten, recordFailed: v.recordFailed };
}

/** 三件的期限：`status` 只看文件在不在；`shellinit` 起一次 `cc-acct-iso`（后端那侧自带 20 秒）；`install` 几个小文件。给 30 秒。 */
const ACCT_ISO_BUDGET_MS = 30_000;
const said = (e: unknown): Error => new Error(saidOf(e, copyText("mcpReads.backend.tooOld")));

/** 那台机器装没装 `cc-acct-iso`。问不出来 ⇒ 抛（不许说成「没装」）。 */
export async function readAcctIsoStatus(origin: Origin): Promise<AcctIsoStatus> {
  try {
    const budget = budgetWithin(ACCT_ISO_BUDGET_MS);
    const body = jsonBody({});
    return decodeAcctIsoStatus(readJson(await chan.call(origin, "acct-iso-status", body, budget)));
  } catch (e) {
    throw said(e);
  }
}

/** 那台机器的 rc 片段（围栏不齐那台后端会拒、带着原因）。 */
export async function readAcctIsoSnippet(origin: Origin): Promise<string> {
  try {
    const budget = budgetWithin(ACCT_ISO_BUDGET_MS);
    const body = jsonBody({});
    return decodeAcctIsoSnippet(readJson(await chan.call(origin, "acct-iso-shellinit", body, budget)));
  } catch (e) {
    throw said(e);
  }
}

/** 把部署推到 `dir` 的那一份落进那台的用户目录（用户显式点了「部署」、字节推完之后才调）。 */
export async function installAcctIso(origin: Origin, dir: string): Promise<AcctIsoInstalled> {
  try {
    const budget = budgetWithin(ACCT_ISO_BUDGET_MS);
    const body = jsonBody({ dir });
    return decodeAcctIsoInstalled(readJson(await chan.call(origin, "acct-iso-install", body, budget)));
  } catch (e) {
    throw said(e);
  }
}
