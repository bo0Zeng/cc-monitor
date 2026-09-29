/**
 * 〔US1 · 第四波 4D〕API key 那两问（`apikey-read` · `apikey-routing`）改走通道、后端出成品之后的判据。
 *
 * 要求住址：`设计/05 §14.3`「成品的两侧对拍：界面按形状严格收（多一格 / 缺一格 / 类型不对 ⇒ 抛「两端契约对不上」，不猜）；
 * 线上形状由一份跨语言金样钉住（后端测试产出 == 金样 · TS 解码器读同一份）」· B 组 `creds.apikey` · `apikey.routing`。
 *
 * | 性质 | 判据 |
 * |---|---|
 * | TS 解码器读得懂**后端真出的**成品 —— 同一份金样，后端 `endpoint_tests::us1_the_apikey_products_match_the_cross_language_golden` 写它（异源：Rust 造、TS 解） | 「金样」 |
 * | 形状不对 ⇒ 抛，不替后端补值（尤其：多一格装明文那一形被拒） | 「严格收」 |
 * | 两问各自经通道说对的帧命令、对的请求体，失败折成一句人话（不退化成「没配」/「没行」） | 「请求」「失败」 |
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import {
  decodeApikeyRouting,
  decodeApikeyStatus,
  decodeApikeyWritten,
  fetchApikeyRouting,
  readApikeyStatus,
  writeApikeyKey,
} from "../../../src/frontend/ui/apikey-reads";
import { REPO_ROOT } from "../../test-support/repo-root";
import {
  chanArgsJson,
  chanReply,
  NO_CHANNEL,
  UNSUPPORTED,
  type ChanCallArgs,
} from "../../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const golden = JSON.parse(
  readFileSync(
    resolve(REPO_ROOT, "tests/__fixtures__/apikey.golden.json"),
    "utf8",
  ),
) as Record<string, unknown>;

beforeEach(() => {
  invokeMock.mockReset();
});

describe("金样：后端出的成品，TS 这一侧读得懂", () => {
  it("apikey-read：五格照收（只有掩码）", () => {
    expect(decodeApikeyStatus(golden["apikey-read"])).toEqual({
      configured: false,
      masked: "",
      path: "<root>/apikey-credentials.json",
      notice: null,
      problem: null,
    });
  });
  it("apikey-routing：表里有行的那一个原样回 · 中转在听", () => {
    expect(decodeApikeyRouting(golden["apikey-routing"])).toEqual({
      routed: ["/h/.claude-alt/work"],
      running: true,
    });
  });
});

describe("严格收：形状不对 ⇒ 抛「两端版本对不上」", () => {
  const st = golden["apikey-read"] as Record<string, unknown>;
  const rt = golden["apikey-routing"] as Record<string, unknown>;
  it.each([
    ["多一格（装明文那一形）", { ...st, key: "sk-x" }],
    ["缺一格", (({ path: _p, ...r }) => r)(st)],
    ["类型不对", { ...st, configured: "yes" }],
    ["老后端还回 rows", { ...st, rows: [] }],
    ["不是对象", "x"],
  ])("apikey-read · %s", (_n, v) => {
    expect(() => decodeApikeyStatus(v)).toThrow(copyText("apikeyReads.status.badShape"));
  });
  it.each([
    ["多一格", { ...rt, x: 1 }],
    ["缺 running", { routed: [] }],
    ["routed 里不是字符串", { routed: [1], running: true }],
    ["running 不是布尔", { routed: [], running: 1 }],
  ])("apikey-routing · %s", (_n, v) => {
    expect(() => decodeApikeyRouting(v)).toThrow(copyText("apikeyReads.routing.badShape"));
  });
});

describe("请求：经通道问那台机器的后端", () => {
  it("apikey-read：op 对、请求体为空对象、origin 原样", async () => {
    invokeMock.mockResolvedValue(chanReply(golden["apikey-read"]));
    await readApikeyStatus("host-a");
    const calls = invokeMock.mock.calls;
    expect(calls.map((c) => c[0])).toEqual(["chan_call"]);
    const a = calls[0][1] as ChanCallArgs;
    expect([a.origin, a.op, chanArgsJson(a)]).toEqual([
      "host-a",
      "apikey-read",
      {},
    ]);
  });
  it("apikey-routing：op 对、agent 与 configDirs 随请求带", async () => {
    invokeMock.mockResolvedValue(chanReply(golden["apikey-routing"]));
    const got = await fetchApikeyRouting("<local>", "claude-code", [
      "/h/.claude-alt/work",
    ]);
    const a = invokeMock.mock.calls[0][1] as ChanCallArgs;
    expect([a.origin, a.op]).toEqual(["<local>", "apikey-routing"]);
    expect(chanArgsJson(a)).toEqual({
      agent: "claude-code",
      configDirs: ["/h/.claude-alt/work"],
    });
    expect(got.routed).toEqual(["/h/.claude-alt/work"]);
  });
});

describe("失败：一句人话，不退化成「没配」/「没行」", () => {
  it("那台后端不认这一问 ⇒ 说后端太旧", async () => {
    invokeMock.mockRejectedValue(UNSUPPORTED);
    await expect(readApikeyStatus("host-a")).rejects.toThrow(/太旧/);
  });
  it("没有控制通道 ⇒ 说够不着（不是空表）", async () => {
    invokeMock.mockRejectedValue(NO_CHANNEL);
    await expect(
      fetchApikeyRouting("host-a", "claude-code", ["/d"]),
    ).rejects.toThrow(/够不着/);
  });
});

// ═══ 〔HX2 · 第四波 4D〕写 key 也走通道（`apikey-key-set`）═══════════════════════════════════
//
// 要求住址：`设计/05 §14.3` B 组（`creds.apikey` 经 `chan.call`）；`第四波记录/US1.md` 表「`creds.apikey` 写 … 等 HX2 落了再迁」；
// `KH2C1`（前端一个字都不推账号 id：请求体里只有 `configDir`）。
describe("〔HX2〕W2 写 key：经通道交那台机器的后端", () => {
  const WRITTEN = { account: "work", path: "/h/k.json", masked: "sk-****", baseUrl: null };
  it("op 对、请求体恰是 {configDir, key}（带 Base URL 那一路多 baseUrl）、origin 原样、一发", async () => {
    invokeMock.mockResolvedValue(chanReply(WRITTEN));
    await writeApikeyKey("devbox", "/h/.claude-alt/work", "sk-FIXTURE");
    invokeMock.mockResolvedValue(chanReply(WRITTEN));
    await writeApikeyKey("<local>", "/h/.claude-alt/work", "sk-FIXTURE", "https://up.example/v1");
    const calls = invokeMock.mock.calls;
    expect(calls.map((c) => c[0])).toEqual(["chan_call", "chan_call"]);
    const [a, b] = calls.map((c) => c[1] as ChanCallArgs);
    expect([a.origin, a.op, chanArgsJson(a)]).toEqual([
      "devbox",
      "apikey-key-set",
      { configDir: "/h/.claude-alt/work", key: "sk-FIXTURE" },
    ]);
    expect([b.origin, b.op, chanArgsJson(b)]).toEqual([
      "<local>",
      "apikey-key-set",
      { configDir: "/h/.claude-alt/work", key: "sk-FIXTURE", baseUrl: "https://up.example/v1" },
    ]);
  });
  it("成品严格收：四格照收；多一格 / 缺一格 / 类型不对 ⇒ 抛（不补值）", () => {
    expect(decodeApikeyWritten(WRITTEN)).toEqual(WRITTEN);
    for (const bad of [
      { ...WRITTEN, key: "sk-FIXTURE" },
      { account: "work", path: "/h/k.json", masked: "sk-****" },
      { ...WRITTEN, baseUrl: 3 },
      null,
    ]) {
      expect(() => decodeApikeyWritten(bad)).toThrow(copyText("apikeyReads.write.badShape"));
    }
  });
  it("失败说人话、话里不带明文", async () => {
    invokeMock.mockRejectedValue(NO_CHANNEL);
    const err = await writeApikeyKey("host-a", "/d", "sk-SHOULD-NOT-SHOW").catch((e: Error) => e);
    expect(String(err)).toMatch(/够不着/);
    expect(String(err)).not.toContain("sk-SHOULD-NOT-SHOW");
  });
});
