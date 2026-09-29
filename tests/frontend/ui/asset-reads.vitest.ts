/**
 * 设计/05 §14.3：「成品的两侧对拍：界面按形状严格收（多一格 / 缺一格 / 类型不对 ⇒ 抛「两端契约对不上」，不猜）；线上形状由一份跨语言金样钉住」。
 *
 * 〔MIG-3a〕资产目录同步（`assets-sync`）与 skill 装 / 卸（`skill-read` · `skill-install-plan` · `skill-install-apply` · `skill-uninstall-apply`）
 * 改走通道：解码器读后端测试对拍过的同一份金样；请求问对那台、说对那条。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { decodeAssetsSynced, syncAssets } from "../../../src/frontend/ui/assets-sync-reads";
import {
  decodeSkillInstalled,
  decodeSkillUninstalled,
  skillInstallApply,
  skillInstallPreview,
} from "../../../src/frontend/ui/skill-install-reads";
import { REPO_ROOT } from "../../test-support/repo-root";
import { chanArgsJson, chanReply, type ChanCallArgs } from "../../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const golden = (n: string) =>
  JSON.parse(readFileSync(resolve(REPO_ROOT, `tests/__fixtures__/${n}.golden.json`), "utf8")) as Record<string, unknown>;
const SYNC = golden("assets-sync");
const SKILL = golden("skill-flow");

beforeEach(() => {
  invokeMock.mockReset();
});

describe("金样：后端出的成品，TS 读得懂", () => {
  it("assets-sync · skill-install-apply · skill-uninstall-apply", () => {
    expect(decodeAssetsSynced(SYNC.reply).reach).toEqual([{ origin: "o", machine: null }]);
    expect(decodeSkillInstalled(SKILL.installReply).written).toEqual(["SKILL.md", "lib/a.txt"]);
    expect(decodeSkillUninstalled(SKILL.uninstallReply).deleted).toEqual(["SKILL.md"]);
  });
});

describe("严格收：形状不对 ⇒ 抛「两端版本对不上」", () => {
  const s = SYNC.reply as Record<string, unknown>;
  const i = SKILL.installReply as Record<string, unknown>;
  const u = SKILL.uninstallReply as Record<string, unknown>;
  it.each([
    ["assets-sync 多一格", () => decodeAssetsSynced({ ...s, x: 1 }), "assetsSyncReads.reply.badShape"],
    ["assets-sync 行里 pushed 不是数", () => decodeAssetsSynced({ ...s, synced: [{ ...(s.synced as object[])[0], pushed: "1" }] }), "assetsSyncReads.reply.badShape"],
    ["install 缺一格", () => decodeSkillInstalled((({ chmodFailed: _c, ...r }) => r)(i)), "skillInstallReads.reply.badShape"],
    ["uninstall 类型不对", () => decodeSkillUninstalled({ ...u, dirRemoved: "no" }), "skillInstallReads.reply.badShape"],
  ] as const)("%s", (_n, f, key) => {
    expect(f).toThrow(copyText(key));
  });
});

// 〔MIG-3a · `01 §3.5` · 主会话 09-28 裁〕★ 源码那一侧：两台之间那两件（MCP 推拉 · skill 装），界面**只问本机**、每一步恰好一问 ——
//   不许再长回「先问来源那台、再把原文递给被写那台」（经前端中继）。人群 = 两份读口里的每一处 `chan.call(`。
describe("两台之间那两件只问本机（枢纽），不经前端中继", () => {
  it("mcp-sync-reads.ts · skill-install-reads.ts：每处 chan.call 的第一参都是 LOCAL_ORIGIN，op 恰是枢纽那四条、各一次", () => {
    const calls: Array<[string, string]> = [];
    for (const f of ["src/frontend/ui/mcp-sync-reads.ts", "src/frontend/ui/skill-install-reads.ts"]) {
      const src = readFileSync(resolve(REPO_ROOT, f), "utf8");
      for (const m of src.matchAll(/chan\.call\(\s*([^,]+),\s*"([^"]+)"/g)) calls.push([m[1].trim(), m[2]]);
    }
    const hub = calls.filter(([, op]) => op.includes("-hub-"));
    expect(hub.map(([, op]) => op).sort()).toEqual([
      "mcp-sync-hub-apply",
      "mcp-sync-hub-preview",
      "skill-install-hub-apply",
      "skill-install-hub-preview",
    ]);
    expect(hub.every(([o]) => o === "LOCAL_ORIGIN"), JSON.stringify(hub)).toBe(true);
    // 来源那台 / 被写那台的内层命令一条都不许界面直问（它们只经枢纽）。
    const inner = ["mcp-sync-source", "mcp-sync-preview", "mcp-sync-apply", "skill-read", "skill-install-plan", "skill-install-apply"];
    expect(calls.filter(([, op]) => inner.includes(op))).toEqual([]);
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
  // 〔MIG-3a · `01 §3.5` · 主会话 09-28 裁〕★ 装**只问本机一次**：枢纽向来源那台读、交被写那台判 / 写 —— 界面一个字节的原文都不递过去。
  it("装：看差异 · 写 各恰好一问、都问本机（枢纽），本机那台在线上是 null", async () => {
    const files = [{ path: "SKILL.md", text: "t", exec: false, why: null }];
    invokeMock.mockResolvedValueOnce(chanReply({ dir: "/d", rows: [], target: [], source: files }));
    const p = await skillInstallPreview({ from: "<local>", to: "devbox", name: "demo" });
    invokeMock.mockResolvedValueOnce(chanReply(SKILL.installReply));
    await skillInstallApply({ from: "<local>", to: "devbox", name: "demo", source: p.source, target: p.target, take: ["SKILL.md"], overwrite: [] });
    const calls = invokeMock.mock.calls.map((c) => c[1] as ChanCallArgs);
    expect(calls.map((a) => [a.origin, a.op, chanArgsJson(a)])).toEqual([
      ["<local>", "skill-install-hub-preview", { from: null, to: "devbox", name: "demo" }],
      [
        "<local>",
        "skill-install-hub-apply",
        { from: null, to: "devbox", name: "demo", expectSource: files, target: [], take: ["SKILL.md"], overwrite: [] },
      ],
    ]);
  });
  it("看差异的成品多一格 ⇒ 抛", async () => {
    invokeMock.mockResolvedValueOnce(chanReply({ dir: "/d", rows: [], target: [], source: [], write: null }));
    await expect(skillInstallPreview({ from: "devbox", to: "<local>", name: "demo" })).rejects.toThrow(copyText("skillInstallReads.reply.badShape"));
  });
});
