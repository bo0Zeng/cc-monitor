/**
 * 别名一族六问**走通道，那台后端出成品**：
 *
 * | 做什么 | 帧命令 | 成品 |
 * |---|---|---|
 * | 清单 → 代码（纯） | `aliases-render` | [`AliasRender`] |
 * | 读回清单（＋ 归组 · 账号表 · 缺的 · 指纹）＋ 启动文件候选 | `aliases-read` | [`AliasListing`] |
 * | 写别名文件（带读回时的指纹；盘上被别处改过 ⇒ [`AliasesStale`]） | `aliases-install` | [`AliasInstallReport`] |
 * | 别名块预览（纯） | `aliases-block-render` | `{text}` |
 * | 别名块装 / 卸 | `aliases-block-install` / `-remove` | `{}` |
 * | 执行策略设成当前用户 `RemoteSigned`（用户确认后） | `powershell-policy-set` | [`PolicySet`] |
 *
 * 从前是 monitor 的六条 Tauri 命令（`aliases_*`，规则与方言在 monitor 一份、事实问那台后端）；规则 · 方言 · 围栏整族进了
 * 那台后端（`src/backend/assets/aliases/`），这里只按形状严格收。**前端不做安全判断**。
 * 「这台已握手的终端数」不是那台盘上的事实（住 monitor 进程里）⇒ 不在成品里，另问 monitor（`commands.bound_terminal_count`）。
 */
import { chan, ChanError } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson, refusalOf, saidOf } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import { copyText } from "./copy-table";

/** 方言：别名文件、别名块、source 那一行问的是同一个问题（线上名与后端 `dialect::Shell` 逐字）。 */
export type Shell = "posix" | "powershell";

/** 调用时跟在别名后面的词交给谁（线上名与后端 `dialect::RestTo` 逐字）：缺省 claude；ccm 只有接回会话那一形。 */
export type RestTo = "agent" | "ccm";

/** 一条别名：名字 ＋ 原样的 ccm argv ＋ 调用时的词交给谁。 */
export interface Alias {
  name: string;
  args: string[];
  restTo: RestTo;
}

/** 读回清单里一条的归组（后端认的「账号那一形」）。 */
export interface AccountShape {
  account: string;
  tmux: boolean;
}

/** 账号表里一个号缺的那一条：点「加上」就把 `alias` 加进清单。 */
export interface MissingAlias {
  account: string;
  tmux: boolean;
  alias: Alias;
}

/** 启动文件里、块外自己定义的与清单同名的函数。 */
export interface NameClash {
  name: string;
  line: number;
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
  conflictingFunctions: NameClash[];
  /** 「你 rc 里这几行是旧的」那段话；空串 = 没有要清的。 */
  manualCleanupHint: string;
}

/** 哪一代 PowerShell（线上名与后端 `platform::shell::PsHost` 逐字）。 */
export type PsHost = "powershell" | "pwsh";

/** 加载这份 `$PROFILE` 的那一代 PowerShell 的执行策略（那台后端现问）。 */
export interface ExecPolicy {
  host: PsHost;
  /** 生效那一档的原词；问不到 ⇒ `null`，原话在 `error`。 */
  effective: string | null;
  /** 这一档下它会不会跑我们装的块（本地、未签名）；说不清 ⇒ `null`。判在后端。 */
  loads: boolean | null;
  /** 组策略钉着 ⇒ 改当前用户那一档没用。 */
  groupPolicy: boolean;
  error: string | null;
}

/** 一份候选启动文件（rc / `$PROFILE`）的状态。 */
export interface StartupFile {
  path: string;
  sourced: boolean;
  exists: boolean;
  block: BlockState;
  /** 在盘上、那台后端读不了它 —— 后端原话；`null` = 读得了或不在。 */
  unreadable: string | null;
  /** 只有 `$PROFILE` 那几份有；POSIX 与人另指的那一份 ⇒ `null`。 */
  policy: ExecPolicy | null;
}

/** `powershell-policy-set` 的成品：设完现问的那一份 ＋ 设的那一下 PowerShell 的原话（没报 ⇒ `null`）。 */
export interface PolicySet {
  policy: ExecPolicy;
  setError: string | null;
}

