/**
 * 〔MIG-3a · `设计/99 §2.1 ⑬`〕MCP 读写**走通道，那台后端出成品**：
 *
 * | 做什么 | 帧命令 | 成品 |
 * |---|---|---|
 * | 读三段（user / local / project）＋ 用过的项目目录 | `mcp-read` | `{entries, dirs, problems}` |
 * | 增 / 改项目 `.mcp.json` 一条 | `mcp-server-put` | `{path, changed}` |
 * | 删一条 | `mcp-server-remove` | `{path, changed}` |
 *
 * 从前这几条是 monitor 的 Tauri 命令（`read_mcp_servers` 等七条，D 组「monitor 算好、后端写」）；计算与写都进了那台后端
 * （`src/backend/assets/mcp_edit.rs`），这里只按形状严格收（金样 `tests/__fixtures__/mcp-read.golden.json` · `mcp-edit.golden.json`）。
 */
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson, saidOf } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import { copyText } from "./copy-table";

/** 一条 MCP server 展示项（`server` 原样，未知字段不丢）。 */
export interface McpServerEntry {
  scope: "user" | "local" | "project";
  name: string;
  server: unknown;
  sourcePath: string;
}

/** `mcp-read` 的成品。`problems` = 读不出来的那几份（宽容读：当空段，但说出来）。 */
export interface McpRead {
  entries: McpServerEntry[];
  dirs: string[];
  problems: string[];
}

/** 一次写的回执：落点 · 真写了没有（算出来与盘上相同 / 删一条不在的 ⇒ `false`）。 */
export interface McpEdited {
  path: string;
  changed: boolean;
}

const isObj = (v: unknown): v is Record<string, unknown> =>
  v !== null && typeof v === "object" && !Array.isArray(v);
const sameKeys = (o: Record<string, unknown>, want: readonly string[]): boolean => {
  const got = Object.keys(o).sort();
  const w = [...want].sort();
  return got.length === w.length && got.every((k, i) => k === w[i]);
};
const strings = (v: unknown): v is string[] =>
  Array.isArray(v) && v.every((x) => typeof x === "string");
const SCOPES = ["user", "local", "project"] as const;

function entryOf(v: unknown): McpServerEntry {
  if (
    !isObj(v) ||
    !sameKeys(v, ["scope", "name", "server", "sourcePath"]) ||
    !SCOPES.includes(v.scope as (typeof SCOPES)[number]) ||
    typeof v.name !== "string" ||
    typeof v.sourcePath !== "string"
  ) {
    throw new Error(copyText("mcpReads.read.badShape"));
  }
  return {
    scope: v.scope as McpServerEntry["scope"],
    name: v.name,
    server: v.server,
    sourcePath: v.sourcePath,
  };
}

/** `mcp-read` 的成品 ⇒ 三段 ＋ 目录。严格收。 */
export function decodeMcpRead(v: unknown): McpRead {
  if (
    !isObj(v) ||
    !sameKeys(v, ["entries", "dirs", "problems"]) ||
    !Array.isArray(v.entries) ||
    !strings(v.dirs) ||
    !strings(v.problems)
  ) {
    throw new Error(copyText("mcpReads.read.badShape"));
  }
  return { entries: v.entries.map(entryOf), dirs: v.dirs, problems: v.problems };
}

/** `mcp-server-put` / `mcp-server-remove` 的成品 ⇒ 回执。严格收。 */
export function decodeMcpEdited(v: unknown): McpEdited {
  if (
    !isObj(v) ||
    !sameKeys(v, ["path", "changed"]) ||
    typeof v.path !== "string" ||
    typeof v.changed !== "boolean"
  ) {
    throw new Error(copyText("mcpReads.edit.badShape"));
  }
  return { path: v.path, changed: v.changed };
}

/** 读一份 `.claude.json`（重度用户可数 MB）＋ 一份 `.mcp.json`，给足 30 秒。 */
const MCP_BUDGET_MS = 30_000;

const OLD_BACKEND = (): string => copyText("mcpReads.backend.tooOld");

/** 读那台机器的 MCP：不给 `projectDir` ⇒ 只有 user 段（local / project 两段要项目目录）。 */
export async function readMcp(origin: Origin, projectDir: string | null): Promise<McpRead> {
  try {
    const body = jsonBody(projectDir ? { projectDir } : {});
    const budget = budgetWithin(MCP_BUDGET_MS);
    const reply = await chan.call(origin, "mcp-read", body, budget);
    return decodeMcpRead(readJson(reply));
  } catch (e) {
    throw new Error(saidOf(e, OLD_BACKEND()));
  }
}

/** 增 / 改那台机器上 `<projectDir>/.mcp.json` 里的一条。 */
export async function putMcpServer(
  origin: Origin,
  projectDir: string,
  name: string,
  server: unknown,
): Promise<McpEdited> {
  try {
    const body = jsonBody({ projectDir, name, server });
    const budget = budgetWithin(MCP_BUDGET_MS);
    const reply = await chan.call(origin, "mcp-server-put", body, budget);
    return decodeMcpEdited(readJson(reply));
  } catch (e) {
    throw new Error(saidOf(e, OLD_BACKEND()));
  }
}

/** 删那台机器上 `<projectDir>/.mcp.json` 里的一条（不在 ⇒ 一个字节不写）。 */
export async function removeMcpServer(
  origin: Origin,
  projectDir: string,
  name: string,
): Promise<McpEdited> {
  try {
    const body = jsonBody({ projectDir, name });
    const budget = budgetWithin(MCP_BUDGET_MS);
    const reply = await chan.call(origin, "mcp-server-remove", body, budget);
    return decodeMcpEdited(readJson(reply));
  } catch (e) {
    throw new Error(saidOf(e, OLD_BACKEND()));
  }
}
