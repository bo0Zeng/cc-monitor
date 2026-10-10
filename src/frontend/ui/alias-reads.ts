/**
 * 别名接入那几问**走通道，那台后端出成品**（清单那一半在 `./profiles-reads`）：
 *
 * | 做什么 | 帧命令 | 成品 |
 * |---|---|---|
 * | 启动文件候选（各带别名块的现状 · 块外同名函数 · 执行策略） | `aliases-read` | [`AliasListing`] |
 * | 别名块预览（纯） | `aliases-block-render` | `{text}` |
 * | 别名块装 / 卸 | `aliases-block-install` / `-remove` | `{}` |
 * | 执行策略设成当前用户 `RemoteSigned`（用户确认后） | `powershell-policy-set` | [`PolicySet`] |
 *
 * 规则 · 方言 · 围栏整族在那台后端（`src/backend/assets/aliases/`），这里只按形状严格收。**前端不做安全判断**。
 */
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson, ReplyUnreadable, saidFrom } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";

/** 方言：别名文件、别名块、source 那一行问的是同一个问题（线上名与后端 `dialect::Shell` 逐字）。 */
export type Shell = "posix" | "powershell";

/** 同名两条里新开的终端敲它起的是哪一个：你写的 · 清单那条 · 说不清（后端比行号，界面按码取句）。 */
export type ClashWins = "yours" | "list" | "unclear";

/** 启动文件里、块外自己定义的与清单同名的函数。 */
export interface NameClash {
  name: string;
  line: number;
  wins: ClashWins;
}

/** 一份启动文件里别名块的现状。 */
export interface BlockState {
  present: boolean;
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
  /** 把别名块装进这一份会写几行（那台后端按装进去的那一块数的）。 */
  blockLines: number;
}

/** `powershell-policy-set` 的成品：设完现问的那一份 ＋ 设的那一下 PowerShell 的原话（没报 ⇒ `null`）。 */
export interface PolicySet {
  policy: ExecPolicy;
  setError: string | null;
}

/** 读回口的成品：启动文件候选。 */
export interface AliasListing {
  /** 这台的家目录（把路径写成 `~/…` 短形用）。 */
  home: string;
  rcCandidates: StartupFile[];
  /** 人另指的那一份过了围栏之后的绝对路径。 */
  otherRc: string | null;
}

const isObj = (v: unknown): v is Record<string, unknown> =>
  v !== null && typeof v === "object" && !Array.isArray(v);
const sameKeys = (o: Record<string, unknown>, want: readonly string[]): boolean => {
  const got = Object.keys(o).sort();
  const w = [...want].sort();
  return got.length === w.length && got.every((k, i) => k === w[i]);
};
const optStr = (v: unknown): v is string | null => v === null || typeof v === "string";
const bad = (): Error => new ReplyUnreadable("aliases reply shape");

const WINS: readonly string[] = ["yours", "list", "unclear"];

function decodeClash(v: unknown): NameClash {
  if (
    !isObj(v) ||
    !sameKeys(v, ["name", "line", "wins"]) ||
    typeof v.name !== "string" ||
    typeof v.line !== "number" ||
    typeof v.wins !== "string" ||
    !WINS.includes(v.wins)
  )
    throw bad();
  return { name: v.name, line: v.line, wins: v.wins as ClashWins };
}

function decodeBlockState(v: unknown): BlockState {
  if (
    !isObj(v) ||
    !sameKeys(v, ["present", "conflictingFunctions", "manualCleanupHint"]) ||
    typeof v.present !== "boolean" ||
    !Array.isArray(v.conflictingFunctions) ||
    typeof v.manualCleanupHint !== "string"
  )
    throw bad();
  return {
    present: v.present,
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
    !sameKeys(v, ["path", "sourced", "exists", "block", "unreadable", "policy", "blockLines"]) ||
    typeof v.path !== "string" ||
    !(typeof v.blockLines === "number" && Number.isInteger(v.blockLines) && v.blockLines >= 0) ||
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
    blockLines: v.blockLines as number,
  };
}

/** `powershell-policy-set` 的成品。严格收。 */
export function decodePolicySet(v: unknown): PolicySet {
  if (!isObj(v) || !sameKeys(v, ["policy", "setError"]) || !optStr(v.setError)) throw bad();
  return { policy: decodePolicy(v.policy), setError: v.setError };
}

/** `aliases-read` 的成品。严格收。 */
export function decodeAliasListing(v: unknown): AliasListing {
  if (!isObj(v) || !sameKeys(v, ["home", "rcCandidates", "otherRc"]) || typeof v.home !== "string" || !Array.isArray(v.rcCandidates) || !optStr(v.otherRc))
    throw bad();
  return { home: v.home, rcCandidates: v.rcCandidates.map(decodeStartupFile), otherRc: v.otherRc };
}

/** 这几问的期限：读几份小文件 / 写一份 / 起一次 PowerShell 问内建别名（秒级）。给 30 秒。 */
const ALIAS_BUDGET_MS = 30_000;
const said = (e: unknown, origin: Origin): Error => new Error(saidFrom(e, origin));

/** 读回那台上的启动文件候选；`rcPath` = 人另指的那一份（那台后端过围栏）。 */
export async function readAliases(origin: Origin, shell: Shell, rcPath: string | null): Promise<AliasListing> {
  try {
    const body = jsonBody({ shell, rcPath });
    const budget = budgetWithin(ALIAS_BUDGET_MS);
    return decodeAliasListing(readJson(await chan.call(origin, "aliases-read", body, budget)));
  } catch (e) {
    throw said(e, origin);
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
    throw said(e, origin);
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
    throw said(e, origin);
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
    throw said(e, origin);
  }
}

/** 别名块卸掉（整块删，块外一个字节不动）。 */
export async function removeAliasBlock(origin: Origin, rcPath: string): Promise<void> {
  try {
    const body = jsonBody({ rcPath });
    const budget = budgetWithin(ALIAS_BUDGET_MS);
    decodeEmpty(readJson(await chan.call(origin, "aliases-block-remove", body, budget)));
  } catch (e) {
    throw said(e, origin);
  }
}
