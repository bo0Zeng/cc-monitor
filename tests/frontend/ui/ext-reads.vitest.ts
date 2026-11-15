/**
 * 要求：扩展页的几问全走通道、判定全在后端 —— 界面按形状严格收（多一格 / 缺一格 / 类型不对 ⇒ 抛「两端版本对不上」），
 * 线上形状由跨语言金样钉住（后端测试产出 == `ext-flow.golden.json`，这里读同一份）；「装」只问本机后端（它当枢纽），「卸」问被卸的那一台。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { syncAssets } from "../../../src/frontend/ui/assets-sync-reads";
import type { AssetsSynced } from "../../../src/frontend/ui/generated/AssetsSynced";
import {
  extHubApply,
  extHubPreview,
  extList,
  extNoteSet,
  extUninstallApply,
  extUninstallPreview,
} from "../../../src/frontend/ui/ext-reads";
import type { ExtDone } from "../../../src/frontend/ui/generated/ExtDone";
import type { ExtList } from "../../../src/frontend/ui/generated/ExtList";
import type { McpReadReply } from "../../../src/frontend/ui/generated/McpReadReply";
import type { ExtUninstallCard } from "../../../src/frontend/ui/generated/ExtUninstallCard";
import { REPO_ROOT } from "../../test-support/repo-root";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { chanArgsJson, chanReply, type ChanCallArgs } from "../../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const golden = (n: string) =>
  JSON.parse(readFileSync(resolve(REPO_ROOT, `tests/__fixtures__/${n}.golden.json`), "utf8")) as Record<string, unknown>;
const SYNC = golden("assets-sync");
const EXT = golden("ext-flow");

beforeEach(() => {
  invokeMock.mockReset();
});

describe("金样：后端出的成品，TS 读得懂", () => {
  it("assets-sync · ext-list · ext-hub-preview / -apply · ext-uninstall-preview / -apply", () => {
    expect((SYNC.reply as AssetsSynced).reach).toEqual([{ origin: "o", machine: null }]);
    const list = EXT.list as ExtList;
    expect(list.machines.map((m) => [m.key, m.here])).toEqual([
      [null, true],
      ["laptop", false],
    ]);
    expect(list.rows.map((r) => [r.name, r.cells.map((c) => c.state)])).toEqual([
      ["planned-build", ["missing", "missing"]],
      ["code-picture", ["missing", "missing"]],
      ["cc-bus", ["missing", "missing"]],
      ["demo", ["same", "missing"]],
    ]);
    expect(list.rows.map((r) => r.known?.hooks ?? null)).toEqual([false, false, true, null]);
    const there = list.rows[3].cells[1];
    expect(there.bring?.targets.map((t) => [t.at.level, t.ok])).toEqual([
      ["user", true],
      ["project", true],
    ]);
    expect(list.rows[3].cells[0].places.map((p) => [p.at.level, p.state, p.uninstall])).toEqual([["user", "same", true]]);
    expect((EXT.done as ExtDone).changed).toEqual(["SKILL.md"]);
    expect((EXT.uninstallCard as ExtUninstallCard).recorded).toBe(true);
    expect((EXT.uninstallDone as ExtDone).changed).toEqual(["SKILL.md"]);
  });
});

// ★ 源码那一侧：「装」只问本机（枢纽），不经前端中继 —— 人群 = `ext-reads.ts` 里每一处 `chan.call(` / `callTyped(`；内层命令一条都不许界面直问。
describe("装只问本机（枢纽）；内层命令界面一条都不直问", () => {
  it("ext-reads.ts：枢纽那两条的第一参恰是 LOCAL，内层命令零出现", () => {
    const src = readFileSync(resolve(REPO_ROOT, "src/frontend/ui/ext-reads.ts"), "utf8");
    const calls = [...src.matchAll(/(?:chan\.call|callTyped)\(\s*([^,]+),\s*"([^"]+)"/g)].map((m) => [m[1].trim(), m[2]]);
    expect(calls.map(([, op]) => op).sort()).toEqual(["ext-hub-apply", "ext-hub-preview", "ext-list", "ext-note-set", "ext-uninstall-apply", "ext-uninstall-preview"]);
    expect(calls.filter(([, op]) => op.startsWith("ext-hub-") || op === "ext-list" || op === "ext-note-set").every(([o]) => o === "LOCAL")).toBe(true);
    const inner = ["mcp-read", "mcp-sync-source", "mcp-sync-preview", "mcp-sync-apply", "skill-read", "skill-install-plan", "skill-install-apply", "cc-bus-install", "cc-bus-install-state"];
    expect(inner.filter((op) => src.includes(`"${op}"`))).toEqual([]);
  });
});

describe("请求：问对那台、说对那条", () => {
  it("同步：远端那一页只报 origin、问的是本机后端；本机那一页什么都不报", async () => {
    invokeMock.mockResolvedValue(chanReply(SYNC.reply));
    await syncAssets("devbox");
    await syncAssets("<local>");
    const calls = invokeMock.mock.calls.map((c) => c[1] as ChanCallArgs);
    expect(calls.map((a) => [a.origin, a.op, chanArgsJson(a)])).toEqual([
      ["<local>", "assets-sync", { origin: "devbox" }],
      ["<local>", "assets-sync", {}],
    ]);
  });
  it("表 · 装（一张卡 · 装到几台）都问本机；卸问被卸那台；各台卡上的记号、那一处与只填一次的值原样交回", async () => {
    invokeMock.mockResolvedValueOnce(chanReply(EXT.list));
    await extList(true);
    const many = { places: [{ at: { level: "user" }, ok: true, note: null }], place: { level: "user" }, slots: [], machines: [{ to: "laptop", name: "laptop", card: EXT.card, files: ["/g · SKILL.md"], error: null }] };
    invokeMock.mockResolvedValueOnce(chanReply(many));
    const card = await extHubPreview({ kind: "skill", name: "demo" }, ["laptop"], null);
    invokeMock.mockResolvedValueOnce(chanReply({ machines: [{ to: "laptop", name: "laptop", done: EXT.done, error: null }] }));
    const done = await extHubApply({ kind: "skill", name: "demo" }, card, { env: { K: "v" } });
    expect(done.machines[0].done).toEqual(EXT.done);
    invokeMock.mockResolvedValueOnce(chanReply(EXT.uninstallCard));
    const u = await extUninstallPreview("laptop", { kind: "skill", name: "demo" }, { level: "user" });
    invokeMock.mockResolvedValueOnce(chanReply(EXT.uninstallDone));
    await extUninstallApply("laptop", { kind: "skill", name: "demo" }, u, { level: "user" });
    const calls = invokeMock.mock.calls.map((c) => c[1] as ChanCallArgs);
    expect(calls.map((a) => [a.origin, a.op, chanArgsJson(a)])).toEqual([
      ["<local>", "ext-list", { visit: true }],
      ["<local>", "ext-hub-preview", { kind: "skill", name: "demo", to: ["laptop"], place: null }],
      ["<local>", "ext-hub-apply", { kind: "skill", name: "demo", to: ["laptop"], place: { level: "user" }, tokens: { laptop: card.machines[0].card!.tokens }, fill: { env: { K: "v" } } }],
      ["laptop", "ext-uninstall-preview", { kind: "skill", name: "demo", at: { level: "user" } }],
      ["laptop", "ext-uninstall-apply", { kind: "skill", name: "demo", at: { level: "user" }, token: u.token }],
    ]);
  });
  it("行上带家 ⇒ 装 · 卸那几问照行上的家带 `agent`；默认那一家那一行不带（帧与今天逐字相同）", async () => {
    const many = { places: [], place: { level: "user" }, slots: [], machines: [{ to: "laptop", name: "laptop", card: EXT.card, files: [], error: null }] };
    const row = { kind: "skill" as const, name: "demo", agent: "fake2" };
    invokeMock.mockResolvedValueOnce(chanReply(many));
    const card = await extHubPreview(row, ["laptop"], null);
    invokeMock.mockResolvedValueOnce(chanReply({ machines: [] }));
    await extHubApply(row, card, {});
    invokeMock.mockResolvedValueOnce(chanReply({ ...(EXT.uninstallCard as object), agent: "fake2" }));
    const u = await extUninstallPreview("laptop", row, { level: "user" });
    invokeMock.mockResolvedValueOnce(chanReply(EXT.uninstallDone));
    await extUninstallApply("laptop", row, u, { level: "user" });
    const agents = invokeMock.mock.calls.map((c) => (chanArgsJson(c[1] as ChanCallArgs) as { agent?: string }).agent);
    expect(agents).toEqual(["fake2", "fake2", "fake2", "fake2"]);
  });
  it("行上带来源（同名插件几个来源各一行）⇒ 装 · 卸那几问都带那一行的来源键；没有来源的行不带", async () => {
    const many = { places: [], place: { level: "user" }, slots: [], machines: [{ to: "laptop", name: "laptop", card: EXT.card, files: [], error: null }] };
    const row = { kind: "plugin" as const, name: "demo", source: { key: "demo@rack", said: "x" } };
    invokeMock.mockResolvedValueOnce(chanReply(many));
    const card = await extHubPreview(row, ["laptop"], null);
    invokeMock.mockResolvedValueOnce(chanReply({ machines: [] }));
    await extHubApply(row, card, {});
    invokeMock.mockResolvedValueOnce(chanReply(EXT.uninstallCard));
    const u = await extUninstallPreview("laptop", row, { level: "user" });
    invokeMock.mockResolvedValueOnce(chanReply(EXT.uninstallDone));
    await extUninstallApply("laptop", row, u, { level: "user" });
    invokeMock.mockResolvedValueOnce(chanReply(EXT.uninstallCard));
    await extUninstallPreview("laptop", { kind: "skill", name: "demo" }, { level: "user" });
    const sources = invokeMock.mock.calls.map((c) => (chanArgsJson(c[1] as ChanCallArgs) as { source?: string }).source);
    expect(sources).toEqual(["demo@rack", "demo@rack", "demo@rack", "demo@rack", undefined]);
  });
  it("备注：问本机后端，种类 · 名字 · 正文原样交，行上带家才带 agent；回现在生效的那一份", async () => {
    invokeMock.mockResolvedValueOnce(chanReply({ note: "先加钩子" }));
    expect(await extNoteSet({ kind: "skill", name: "cc-bus" }, "先加钩子")).toBe("先加钩子");
    invokeMock.mockResolvedValueOnce(chanReply({ note: null }));
    expect(await extNoteSet({ kind: "skill", name: "cc-bus" }, "")).toBeNull();
    invokeMock.mockResolvedValueOnce(chanReply({ note: "第二家的" }));
    expect(await extNoteSet({ kind: "skill", name: "demo", agent: "fake2" }, "第二家的")).toBe("第二家的");
    const calls = invokeMock.mock.calls.map((c) => c[1] as ChanCallArgs);
    expect(calls.map((a) => [a.origin, a.op, chanArgsJson(a)])).toEqual([
      ["<local>", "ext-note-set", { kind: "skill", name: "cc-bus", text: "先加钩子" }],
      ["<local>", "ext-note-set", { kind: "skill", name: "cc-bus", text: "" }],
      ["<local>", "ext-note-set", { kind: "skill", name: "demo", agent: "fake2", text: "第二家的" }],
    ]);
    expect(Object.keys(chanArgsJson(calls[0]) as object)).not.toContain("agent");
  });
});

// 要求：MCP 状态那一块 —— 小标与抽屉那一行的字都是那台后端 `mcp-read` 写好的，界面照抄（回包形状由握手指纹核过）。
describe("mcp-read：每台问它自己的后端，写好的字照收", () => {
  const MCP = golden("mcp-read");
  it("金样两份（不带字的 · 带需登录 / 连不上那几格的）都读得懂；字原样带出", () => {
    expect((MCP.reply as McpReadReply).entries.map((e) => [e.name, e.mark])).toEqual([
      ["u1", null],
      ["l1", null],
      ["p1", null],
    ]);
    const m = (MCP.marked as McpReadReply).entries;
    expect(m.map((e) => e.mark)).toEqual(["needsLogin", "failed", "failed", null, null]);
    const fix = (account: string) => ({ account, fix: { command: "login-start", args: { kind: "mcp", agent: "claude", account, server: "s-login", projectDir: null } } });
    expect(m[0].login).toEqual({ said: copyText("beMcp.mark.login", { accounts: "acct-a、acct-b", time: "10:42" }), tip: copyText("beMcp.mark.loginTip", { accounts: "acct-a、acct-b" }), fix: [fix("acct-a"), fix("acct-b")] });
    expect(m[1].failed).toEqual({ said: copyText("beMcp.mark.failed", { title: "会话-甲", time: "10:31" }), tip: copyText("beServer.words.cantConnect"), detail: "err-1" });
  });
});
