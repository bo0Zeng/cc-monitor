/**
 * 〔MIG-3a · `设计/99 §2.1 ⑬`〕这台机器上的 `cc-acct-iso` 两问**走通道，那台后端出成品**：
 *
 * | 问什么 | 帧命令 | 成品 |
 * |---|---|---|
 * | 装没装、装在哪 | `acct-iso-status` | `{installed, path, looked}` |
 * | rc 片段（围栏已由那台后端校验过） | `acct-iso-shellinit` | `{snippet}` |
 * | 装到那台（字节随那台后端带着） | `acct-iso-install` | `{dest, version, written, link, linked, config, configWritten, recordFailed}` |
 *
 * 从前是 monitor 的两条 Tauri 命令（本机远端合一的 `acct_iso_status` / `acct_iso_shellinit`），它们在 monitor 里判读应答、
 * 校验围栏（本机远端各一份话）；那一份判读进了后端（`accounts/iso.rs`），这里只按形状严格收。
 * 〔09-28 裁 2 · 预裁〕装这一件从前是 monitor 推字节（`deploy_remote_acct_iso`〔散文墓碑〕）＋ 经 ssh 跑安装脚本，今天是那台后端的
 * `acct-iso-install`（字节随它带着 · 链接走写面 `files-link` · 装卸账记 skill 装记录）；这里一样只按形状收。
 */
import { chan } from "./comms/inward/chan";
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

/** 一次装的结果。`written === 0` = 字节已是这一版；`linked` / `configWritten` 为假 = 那儿已有东西、没动它。 */
export interface AcctIsoInstalled {
  dest: string;
  version: string;
  written: number;
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
    !sameKeys(v, ["dest", "version", "written", "link", "linked", "config", "configWritten", "recordFailed"]) ||
    typeof v.dest !== "string" ||
    typeof v.version !== "string" ||
    typeof v.written !== "number" ||
    !Number.isInteger(v.written) ||
    v.written < 0 ||
    typeof v.link !== "string" ||
    typeof v.linked !== "boolean" ||
    typeof v.config !== "string" ||
    typeof v.configWritten !== "boolean" ||
    !optStr(v.recordFailed)
  )
    throw bad();
  return {
    dest: v.dest,
    version: v.version,
    written: v.written,
    link: v.link,
    linked: v.linked,
    config: v.config,
    configWritten: v.configWritten,
    recordFailed: v.recordFailed,
  };
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

/** 把那台后端带着的 cc-acct-iso 装到那台（用户显式点了「部署」才调；落点那台自己算）。 */
export async function installAcctIso(origin: Origin): Promise<AcctIsoInstalled> {
  try {
    const budget = budgetWithin(ACCT_ISO_BUDGET_MS);
    const body = jsonBody({});
    return decodeAcctIsoInstalled(readJson(await chan.call(origin, "acct-iso-install", body, budget)));
  } catch (e) {
    throw said(e);
  }
}
