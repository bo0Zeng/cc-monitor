/**
 * 〔MIG-3a · `设计/99 §2.1 ⑬` · 主会话 09-27 裁〕别名一族六问**走通道，那台后端出成品**：
 *
 * | 做什么 | 帧命令 | 成品 |
 * |---|---|---|
 * | 清单 → 代码（纯） | `aliases-render` | [`AliasRender`] |
 * | 读回清单 ＋ 启动文件候选 | `aliases-read` | [`AliasListing`] |
 * | 写别名文件（选了 rc 只查不装） | `aliases-install` | [`AliasInstallReport`] |
 * | 别名块预览（纯） | `aliases-block-render` | `{text}` |
 * | 别名块装 / 卸 | `aliases-block-install` / `-remove` | `{}` |
 *
 * 从前是 monitor 的六条 Tauri 命令（`aliases_*`，规则与方言在 monitor 一份、事实问那台后端）；规则 · 方言 · 围栏整族进了
 * 那台后端（`src/backend/assets/aliases/`），这里只按形状严格收。**前端不做安全判断**。
 * 「这台已握手的终端数」不是那台盘上的事实（住 monitor 进程里）⇒ 不在成品里，另问 monitor（`commands.bound_terminal_count`）。
 */
import { chan } from "./comms/inward/chan";
import { budgetWithin, jsonBody, readJson, saidOf } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import { copyText } from "./copy-table";

/** 方言：别名文件、别名块、source 那一行问的是同一个问题（线上名与后端 `dialect::Shell` 逐字）。 */
export type Shell = "posix" | "powershell";

/** 一条别名：名字 ＋ 原样的 ccm argv。 */
export interface Alias {
  name: string;
  args: string[];
}

/** 进不了代码的那一条为什么进不了。 */
export interface AliasProblem {
  name: string;
  message: string;
}

/** 清单 → 代码那一跳的成品。 */
export interface AliasRender {
  /** 整份文件（写入那一跳原样落盘的就是它）。 */
  fileText: string;
  /** 每条合格别名的写法，按清单顺序。 */
  lines: string[];
  /** 不合格的那几条（非空时写入那一跳一个字节都不写）。 */
  problems: AliasProblem[];
  /** 名字撞了的提示，一条一句（只出声、不拦）。 */
  collisions: string[];
}

/** 一份启动文件里别名块的现状。 */
export interface BlockState {
  present: boolean;
  version: string | null;
  outdated: boolean;
  conflictingFunctions: string[];
  /** 「你 rc 里这几行是旧的」那段话；空串 = 没有要清的。 */
  manualCleanupHint: string;
}

/** 一份候选启动文件（rc / `$PROFILE`）的状态。 */
export interface StartupFile {
  path: string;
  sourced: boolean;
  exists: boolean;
  block: BlockState;
  /** 在盘上、那台后端读不了它 —— 后端原话；`null` = 读得了或不在。 */
  unreadable: string | null;
}

/** 读回口的成品。 */
export interface AliasListing {
  aliasPath: string;
  exists: boolean;
  aliases: Alias[];
  unparsed: string[];
  rcCandidates: StartupFile[];
  /** 人另指的那一份过了围栏之后的绝对路径。 */
  otherRc: string | null;
}

