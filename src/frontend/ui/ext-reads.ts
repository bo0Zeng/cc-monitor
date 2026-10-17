/**
 * 设置「扩展」页的几问，**全走通道**：
 *
 * | 做什么 | 问哪台 | 命令 |
 * |---|---|---|
 * | 那张表（条目 × 机器：每格的点、那台上的各处、「装到…」能装到哪几处） | 本机后端 | `ext-list` |
 * | 装到一台之前那张确认卡 / 装 | 本机后端（枢纽，向来源取、交被写那台写） | `ext-hub-preview` / `ext-hub-apply` |
 * | 从一台的某一处卸之前那张卡 / 卸 | 被卸的那一台 | `ext-uninstall-preview` / `ext-uninstall-apply` |
 * | 写 / 改 / 清一个条目的备注 | 本机后端（记进它的目录，随目录同步） | `ext-note-set` |
 * | MCP 那几行每台的小标（需登录 · 断开）与抽屉里那一行的字 | 那一台（判定与写字都在它的后端：`mcp-read`） | `mcp-read` |
 *
 * 判定全在后端。几问都是 typed（`callTyped`：入参 · 回包的类型从后端生成，回包形状由握手指纹核过；线上形状由金样
 * `tests/__fixtures__/ext-flow.golden.json` · `mcp-read.golden.json` 钉），不比较、不推断。
 */
import type { Budget } from "../../comms/inward/chan";
import { budgetWithin, callTyped, refusalIn } from "./ipc/chan-caller";
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
import type { ExtTokens } from "./generated/ExtTokens";
import type { HubApplied } from "./generated/HubApplied";
import type { HubPreview } from "./generated/HubPreview";
import type { McpReadEntry } from "./generated/McpReadEntry";
import type { McpReadReply } from "./generated/McpReadReply";
import type { ExtTarget } from "./generated/ExtTarget";
import type { ExtUninstallCard } from "./generated/ExtUninstallCard";
import { saidOf } from "./kit/detail";

export type { ExtBring, ExtBuiltin, ExtCard, ExtCell, ExtDone, ExtKind, ExtList, ExtLoc, ExtMachine, ExtPlace, ExtRow, ExtScope, ExtSlot, ExtTarget, ExtUninstallCard };

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
  const code = refusalIn(e)?.code ?? null;
  return new ExtRefused(saidOf(e), code);
}

/** 读一趟：表在本机后端，远端慢时同步那一趟另走（`assets-sync-reads.ts`）。 */
const LIST_BUDGET_MS = 30_000;
/** 装 / 卸：远端那一跳要拨号、读写几个文件，给 120 秒。 */
const WRITE_BUDGET_MS = 120_000;

/** 那张表。`visit` = 这一问算「来看了一次」（页面每次变可见时的第一问）。 */
export function extList(visit: boolean): Promise<ExtList> {
  return askTyped((budget) => callTyped(LOCAL_ORIGIN, "ext-list", { visit }, budget), LIST_BUDGET_MS);
}

/** 「装到 N 台」那一张卡（本机后端当枢纽，向各台问完并好）：落点交集 · 共用的那一处 · 并好的要填格 · 各台的卡（那台没拼成 ⇒ `card` 为 `null`、`error` 是那台说的那一句）。 */
export type ExtManyCard = HubPreview;

/** 各台各自结局。 */
export type ExtManyDone = HubApplied;

/** typed 那一对：入参 · 回包的类型从后端生成，形状由握手指纹核过；没成 ⇒ [`ExtRefused`]（带后端的码）。 */
async function askTyped<T>(go: (budget: Budget) => Promise<T>, ms: number): Promise<T> {
  try {
    return await go(budgetWithin(ms));
  } catch (e) {
    throw refused(e);
  }
}

/** 「装到 N 台」那一张卡：勾上的几台（`null` = 本机后端自己）＋ 用户选的那一处（没选 ⇒ 后端建议）。 */
export function extHubPreview(kind: ExtKind, name: string, to: (string | null)[], place: ExtLoc | null): Promise<ExtManyCard> {
  return askTyped((budget) => callTyped(LOCAL_ORIGIN, "ext-hub-preview", { kind, name, to, place }, budget), WRITE_BUDGET_MS);
}

/** 装到那几台：卡上那一处 ＋ 各台卡上的记号原样交回；`fill` 只填一次。 */
export function extHubApply(kind: ExtKind, name: string, card: ExtManyCard, fill: Record<string, Record<string, string>>): Promise<ExtManyDone> {
  const tokens: Record<string, ExtTokens> = {};
  for (const m of card.machines) if (m.card) tokens[m.to ?? ""] = m.card.tokens;
  const to = card.machines.map((m) => m.to);
  return askTyped((budget) => callTyped(LOCAL_ORIGIN, "ext-hub-apply", { kind, name, to, place: card.place, tokens, fill }, budget), WRITE_BUDGET_MS);
}

/** 卸之前那张卡（问被卸的那一台）。 */
export function extUninstallPreview(on: Origin, kind: ExtKind, name: string, at: ExtLoc): Promise<ExtUninstallCard> {
  return askTyped((budget) => callTyped(on, "ext-uninstall-preview", { kind, name, at }, budget), WRITE_BUDGET_MS);
}

/** 卸：卡上的记号原样交回。 */
export function extUninstallApply(on: Origin, card: ExtUninstallCard, at: ExtLoc): Promise<ExtDone> {
  const args = { kind: card.kind, name: card.name, at, token: card.token };
  return askTyped((budget) => callTyped(on, "ext-uninstall-apply", args, budget), WRITE_BUDGET_MS);
}

/** 写 / 改 / 清（空串）一个条目的备注（记进本机后端的目录，随目录同步到别的后端）：回现在生效的那一份（清掉了 ⇒ `null`）。 */
export async function extNoteSet(kind: ExtKind, name: string, text: string): Promise<string | null> {
  const got = await askTyped((budget) => callTyped(LOCAL_ORIGIN, "ext-note-set", { kind, name, text }, budget), LIST_BUDGET_MS);
  return got.note;
}

/** `mcp-read` 的成品（形状从后端类型生成；金样 `tests/__fixtures__/mcp-read.golden.json` 钉线上形）。`mark` = 小标画哪一种（后端定）。 */
export type McpRead = McpReadReply;
export type McpEntry = McpReadEntry;

/** 问那台它的 MCP 列表（全局那一段；每条带后端写好的需登录 / 断开那几格）。本机远端同一条命令。 */
export function mcpRead(on: Origin): Promise<McpRead> {
  return askTyped((budget) => callTyped(on, "mcp-read", {}, budget), LIST_BUDGET_MS);
}
