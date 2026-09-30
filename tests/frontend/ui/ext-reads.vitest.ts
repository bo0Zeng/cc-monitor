/**
 * 要求：扩展页的几问全走通道、判定全在后端 —— 界面按形状严格收（多一格 / 缺一格 / 类型不对 ⇒ 抛「两端版本对不上」），
 * 线上形状由跨语言金样钉住（后端测试产出 == `ext-flow.golden.json`，这里读同一份）；「装」只问本机后端（它当枢纽），「卸」问被卸的那一台。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { decodeAssetsSynced, syncAssets } from "../../../src/frontend/ui/assets-sync-reads";
import {
  ExtRefused,
  decodeExtCard,
  decodeExtDone,
  decodeExtList,
  decodeExtUninstallCard,
  extHubApply,
  extHubPreview,
  extList,
  extUninstallApply,
  extUninstallPreview,
} from "../../../src/frontend/ui/ext-reads";
import { REPO_ROOT } from "../../test-support/repo-root";
import { chanArgsJson, chanReply, refusedReply, type ChanCallArgs } from "../../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const golden = (n: string) =>
  JSON.parse(readFileSync(resolve(REPO_ROOT, `tests/__fixtures__/${n}.golden.json`), "utf8")) as Record<string, unknown>;
const SYNC = golden("assets-sync");
const EXT = golden("ext-flow");
const BAD = copyText("extReads.reply.badShape");

beforeEach(() => {
  invokeMock.mockReset();
});

describe("金样：后端出的成品，TS 读得懂", () => {
  it("assets-sync · ext-list · ext-hub-preview / -apply · ext-uninstall-preview / -apply", () => {
    expect(decodeAssetsSynced(SYNC.reply).reach).toEqual([{ origin: "o", machine: null }]);
    const list = decodeExtList(EXT.list);
    expect(list.machines.map((m) => [m.key, m.here])).toEqual([
      [null, true],
      ["gpd", false],
    ]);
    expect(list.rows.map((r) => [r.name, r.cells.map((c) => c.state)])).toEqual([["demo", ["same", "missing"]]]);
    expect(list.rows[0].cells[1].action?.verb).toBe("install");
    expect(decodeExtCard(EXT.card).writes).toEqual(["SKILL.md"]);
    expect(decodeExtDone(EXT.done).changed).toEqual(["SKILL.md"]);
    expect(decodeExtUninstallCard(EXT.uninstallCard).recorded).toBe(true);
    expect(decodeExtDone(EXT.uninstallDone).changed).toEqual(["SKILL.md"]);
  });
});

describe("严格收：形状不对 ⇒ 抛「两端版本对不上」", () => {
  const list = EXT.list as { machines: object[]; rows: Array<{ cells: object[] }> };
  const card = EXT.card as Record<string, unknown>;
  it.each([
    ["表多一格", () => decodeExtList({ ...list, x: 1 })],
    ["一行的格数与机器数对不上", () => decodeExtList({ ...list, rows: [{ ...list.rows[0], cells: list.rows[0].cells.slice(1) }] })],
    ["格态不在闭集里", () => decodeExtList({ ...list, rows: [{ ...list.rows[0], cells: [{ ...list.rows[0].cells[0], state: "maybe" }, list.rows[0].cells[1]] }] })],
    ["一格带了摘要", () => decodeExtList({ ...list, rows: [{ ...list.rows[0], cells: [{ ...list.rows[0].cells[0], digest: "d" }, list.rows[0].cells[1]] }] })],
    ["卡缺记号", () => decodeExtCard((({ tokens: _t, ...r }) => r)(card))],
    ["装完类型不对", () => decodeExtDone({ ...(EXT.done as object), changed: "SKILL.md" })],
  ] as const)("%s", (_n, f) => {
    expect(f).toThrow(BAD);
  });
});

// ★ 源码那一侧：「装」只问本机（枢纽），不经前端中继 —— 人群 = `ext-reads.ts` 里每一处 `chan.call(`；内层命令一条都不许界面直问。
describe("装只问本机（枢纽）；内层命令界面一条都不直问", () => {
  it("ext-reads.ts：枢纽那两条的第一参恰是 LOCAL_ORIGIN，内层命令零出现", () => {
    const src = readFileSync(resolve(REPO_ROOT, "src/frontend/ui/ext-reads.ts"), "utf8");
    const calls = [...src.matchAll(/chan\.call\(\s*([^,]+),\s*"([^"]+)"/g)].map((m) => [m[1].trim(), m[2]]);
    expect(calls.map(([, op]) => op).sort()).toEqual(["ext-hub-apply", "ext-hub-preview", "ext-list", "ext-uninstall-apply", "ext-uninstall-preview"]);
    expect(calls.filter(([, op]) => op.startsWith("ext-hub-") || op === "ext-list").every(([o]) => o === "LOCAL_ORIGIN")).toBe(true);
    const inner = ["mcp-sync-source", "mcp-sync-preview", "mcp-sync-apply", "skill-read", "skill-install-plan", "skill-install-apply"];
    expect(inner.filter((op) => src.includes(`"${op}"`))).toEqual([]);
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
  it("表 · 装（看卡 · 写）都问本机；卸问被卸那台；卡上的记号与填的值原样交回", async () => {
    invokeMock.mockResolvedValueOnce(chanReply(EXT.list));
    await extList(true);
    const bring = { kind: "skill" as const, name: "demo", from: null, to: "gpd", scope: { from: { level: "user" as const }, to: { level: "user" as const } } };
    invokeMock.mockResolvedValueOnce(chanReply(EXT.card));
    const card = await extHubPreview(bring);
    invokeMock.mockResolvedValueOnce(chanReply(EXT.done));
    await extHubApply(bring, card, { env: { K: "v" } });
    invokeMock.mockResolvedValueOnce(chanReply(EXT.uninstallCard));
    const u = await extUninstallPreview("gpd", "skill", "demo", { level: "user" });
    invokeMock.mockResolvedValueOnce(chanReply(EXT.uninstallDone));
    await extUninstallApply("gpd", u, { level: "user" });
    const calls = invokeMock.mock.calls.map((c) => c[1] as ChanCallArgs);
    expect(calls.map((a) => [a.origin, a.op, chanArgsJson(a)])).toEqual([
      ["<local>", "ext-list", { visit: true }],
      ["<local>", "ext-hub-preview", bring],
      ["<local>", "ext-hub-apply", { ...bring, tokens: card.tokens, fill: { env: { K: "v" } } }],
      ["gpd", "ext-uninstall-preview", { kind: "skill", name: "demo", at: { level: "user" } }],
      ["gpd", "ext-uninstall-apply", { kind: "skill", name: "demo", at: { level: "user" }, token: u.token }],
    ]);
  });
  it("后端答 stale ⇒ 带着码抛（卡上据它给「重看」）", async () => {
    invokeMock.mockRejectedValueOnce(refusedReply("stale", "看过之后又变了"));
    const err = await extHubApply(
      { kind: "skill", name: "demo", from: null, to: "gpd", scope: { from: { level: "user" }, to: { level: "user" } } },
      decodeExtCard(EXT.card),
      {},
    ).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(ExtRefused);
    expect((err as ExtRefused).code).toBe("stale");
  });
});