/** 写入那一跳的成品。 */
export interface AliasInstallReport {
  aliasPath: string;
  wroteAliasFile: boolean;
  notes: string[];
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
const bad = (): Error => new Error(copyText("aliasReads.reply.badShape"));

function decodeAlias(v: unknown): Alias {
  if (!isObj(v) || !sameKeys(v, ["name", "args"]) || typeof v.name !== "string" || !strs(v.args)) throw bad();
  return { name: v.name, args: v.args };
}

/** `aliases-render` 的成品。严格收。 */
export function decodeAliasRender(v: unknown): AliasRender {
  if (
    !isObj(v) ||
    !sameKeys(v, ["fileText", "lines", "problems", "collisions"]) ||
    typeof v.fileText !== "string" ||
    !strs(v.lines) ||
    !strs(v.collisions) ||
    !Array.isArray(v.problems)
  )
    throw bad();
  const problems = v.problems.map((p) => {
    if (!isObj(p) || !sameKeys(p, ["name", "message"]) || typeof p.name !== "string" || typeof p.message !== "string")
      throw bad();
    return { name: p.name, message: p.message };
  });
  return { fileText: v.fileText, lines: v.lines, problems, collisions: v.collisions };
}

function decodeBlockState(v: unknown): BlockState {
  if (
    !isObj(v) ||
    !sameKeys(v, ["present", "version", "outdated", "conflictingFunctions", "manualCleanupHint"]) ||
    typeof v.present !== "boolean" ||
    !optStr(v.version) ||
    typeof v.outdated !== "boolean" ||
    !strs(v.conflictingFunctions) ||
    typeof v.manualCleanupHint !== "string"
  )
    throw bad();
  return {
    present: v.present,
    version: v.version,
    outdated: v.outdated,
    conflictingFunctions: v.conflictingFunctions,
    manualCleanupHint: v.manualCleanupHint,
  };
}

function decodeStartupFile(v: unknown): StartupFile {
  if (
    !isObj(v) ||
    !sameKeys(v, ["path", "sourced", "exists", "block", "unreadable"]) ||
    typeof v.path !== "string" ||
    typeof v.sourced !== "boolean" ||
    typeof v.exists !== "boolean" ||
    !optStr(v.unreadable)
  )
    throw bad();
  return { path: v.path, sourced: v.sourced, exists: v.exists, block: decodeBlockState(v.block), unreadable: v.unreadable };
}

/** `aliases-read` 的成品。严格收。 */
export function decodeAliasListing(v: unknown): AliasListing {
  if (
    !isObj(v) ||
    !sameKeys(v, ["aliasPath", "exists", "aliases", "unparsed", "rcCandidates", "otherRc"]) ||
    typeof v.aliasPath !== "string" ||
    typeof v.exists !== "boolean" ||
    !Array.isArray(v.aliases) ||
    !strs(v.unparsed) ||
    !Array.isArray(v.rcCandidates) ||
    !optStr(v.otherRc)
  )
    throw bad();
  return {
    aliasPath: v.aliasPath,
    exists: v.exists,
    aliases: v.aliases.map(decodeAlias),
    unparsed: v.unparsed,
    rcCandidates: v.rcCandidates.map(decodeStartupFile),
    otherRc: v.otherRc,
  };
}

/** `aliases-install` 的成品。严格收。 */
export function decodeAliasInstallReport(v: unknown): AliasInstallReport {
  if (
    !isObj(v) ||
    !sameKeys(v, ["aliasPath", "wroteAliasFile", "notes"]) ||
    typeof v.aliasPath !== "string" ||
    typeof v.wroteAliasFile !== "boolean" ||
    !strs(v.notes)
  )
    throw bad();
  return { aliasPath: v.aliasPath, wroteAliasFile: v.wroteAliasFile, notes: v.notes };
}

/** 六问的期限：读几份小文件 / 写一份 / 起一次 PowerShell 问内建别名（秒级）。给 30 秒。 */
const ALIAS_BUDGET_MS = 30_000;
const said = (e: unknown): Error => new Error(saidOf(e, copyText("mcpReads.backend.tooOld")));

/** 清单 → 代码（纯：一个字节都不写）。 */
export async function renderAliases(origin: Origin, aliases: Alias[], shell: Shell): Promise<AliasRender> {
  try {
    const body = jsonBody({ aliases, shell });
    const budget = budgetWithin(ALIAS_BUDGET_MS);
    return decodeAliasRender(readJson(await chan.call(origin, "aliases-render", body, budget)));
  } catch (e) {
    throw said(e);
  }
}

/** 读回那台上的别名文件 ＋ 启动文件候选；`rcPath` = 人另指的那一份（那台后端过围栏）。 */
export async function readAliases(origin: Origin, shell: Shell, rcPath: string | null): Promise<AliasListing> {
  try {
    const body = jsonBody({ shell, rcPath });
    const budget = budgetWithin(ALIAS_BUDGET_MS);
    return decodeAliasListing(readJson(await chan.call(origin, "aliases-read", body, budget)));
  } catch (e) {
    throw said(e);
  }
}

/** 写别名文件（收清单不收代码）；`rcPath` 只查接没接上、不往里写。 */
export async function installAliases(
  origin: Origin,
  aliases: Alias[],
  rcPath: string | null,
  shell: Shell,
): Promise<AliasInstallReport> {
  try {
    const body = jsonBody({ aliases, rcPath, shell });
    const budget = budgetWithin(ALIAS_BUDGET_MS);
    return decodeAliasInstallReport(readJson(await chan.call(origin, "aliases-install", body, budget)));
  } catch (e) {
    throw said(e);
  }
}

/** 别名块预览（往一份空文件里装一次会写成什么；方言由 `rcPath` 的扩展名定）。 */
export async function renderAliasBlock(origin: Origin, rcPath: string, withCc: boolean): Promise<string> {
  try {
    const body = jsonBody({ rcPath, withCc });
    const budget = budgetWithin(ALIAS_BUDGET_MS);
    const v = readJson(await chan.call(origin, "aliases-block-render", body, budget));
    if (!isObj(v) || !sameKeys(v, ["text"]) || typeof v.text !== "string") throw bad();
    return v.text;
  } catch (e) {
    throw said(e);
  }
}

const decodeEmpty = (v: unknown): void => {
  if (!isObj(v) || !sameKeys(v, [])) throw bad();
};

/** 别名块装进那台上人选的那份启动文件（幂等，整块替换）。 */
export async function installAliasBlock(origin: Origin, rcPath: string, withCc: boolean): Promise<void> {
  try {
    const body = jsonBody({ rcPath, withCc });
    const budget = budgetWithin(ALIAS_BUDGET_MS);
    decodeEmpty(readJson(await chan.call(origin, "aliases-block-install", body, budget)));
  } catch (e) {
    throw said(e);
  }
}

/** 别名块卸掉（整块删，块外一个字节不动）。 */
export async function removeAliasBlock(origin: Origin, rcPath: string): Promise<void> {
  try {
    const body = jsonBody({ rcPath });
    const budget = budgetWithin(ALIAS_BUDGET_MS);
    decodeEmpty(readJson(await chan.call(origin, "aliases-block-remove", body, budget)));
  } catch (e) {
    throw said(e);
  }
}