/** 读回口的成品。 */
export interface AliasListing {
  aliasPath: string;
  exists: boolean;
  /** 不在盘上 ⇒ 首建会带上的那几条。 */
  aliases: Alias[];
  /** 与 `aliases` 逐条对应：账号那一形 ⇒ `{account, tmux}`，其余 ⇒ `null`（界面照画，不自己认）。 */
  groups: Array<AccountShape | null>;
  /** 这台的账号表。 */
  accounts: string[];
  missing: MissingAlias[];
  /** 盘上那份的指纹（不在 ⇒ `null`）；存的时候原样交回。 */
  fingerprint: string | null;
  unparsed: string[];
  rcCandidates: StartupFile[];
  /** 人另指的那一份过了围栏之后的绝对路径。 */
  otherRc: string | null;
}

/** 写入那一跳的成品。 */
export interface AliasInstallReport {
  aliasPath: string;
  wroteAliasFile: boolean;
}

/** 存的那一刻盘上那份不是读回时那一份（被别处改过）⇒ 什么都没写；界面重读再让人存。 */
export class AliasesStale extends Error {}

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
  if (
    !isObj(v) ||
    !sameKeys(v, ["name", "args", "restTo"]) ||
    typeof v.name !== "string" ||
    !strs(v.args) ||
    (v.restTo !== "agent" && v.restTo !== "ccm")
  )
    throw bad();
  return { name: v.name, args: v.args, restTo: v.restTo };
}

function decodeShape(v: unknown): AccountShape | null {
  if (v === null) return null;
  if (!isObj(v) || !sameKeys(v, ["account", "tmux"]) || typeof v.account !== "string" || typeof v.tmux !== "boolean") throw bad();
  return { account: v.account, tmux: v.tmux };
}

function decodeMissing(v: unknown): MissingAlias {
  if (!isObj(v) || !sameKeys(v, ["account", "tmux", "alias"]) || typeof v.account !== "string" || typeof v.tmux !== "boolean")
    throw bad();
  return { account: v.account, tmux: v.tmux, alias: decodeAlias(v.alias) };
}

function decodeClash(v: unknown): NameClash {
  if (!isObj(v) || !sameKeys(v, ["name", "line"]) || typeof v.name !== "string" || typeof v.line !== "number") throw bad();
  return { name: v.name, line: v.line };
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
    !Array.isArray(v.conflictingFunctions) ||
    typeof v.manualCleanupHint !== "string"
  )
    throw bad();
  return {
    present: v.present,
    version: v.version,
    outdated: v.outdated,
    conflictingFunctions: v.conflictingFunctions.map(decodeClash),
    manualCleanupHint: v.manualCleanupHint,
  };
}

function decodePolicy(v: unknown): ExecPolicy {
  if (
    !isObj(v) ||
    !sameKeys(v, ["host", "effective", "loads", "groupPolicy", "error"]) ||
    (v.host !== "powershell" && v.host !== "pwsh") ||
    !optStr(v.effective) ||
    (v.loads !== null && typeof v.loads !== "boolean") ||
    typeof v.groupPolicy !== "boolean" ||
    !optStr(v.error)
  )
    throw bad();
  return { host: v.host, effective: v.effective, loads: v.loads, groupPolicy: v.groupPolicy, error: v.error };
}

function decodeStartupFile(v: unknown): StartupFile {
  if (
    !isObj(v) ||
    !sameKeys(v, ["path", "sourced", "exists", "block", "unreadable", "policy"]) ||
    typeof v.path !== "string" ||
    typeof v.sourced !== "boolean" ||
    typeof v.exists !== "boolean" ||
    !optStr(v.unreadable)
  )
    throw bad();
  return {
    path: v.path,
    sourced: v.sourced,
    exists: v.exists,
    block: decodeBlockState(v.block),
    unreadable: v.unreadable,
    policy: v.policy === null ? null : decodePolicy(v.policy),
  };
}

