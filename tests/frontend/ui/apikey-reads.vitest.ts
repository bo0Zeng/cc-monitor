/**
 * 写 key 那一问（`apikey-key-set`）走通道、后端出成品之后的判据（`apikey-read` · `apikey-routing` 两条没有读者，删了）。
 *
 * 要求：「成品的两侧对拍：界面按形状严格收（多一格 / 缺一格 / 类型不对 ⇒ 抛「两端契约对不上」，不猜）」· B 组 `creds.apikey`。
 *
 * | 性质 | 判据 |
 * |---|---|
 * | 形状不对 ⇒ 抛，不替后端补值（尤其：多一格装明文那一形被拒） | 「严格收」 |
 * | 经通道说对的帧命令、对的请求体，失败折成一句人话 | 「请求」「失败」 |
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { ReplyUnreadable } from "../../../src/frontend/ui/ipc/chan-caller";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { decodeApikeyWritten, writeApikeyKey } from "../../../src/frontend/ui/apikey-reads";
import {
  chanArgsJson,
  chanReply,
  NO_CHANNEL,
  type ChanCallArgs,
} from "../../test-support/chan-fake";
import { copyText } from "../../../src/frontend/ui/copy-table";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

beforeEach(() => {
  invokeMock.mockReset();
});

// ═══ 写 key 也走通道（`apikey-key-set`）═══════════════════════════════════
//
// （`creds.apikey` 经 `chan.call`）；表「`creds.apikey` 写 … 等 HX2 落了再迁」；
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
      expect(() => decodeApikeyWritten(bad)).toThrow(ReplyUnreadable);
    }
  });
  it("失败说人话、话里不带明文", async () => {
    invokeMock.mockRejectedValue(NO_CHANNEL);
    const err = await writeApikeyKey("host-a", "/d", "sk-SHOULD-NOT-SHOW").catch((e: Error) => e);
    expect(String(err)).toMatch(copyText("chanCaller.said.unreachable"));
    expect(String(err)).not.toContain("sk-SHOULD-NOT-SHOW");
  });
});
