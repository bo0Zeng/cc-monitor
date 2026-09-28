/**
 * 设计/05 §14.3：「成品的两侧对拍：界面按形状严格收（多一格 / 缺一格 / 类型不对 ⇒ 抛「两端契约对不上」，不猜）；线上形状由一份跨语言金样钉住」。
 *
 * 〔MIG-3a〕MCP 读写（`mcp-read` · `mcp-server-put` / `-remove`）与推 / 拉三问（`mcp-sync-source` / `-preview` / `-apply`）
 * 改走通道：TS 解码器读后端测试写下的同一份金样（异源：Rust 对临时目录现算 == 金样；这里解同一份）；请求说对 op、问对那台。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { decodeMcpEdited, decodeMcpRead, putMcpServer, readMcp } from "../src/mcp-reads";
import {
  decodeMcpSyncApplied,
  decodeMcpSyncPreview,
  decodeMcpSyncSource,
  mcpSyncApply,
  mcpSyncPreview,
} from "../src/mcp-sync-reads";
import { REPO_ROOT } from "./test-support/repo-root";
import { chanArgsJson, chanReply, type ChanCallArgs } from "./test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const golden = (n: string) =>
  JSON.parse(readFileSync(resolve(REPO_ROOT, `tests/__fixtures__/${n}.golden.json`), "utf8")) as Record<string, unknown>;
const READ = golden("mcp-read");
const EDIT = golden("mcp-edit");
const SYNC = golden("mcp-sync-flow");

beforeEach(() => {
  invokeMock.mockReset();
});

describe("金样：后端出的成品，TS 读得懂", () => {
  it("mcp-read · mcp-server-put / -remove · 推拉三问", () => {
    expect(decodeMcpRead(READ.reply).entries.map((e) => e.scope)).toEqual(["user", "local", "project"]);
    expect(decodeMcpEdited(EDIT.putReply)).toEqual({ path: "<DIR>/.mcp.json", changed: true });
    expect(decodeMcpEdited(EDIT.removeAbsentReply).changed).toBe(false);
    expect(decodeMcpSyncSource(SYNC.sourceReply).path).toBe("<SRC>/.mcp.json");
    expect(decodeMcpSyncPreview(SYNC.previewReply).rows.map((r) => r.state)).toEqual(["new", "differs", "only-there"]);
    expect(decodeMcpSyncApplied(SYNC.applyReply).names).toEqual(["a", "b"]);
  });
});

describe("严格收：形状不对 ⇒ 抛「两端版本对不上」", () => {
  const r = READ.reply as Record<string, unknown>;
  const p = SYNC.previewReply as Record<string, unknown>;
  it.each([
    ["mcp-read 多一格", () => decodeMcpRead({ ...r, x: 1 })],
    ["mcp-read 认不出的 scope", () => decodeMcpRead({ ...r, entries: [{ scope: "weird", name: "w", server: {}, sourcePath: "" }] })],
    ["put 回执缺一格", () => decodeMcpEdited({ path: "/p" })],
    ["preview 行多一格", () => decodeMcpSyncPreview({ ...p, rows: [{ ...(p.rows as object[])[0], extra: 1 }] })],
    ["apply 回执类型不对", () => decodeMcpSyncApplied({ path: "/p", written: "yes", names: [] })],
  ])("%s", (_n, f) => {
    expect(f).toThrow(/两端版本对不上/);
  });
});

describe("请求：问对那台、说对那条", () => {
  it("读 / 写：origin 原样、op 对、请求体恰是那几格", async () => {
    invokeMock.mockResolvedValueOnce(chanReply(READ.reply));
    await readMcp("aya", "/p");
    invokeMock.mockResolvedValueOnce(chanReply(EDIT.putReply));
    await putMcpServer("<local>", "/p", "s", { command: "x" });
    const calls = invokeMock.mock.calls.map((c) => c[1] as ChanCallArgs);
    expect(calls.map((a) => [a.origin, a.op, chanArgsJson(a)])).toEqual([
      ["aya", "mcp-read", { projectDir: "/p" }],
      ["<local>", "mcp-server-put", { projectDir: "/p", name: "s", server: { command: "x" } }],
    ]);
  });
  it("推拉：来源那台交原文、被写那台看差异（原文原样递过去）· 写只问被写那台", async () => {
    invokeMock.mockResolvedValueOnce(chanReply(SYNC.sourceReply));
    invokeMock.mockResolvedValueOnce(chanReply(SYNC.previewReply));
    await mcpSyncPreview({ from: "<local>", fromDir: "/a", to: "aya", toDir: "/b" });
    invokeMock.mockResolvedValueOnce(chanReply(SYNC.applyReply));
    await mcpSyncApply({ to: "aya", toDir: "/b", sourceText: "S", targetText: null, take: ["a"], overwrite: [] });
    const calls = invokeMock.mock.calls.map((c) => c[1] as ChanCallArgs);
    const src = SYNC.sourceReply as { path: string; text: string };
    expect(calls.map((a) => [a.origin, a.op, chanArgsJson(a)])).toEqual([
      ["<local>", "mcp-sync-source", { projectDir: "/a" }],
      ["aya", "mcp-sync-preview", { projectDir: "/b", source: src.text, sourcePath: src.path, sameMachine: false }],
      ["aya", "mcp-sync-apply", { projectDir: "/b", source: "S", target: null, take: ["a"], overwrite: [] }],
    ]);
  });
});
