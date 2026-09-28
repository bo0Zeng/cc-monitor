/**
 * 设计/05 §14.3：「成品的两侧对拍：界面按形状严格收（多一格 / 缺一格 / 类型不对 ⇒ 抛「两端契约对不上」，不猜）」。
 *
 * 〔MIG-3a〕`acct-iso-status` / `acct-iso-shellinit` 改走通道：成品由那台后端出（围栏在那边校验），这里按形状严格收、问对那台。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { decodeAcctIsoInstalled, decodeAcctIsoSnippet, decodeAcctIsoStatus, installAcctIso, readAcctIsoSnippet, readAcctIsoStatus } from "../src/acct-iso-reads";
import { chanArgsJson, chanReply, NO_CHANNEL, type ChanCallArgs } from "./test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
beforeEach(() => {
  invokeMock.mockReset();
});

describe("严格收", () => {
  it("够不着 ⇒ 抛（不是 installed:false）", async () => {
    invokeMock.mockRejectedValue(NO_CHANNEL);
    const got = await readAcctIsoStatus("devbox").then(
      () => "答了",
      (e: unknown) => (e instanceof Error ? e.message : "非 Error"),
    );
    expect(got).toMatch(/够不着/);
  });
  it("没装是答案；缺格 / 多格 / 类型不对 ⇒ 抛", () => {
    expect(decodeAcctIsoStatus({ installed: false, path: null, looked: "~/.local/bin" }).installed).toBe(false);
    expect(() => decodeAcctIsoStatus({ installed: false, path: null })).toThrow(/两端版本对不上/);
    expect(() => decodeAcctIsoStatus({ installed: false, path: null, looked: null, vendor_id: "v" })).toThrow(/两端版本对不上/);
    expect(() => decodeAcctIsoStatus({ installed: "no", path: null, looked: null })).toThrow(/两端版本对不上/);
    expect(decodeAcctIsoSnippet({ snippet: "x" })).toBe("x");
    expect(() => decodeAcctIsoSnippet({ snippet: "x", extra: 1 })).toThrow(/两端版本对不上/);
  });
});

describe("请求：问对那台、说对那条；问不出来不当成「没装」", () => {
  it("两问各一发、origin 原样、请求体为空对象", async () => {
    invokeMock.mockResolvedValueOnce(chanReply({ installed: true, path: "/h/.local/bin/cc-acct-iso", looked: null }));
    await readAcctIsoStatus("devbox");
    invokeMock.mockResolvedValueOnce(chanReply({ snippet: "s" }));
    expect(await readAcctIsoSnippet("<local>")).toBe("s");
    const calls = invokeMock.mock.calls.map((c) => c[1] as ChanCallArgs);
    expect(calls.map((a) => [a.origin, a.op, chanArgsJson(a)])).toEqual([
      ["devbox", "acct-iso-status", {}],
      ["<local>", "acct-iso-shellinit", {}],
    ]);
  });
});

// 〔MIG-3a · 09-28 裁 2〕落进用户目录那一步（`acct-iso-install`）：同样严格收、问对那台、`dir` 原样。
describe("acct-iso-install", () => {
  const ok = { link: "/h/.local/bin/cc-acct-iso", linked: false, config: "/h/.cc-acct-iso/config", configWritten: true, recordFailed: null };
  it("严格收：缺格 / 多格 / 类型不对 ⇒ 抛", () => {
    expect(decodeAcctIsoInstalled(ok)).toEqual(ok);
    const { recordFailed: _, ...short } = ok;
    expect(() => decodeAcctIsoInstalled(short)).toThrow(/两端版本对不上/);
    expect(() => decodeAcctIsoInstalled({ ...ok, extra: 1 })).toThrow(/两端版本对不上/);
    expect(() => decodeAcctIsoInstalled({ ...ok, linked: "yes" })).toThrow(/两端版本对不上/);
  });
  it("一发、origin 原样、请求体只有 dir", async () => {
    invokeMock.mockResolvedValueOnce(chanReply(ok));
    expect(await installAcctIso("devbox", "/h/.cc-monitor/bin/cc-acct-iso")).toEqual(ok);
    const calls = invokeMock.mock.calls.map((c) => c[1] as ChanCallArgs);
    expect(calls.map((a) => [a.origin, a.op, chanArgsJson(a)])).toEqual([["devbox", "acct-iso-install", { dir: "/h/.cc-monitor/bin/cc-acct-iso" }]]);
  });
});
