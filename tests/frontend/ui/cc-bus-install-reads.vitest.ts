/**
 * 设计/05 §14.3：「成品的两侧对拍：界面按形状严格收（多一格 / 缺一格 / 类型不对 ⇒ 抛「两端契约对不上」，不猜）」。
 *
 * 〔MIG-3a · 子步 3〕cc-bus 装到本机改经通道问本机后端（`cc-bus-install` / `-state`）：三态严格收、装的结果严格收、问的是本机。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { decodeCcBusInstalled, decodeCcBusInstallState, installCcBus, readCcBusInstallState } from "../../../src/frontend/ui/cc-bus-install-reads";
import { chanArgsJson, chanReply, type ChanCallArgs } from "../../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
beforeEach(() => {
  invokeMock.mockReset();
});

describe("严格收", () => {
  it("三态各自收得下；drifted 缺格 / 多格 / 负数 ⇒ 抛", () => {
    expect(decodeCcBusInstallState({ state: "not_installed" })).toEqual({ state: "not_installed" });
    expect(decodeCcBusInstallState({ state: "drifted", differing: 1, missing: 0 })).toEqual({ state: "drifted", differing: 1, missing: 0 });
    expect(() => decodeCcBusInstallState({ state: "drifted", differing: 1 })).toThrow(/两端版本对不上/);
    expect(() => decodeCcBusInstallState({ state: "up_to_date", differing: 0 })).toThrow(/两端版本对不上/);
    expect(() => decodeCcBusInstallState({ state: "drifted", differing: -1, missing: 0 })).toThrow(/两端版本对不上/);
    expect(() => decodeCcBusInstallState({ state: "weird" })).toThrow(/两端版本对不上/);
  });
  it("装的结果：五格齐、类型对；多一格（从前的 warning）⇒ 抛", () => {
    const ok = { dest: "/h/.claude/skills/cc-bus", written: 3, unchanged: 0, backup: null, recordFailed: null };
    expect(decodeCcBusInstalled(ok)).toEqual(ok);
    expect(() => decodeCcBusInstalled({ ...ok, warning: null })).toThrow(/两端版本对不上/);
    expect(() => decodeCcBusInstalled({ ...ok, backup: 1 })).toThrow(/两端版本对不上/);
  });
});

describe("请求：问的是本机后端", () => {
  it("两问各一发、origin 是 <local>、请求体为空对象", async () => {
    invokeMock.mockResolvedValueOnce(chanReply({ state: "up_to_date" }));
    await readCcBusInstallState();
    invokeMock.mockResolvedValueOnce(chanReply({ dest: "/d", written: 0, unchanged: 28, backup: null, recordFailed: null }));
    await installCcBus();
    const calls = invokeMock.mock.calls.map((c) => c[1] as ChanCallArgs);
    expect(calls.map((a) => [a.origin, a.op, chanArgsJson(a)])).toEqual([
      ["<local>", "cc-bus-install-state", {}],
      ["<local>", "cc-bus-install", {}],
    ]);
  });
});
