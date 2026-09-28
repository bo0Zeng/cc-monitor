/**
 * 〔MIG-3a · `设计/99 §2.1 ⑬`〕MCP 推 / 拉**走通道**，每一问只问一台：
 *
 * | 一步 | 问哪台 | 帧命令 | 成品 |
 * |---|---|---|---|
 * | 拿来源那份原文 | `from` | `mcp-sync-source` | `{path, text}` |
 * | 看差异（读自己那份 ＋ 判差异与可疑项） | `to` | `mcp-sync-preview` | `{sourcePath, targetPath, sourceText, targetText, rows}` |
 * | 写（CAS 期望 = 看差异时那份） | `to` | `mcp-sync-apply` | `{path, written, names}` |
 *
 * 从前是 monitor 的两条 Tauri 命令（`mcp_sync_preview` / `mcp_sync_apply`）编排这几跳；判定一直在被写那台后端
 * （`mcp-sync-plan`），今天连读写都在它（`src/backend/assets/mcp_sync_flow.rs`）。这里零判定：把来源那份原文原样递给被写那台、
 * 按形状严格收（金样 `tests/__fixtures__/mcp-sync-flow.golden.json`）。
 */
import { chan } from "./ipc/chan";
import { budgetWithin, jsonBody, readJson, saidOf } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import { copyText } from "./copy-table";

/** 一条可疑项（被写那台后端判的，原样上屏）。 */
export interface McpSyncSuspect {
  kind: string;
  field: string;
  value: string;
  there: string | null;
}

/** 差异表的一行。`source` / `target` 是两边那一条的配置原样（没有 ⇒ `null`）。 */
export interface McpSyncRow {
  name: string;
  state: string;
  suspects: McpSyncSuspect[];
  source: unknown;
  target: unknown;
}

/** 看差异的结果。两份原文原样带着，写的时候原样送回（后者当 CAS 期望）。 */
export interface McpSyncPreview {
  sourcePath: string;
  targetPath: string;
  sourceText: string;
  targetText: string | null;
  rows: McpSyncRow[];
}

/** 写的结果。 */
export interface McpSyncApplied {
  path: string;
  written: boolean;
  names: string[];
}

const isObj = (v: unknown): v is Record<string, unknown> =>
  v !== null && typeof v === "object" && !Array.isArray(v);
const sameKeys = (o: Record<string, unknown>, want: readonly string[]): boolean => {
  const got = Object.keys(o).sort();
  const w = [...want].sort();
  return got.length === w.length && got.every((k, i) => k === w[i]);
};
const bad = (): Error => new Error(copyText("mcpSyncReads.reply.badShape"));

function suspectOf(v: unknown): McpSyncSuspect {
  if (
    !isObj(v) ||
    !sameKeys(v, ["kind", "field", "value", "there"]) ||
    typeof v.kind !== "string" ||
    typeof v.field !== "string" ||
    typeof v.value !== "string" ||
    !(v.there === null || typeof v.there === "string")
  )
    throw bad();
  return { kind: v.kind, field: v.field, value: v.value, there: v.there };
}

function rowOf(v: unknown): McpSyncRow {
  if (
    !isObj(v) ||
    !sameKeys(v, ["name", "state", "suspects", "source", "target"]) ||
    typeof v.name !== "string" ||
    typeof v.state !== "string" ||
    !Array.isArray(v.suspects)
  )
    throw bad();
  return {
    name: v.name,
    state: v.state,
    suspects: v.suspects.map(suspectOf),
    source: v.source,
    target: v.target,
  };
}

/** `mcp-sync-source` 的成品。严格收。 */
export function decodeMcpSyncSource(v: unknown): { path: string; text: string } {
  if (!isObj(v) || !sameKeys(v, ["path", "text"]) || typeof v.path !== "string" || typeof v.text !== "string")
    throw bad();
  return { path: v.path, text: v.text };
}

/** `mcp-sync-preview` 的成品。严格收。 */
export function decodeMcpSyncPreview(v: unknown): McpSyncPreview {
  if (
    !isObj(v) ||
    !sameKeys(v, ["sourcePath", "targetPath", "sourceText", "targetText", "rows"]) ||
    typeof v.sourcePath !== "string" ||
    typeof v.targetPath !== "string" ||
    typeof v.sourceText !== "string" ||
    !(v.targetText === null || typeof v.targetText === "string") ||
    !Array.isArray(v.rows)
  )
    throw bad();
  return {
    sourcePath: v.sourcePath,
    targetPath: v.targetPath,
    sourceText: v.sourceText,
    targetText: v.targetText,
    rows: v.rows.map(rowOf),
  };
}

/** `mcp-sync-apply` 的成品。严格收。 */
export function decodeMcpSyncApplied(v: unknown): McpSyncApplied {
  if (
    !isObj(v) ||
    !sameKeys(v, ["path", "written", "names"]) ||
    typeof v.path !== "string" ||
    typeof v.written !== "boolean" ||
    !Array.isArray(v.names) ||
    !v.names.every((n) => typeof n === "string")
  )
    throw bad();
  return { path: v.path, written: v.written, names: v.names as string[] };
}

/** 一问的期限：读一份 `.mcp.json`、对可疑路径逐条 `stat`，秒级内；给 30 秒。 */
const SYNC_BUDGET_MS = 30_000;
const OLD_BACKEND = (): string => copyText("mcpReads.backend.tooOld");

/** 通道三层 / 拒绝码 → 一句人话（同 `mcp-reads.ts`）。 */
const said = (e: unknown): Error => new Error(saidOf(e, OLD_BACKEND()));

/** 看差异：`from` 那台 `fromDir` 的 `.mcp.json` 拷到 `to` 那台 `toDir` 会发生什么。推 / 拉同一问，界面换个方向说。 */
export async function mcpSyncPreview(a: {
  from: Origin;
  fromDir: string;
  to: Origin;
  toDir: string;
}): Promise<McpSyncPreview> {
  try {
    const body = jsonBody({ projectDir: a.fromDir });
    const budget = budgetWithin(SYNC_BUDGET_MS);
    const src = decodeMcpSyncSource(readJson(await chan.call(a.from, "mcp-sync-source", body, budget)));
    const ask = jsonBody({
      projectDir: a.toDir,
      source: src.text,
      sourcePath: src.path,
      sameMachine: a.from === a.to,
    });
    return decodeMcpSyncPreview(readJson(await chan.call(a.to, "mcp-sync-preview", ask, budget)));
  } catch (e) {
    throw said(e);
  }
}

/** 写：把勾的那几条原样合进 `to` 那台那份。`sourceText` / `targetText` 原样送回看差异时拿到的那两份。 */
export async function mcpSyncApply(a: {
  to: Origin;
  toDir: string;
  sourceText: string;
  targetText: string | null;
  take: string[];
  overwrite: string[];
}): Promise<McpSyncApplied> {
  try {
    const body = jsonBody({
      projectDir: a.toDir,
      source: a.sourceText,
      target: a.targetText,
      take: a.take,
      overwrite: a.overwrite,
    });
    const budget = budgetWithin(SYNC_BUDGET_MS);
    return decodeMcpSyncApplied(readJson(await chan.call(a.to, "mcp-sync-apply", body, budget)));
  } catch (e) {
    throw said(e);
  }
}
