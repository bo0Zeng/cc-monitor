/**
 * 〔MIG-3a · `设计/99 §2.1 ⑬` · `01 §3.5` · 主会话 09-28 裁〕MCP 推 / 拉**走通道、只问本机一次**，本机常驻后端当枢纽：
 *
 * | 一步 | 问哪台 | 帧命令 | 成品 |
 * |---|---|---|---|
 * | 看差异（枢纽向 `from` 取原文、交 `to` 判） | 本机 | `mcp-sync-hub-preview` | `{sourcePath, targetPath, sourceText, targetText, rows}` |
 * | 写（枢纽向 `from` 再取一次核对、交 `to` 写，CAS 期望 = 看差异时那份） | 本机 | `mcp-sync-hub-apply` | `{path, written, names}` |
 *
 * 前半那一形是界面先问来源那台拿原文、再递给被写那台 —— 经前端中继，撞 `01 §3.5`，主会话 09-28 裁改掉。
 * 判定与读写都在后端（`src/backend/assets/hub.rs` · `mcp_sync_flow.rs`）；这里零判定、按形状严格收（金样 `mcp-sync-flow.golden.json`）。
 * `from` / `to` 线上是可达表的键，**本机那台发 `null`**（后端不认 `<local>` 这个名字）。
 */
import { chan } from "./comms/inward/chan";
import { budgetWithin, jsonBody, readJson, saidOf } from "./ipc/chan-caller";
import { isLocalOrigin, LOCAL_ORIGIN, type Origin } from "./ipc/origin";
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

/** 枢纽线上的那一台：本机 ⇒ `null`（= 枢纽自己），远端 ⇒ 可达表的键。 */
export const hubMachine = (o: Origin): string | null => (isLocalOrigin(o) ? null : o);

/** 看差异：`from` 那台 `fromDir` 的 `.mcp.json` 拷到 `to` 那台 `toDir` 会发生什么。推 / 拉同一问，界面换个方向说。 */
export async function mcpSyncPreview(a: {
  from: Origin;
  fromDir: string;
  to: Origin;
  toDir: string;
}): Promise<McpSyncPreview> {
  try {
    const body = jsonBody({ from: hubMachine(a.from), fromDir: a.fromDir, to: hubMachine(a.to), toDir: a.toDir });
    const budget = budgetWithin(SYNC_BUDGET_MS);
    return decodeMcpSyncPreview(readJson(await chan.call(LOCAL_ORIGIN, "mcp-sync-hub-preview", body, budget)));
  } catch (e) {
    throw said(e);
  }
}

/** 写：把勾的那几条合进 `to` 那台那份。`sourceText` / `targetText` 是看差异时拿到的那两份，只当 CAS 期望送回（写的内容由枢纽自己取）。 */
export async function mcpSyncApply(a: {
  from: Origin;
  fromDir: string;
  to: Origin;
  toDir: string;
  sourceText: string;
  targetText: string | null;
  take: string[];
  overwrite: string[];
}): Promise<McpSyncApplied> {
  try {
    const body = jsonBody({
      from: hubMachine(a.from),
      fromDir: a.fromDir,
      to: hubMachine(a.to),
      toDir: a.toDir,
      expectSource: a.sourceText,
      target: a.targetText,
      take: a.take,
      overwrite: a.overwrite,
    });
    const budget = budgetWithin(SYNC_BUDGET_MS);
    return decodeMcpSyncApplied(readJson(await chan.call(LOCAL_ORIGIN, "mcp-sync-hub-apply", body, budget)));
  } catch (e) {
    throw said(e);
  }
}