/** `powershell-policy-set` 的成品。严格收。 */
export function decodePolicySet(v: unknown): PolicySet {
  if (!isObj(v) || !sameKeys(v, ["policy", "setError"]) || !optStr(v.setError)) throw bad();
  return { policy: decodePolicy(v.policy), setError: v.setError };
}

/** `aliases-read` 的成品。严格收。 */
export function decodeAliasListing(v: unknown): AliasListing {
  if (
    !isObj(v) ||
    !sameKeys(v, [
      "aliasPath",
      "exists",
      "aliases",
      "groups",
      "accounts",
      "missing",
      "fingerprint",
      "unparsed",
      "rcCandidates",
      "otherRc",
    ]) ||
    typeof v.aliasPath !== "string" ||
    typeof v.exists !== "boolean" ||
    !Array.isArray(v.aliases) ||
    !Array.isArray(v.groups) ||
    v.groups.length !== v.aliases.length ||
    !strs(v.accounts) ||
    !Array.isArray(v.missing) ||
    !optStr(v.fingerprint) ||
    !strs(v.unparsed) ||
    !Array.isArray(v.rcCandidates) ||
    !optStr(v.otherRc)
  )
    throw bad();
  return {
    aliasPath: v.aliasPath,
    exists: v.exists,
    aliases: v.aliases.map(decodeAlias),
    groups: v.groups.map(decodeShape),
    accounts: v.accounts,
    missing: v.missing.map(decodeMissing),
    fingerprint: v.fingerprint,
    unparsed: v.unparsed,
    rcCandidates: v.rcCandidates.map(decodeStartupFile),
    otherRc: v.otherRc,
  };
}

/** `aliases-install` 的成品。严格收。 */
export function decodeAliasInstallReport(v: unknown): AliasInstallReport {
  if (
    !isObj(v) ||
    !sameKeys(v, ["aliasPath", "wroteAliasFile"]) ||
    typeof v.aliasPath !== "string" ||
    typeof v.wroteAliasFile !== "boolean"
  )
    throw bad();
  return { aliasPath: v.aliasPath, wroteAliasFile: v.wroteAliasFile };
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

/** 写别名文件（收清单不收代码）；`fingerprint` = 读回时那份的指纹。盘上被别处改过 ⇒ 抛 [`AliasesStale`]。 */
export async function installAliases(
  origin: Origin,
  aliases: Alias[],
  shell: Shell,
  fingerprint: string | null,
): Promise<AliasInstallReport> {
  try {
    const body = jsonBody({ aliases, shell, fingerprint });
    const budget = budgetWithin(ALIAS_BUDGET_MS);
    return decodeAliasInstallReport(readJson(await chan.call(origin, "aliases-install", body, budget)));
  } catch (e) {
    const err = e instanceof ChanError ? e.error : null;
    if (err && err.layer === "peer" && err.why === "refused" && refusalOf(err.body)?.code === "stale") {
      throw new AliasesStale(saidOf(e, copyText("mcpReads.backend.tooOld")));
    }
    throw said(e);
  }
}

/** 别名块预览（往一份空文件里装一次会写成什么；方言由 `rcPath` 的扩展名定）。 */
export async function renderAliasBlock(origin: Origin, rcPath: string): Promise<string> {
  try {
    const body = jsonBody({ rcPath });
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
export async function installAliasBlock(origin: Origin, rcPath: string): Promise<void> {
  try {
    const body = jsonBody({ rcPath });
    const budget = budgetWithin(ALIAS_BUDGET_MS);
    decodeEmpty(readJson(await chan.call(origin, "aliases-block-install", body, budget)));
  } catch (e) {
    throw said(e);
  }
}

/**
 * 把那一代 PowerShell 的执行策略设成当前用户 `RemoteSigned`（后端只做这一件固定的事）。
 * **只在用户点了、确认了之后调**（不代改）。
 */
export async function allowLocalScripts(origin: Origin, host: PsHost): Promise<PolicySet> {
  try {
    const body = jsonBody({ host });
    const budget = budgetWithin(ALIAS_BUDGET_MS);
    return decodePolicySet(readJson(await chan.call(origin, "powershell-policy-set", body, budget)));
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
