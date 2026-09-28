/**
 * 设计/05 §14.3：「成品的两侧对拍：界面按形状严格收（多一格 / 缺一格 / 类型不对 ⇒ 抛「两端契约对不上」，不猜）；线上形状由一份跨语言金样钉住」。
 *
 * 〔MIG-3a〕资产目录同步（`assets-sync`）与 skill 装 / 卸（`skill-read` · `skill-install-plan` · `skill-install-apply` · `skill-uninstall-apply`）
 * 改走通道：解码器读后端测试对拍过的同一份金样；请求问对那台、说对那条。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { decodeAssetsSynced, syncAssets } from "../src/assets-sync-reads";
import {
  decodeSkillInstalled,
  decodeSkillUninstalled,
  skillInstallApply,
  skillInstallPreview,
} from "../src/skill-install-reads";
import { REPO_ROOT } from "./test-support/repo-root";
import { chanArgsJson, chanReply, type ChanCallArgs } from "./test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const golden = (n: string) =>
  JSON.parse(readFileSync(resolve(REPO_ROOT, `tests/__fixtures__/${n}.golden.json`), "utf8")) as Record<string, unknown>;
const SYNC = golden("assets-sync");
const SKILL = golden("skill-flow");

beforeEach(() => invokeMock.mockReset());

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
    ["assets-sync 多一格", () => decodeAssetsSynced({ ...s, x: 1 })],
    ["assets-sync 行里 pushed 不是数", () => decodeAssetsSynced({ ...s, synced: [{ ...(s.synced as object[])[0], pushed: "1" }] })],
    ["install 缺一格", () => decodeSkillInstalled((({ chmodFailed: _c, ...r }) => r)(i))],
    ["uninstall 类型不对", () => decodeSkillUninstalled({ ...u, dirRemoved: "no" })],
  ])("%s", (_n, f) => {
    expect(f).toThrow(/两端版本对不上/);
  });
});

describe("请求：问对那台、说对那条", () => {
  it("同步：远端那一页只报 origin、问的是本机后端；本机那一页什么都不报", async () => {
    invokeMock.mockResolvedValue(chanReply(SYNC.reply));
    await syncAssets("aya");
    await syncAssets("<local>");
    const calls = invokeMock.mock.calls.map((c) => c[1] as ChanCallArgs);
    expect(calls.map((a) => [a.origin, a.op, chanArgsJson(a)])).toEqual([
      ["<local>", "assets-sync", { origin: "aya" }],
      ["<local>", "assets-sync", {}],
    ]);
  });
  it("装：来源那台读 → 被写那台判（原文原样递过去）→ 写只问被写那台", async () => {
    const files = [{ path: "SKILL.md", text: "t", exec: false, why: null, bytes: 1 }];
    invokeMock.mockResolvedValueOnce(chanReply({ files }));
    invokeMock.mockResolvedValueOnce(chanReply({ dir: "/d", rows: [], target: [], write: null }));
    const p = await skillInstallPreview({ from: "<local>", to: "aya", name: "demo" });
    invokeMock.mockResolvedValueOnce(chanReply(SKILL.installReply));
    await skillInstallApply({ to: "aya", name: "demo", source: p.source, target: p.target, take: ["SKILL.md"], overwrite: [] });
    const calls = invokeMock.mock.calls.map((c) => c[1] as ChanCallArgs);
    expect(calls.map((a) => [a.origin, a.op])).toEqual([
      ["<local>", "skill-read"],
      ["aya", "skill-install-plan"],
      ["aya", "skill-install-apply"],
    ]);
    expect(chanArgsJson(calls[1])).toEqual({ name: "demo", source: [{ path: "SKILL.md", text: "t", exec: false }] });
    expect(chanArgsJson(calls[2])).toEqual({
      name: "demo",
      source: [{ path: "SKILL.md", text: "t", exec: false, why: null }],
      target: [],
      take: ["SKILL.md"],
      overwrite: [],
    });
  });
});
