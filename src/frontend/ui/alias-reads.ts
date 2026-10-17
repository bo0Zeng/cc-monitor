/**
 * 别名接入那几问**走通道，那台后端出成品**（清单那一半在 `./profiles-reads`）：
 *
 * | 做什么 | 帧命令 | 成品 |
 * |---|---|---|
 * | 启动文件候选（各带别名块的现状 · 块外同名函数 · 执行策略） | `aliases-read` | [`AliasListing`] |
 * | 别名块预览（纯） | `aliases-block-render` | `{text}` |
 * | 别名块装 / 卸 | `aliases-block-install` / `-remove` | `{}` |
 * | 执行策略设成当前用户 `RemoteSigned`（用户确认后） | `powershell-policy-set` | [`PolicySet`] |
 * | 终端里敲 `ccm` 走到哪一份（判词 ＋ 那一句） | `ccm-probe` 的 `onPath` | [`OnPath`] |
 * | `ccm` 所在目录在不在用户级 PATH 上 · 加 · 撤（做完现问） | `ccm-user-path` | [`UserPath`] |
 *
 * 规则 · 方言 · 围栏整族在那台后端（`src/backend/assets/aliases/`）。入参 · 回包都是后端类型生成的（`generated/`），
 * 回包形状由握手时的形状指纹管，这里不逐格判；没成原样抛（那一句 ＋ 详情是出错那一端写的）。**前端不做安全判断**。
 */
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import type { ShellKind } from "./generated/ShellKind";
import type { AliasWins } from "./generated/AliasWins";
import type { PsHost as PsHostWire } from "./generated/PsHost";
import type { PsExecPolicy } from "./generated/PsExecPolicy";
import type { AliasStartupFile } from "./generated/AliasStartupFile";
import type { AliasListing as AliasListingWire } from "./generated/AliasListing";
import type { PolicySet as PolicySetWire } from "./generated/PolicySet";
import type { OnPath } from "./generated/OnPath";
import type { UserPath } from "./generated/UserPath";
import type { UserPathOp } from "./generated/UserPathOp";

/** 方言：别名文件、别名块、source 那一行问的是同一个问题。 */
export type Shell = ShellKind;

/** 同名两条里新开的终端敲它起的是哪一个：你写的 · 清单那条 · 说不清（后端比行号，界面按码取句）。 */
export type ClashWins = AliasWins;

/** 哪一代 PowerShell。 */
export type PsHost = PsHostWire;

/** 加载这份 `$PROFILE` 的那一代 PowerShell 的执行策略（那台后端现问）。 */
export type ExecPolicy = PsExecPolicy;

/** 一份候选启动文件（rc / `$PROFILE`）的状态。 */
export type StartupFile = AliasStartupFile;

/** `powershell-policy-set` 的成品：设完现问的那一份 ＋ 设的那一下没成的原因词（没报 ⇒ `null`）。 */
export type PolicySet = PolicySetWire;

/** 读回口的成品：启动文件候选。 */
export type AliasListing = AliasListingWire;

/** 这几问的期限：读几份小文件 / 写一份 / 起一次 PowerShell 问内建别名（秒级）。给 30 秒。 */
const ALIAS_BUDGET_MS = 30_000;

/** 读回那台上的启动文件候选；`rcPath` = 人另指的那一份（那台后端过围栏）。 */
export function readAliases(origin: Origin, shell: Shell, rcPath: string | null): Promise<AliasListing> {
  return callTyped(origin, "aliases-read", { shell, rcPath }, budgetWithin(ALIAS_BUDGET_MS));
}

/** 那台终端里敲 `ccm` 走到的是不是 cc-monitor 放的那一份、给人读的那一句（那台后端现问新开终端的 `PATH`）。 */
export async function readCcmOnPath(origin: Origin): Promise<OnPath> {
  const { onPath } = await callTyped(origin, "ccm-probe", {}, budgetWithin(ALIAS_BUDGET_MS));
  return onPath;
}

/** 用户级 PATH 那一格：读 · 加 · 撤，回做完之后现问的成品（状态 · 那一句 · 详情 · 那段命令）。 */
export function userPath(origin: Origin, op: UserPathOp): Promise<UserPath> {
  return callTyped(origin, "ccm-user-path", { op }, budgetWithin(ALIAS_BUDGET_MS));
}

/** 别名块预览（往一份空文件里装一次会写成什么；方言由 `rcPath` 的扩展名定）。 */
export async function renderAliasBlock(origin: Origin, rcPath: string): Promise<string> {
  const { text } = await callTyped(origin, "aliases-block-render", { rcPath }, budgetWithin(ALIAS_BUDGET_MS));
  return text;
}

/** 别名块装进那台上人选的那份启动文件（幂等，整块替换）。 */
export async function installAliasBlock(origin: Origin, rcPath: string): Promise<void> {
  await callTyped(origin, "aliases-block-install", { rcPath }, budgetWithin(ALIAS_BUDGET_MS));
}

/**
 * 把那一代 PowerShell 的执行策略设成当前用户 `RemoteSigned`（后端只做这一件固定的事）。
 * **只在用户点了、确认了之后调**（不代改）。
 */
export function allowLocalScripts(origin: Origin, host: PsHost): Promise<PolicySet> {
  return callTyped(origin, "powershell-policy-set", { host }, budgetWithin(ALIAS_BUDGET_MS));
}

/** 别名块卸掉（整块删，块外一个字节不动）。 */
export async function removeAliasBlock(origin: Origin, rcPath: string): Promise<void> {
  await callTyped(origin, "aliases-block-remove", { rcPath }, budgetWithin(ALIAS_BUDGET_MS));
}
