/**
 * 计划那几问没成时，通道上的失败原样交到调用方（那一句 ＋ 复制详情是出错那一端写的），界面不另包一层码。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { ChanError } from "../../../src/comms/inward/chan";
import { fetchPlanRead, takeRepo } from "../../../src/frontend/ui/plan-reads";
import { chanReply, refusedReply } from "../../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

beforeEach(() => {
  invokeMock.mockReset();
});

describe("plan-* 没成", () => {
  it("被拒 ⇒ 抛的就是通道那一份（那一句 ＋ 详情）", async () => {
    invokeMock.mockRejectedValueOnce(refusedReply("no_pb", "这台没装 pb"));
    const e = await fetchPlanRead("devbox", "/w").catch((x: unknown) => x);
    expect(e).toBeInstanceOf(ChanError);
    expect([(e as ChanError).message, (e as ChanError).detail]).toEqual(["这台没装 pb", refusedReply("no_pb", "").detail]);
  });
  it("接过仓库回包没说接到哪一片 ⇒ 不当成了，那一句是回包里那一句", async () => {
    invokeMock.mockResolvedValueOnce(chanReply({ said: "pb 没认这一片", took: null, path: null }));
    await expect(takeRepo("devbox", "/w", "invoice")).rejects.toThrow("pb 没认这一片");
  });
});

describe("记原话：代敲 plan-command record（片 · 定了什么 · 原话照给）", () => {
  it("请求里带着看着的那一片、定了什么与每一句原话", async () => {
    const { recordQuote } = await import("../../../src/frontend/ui/plan-reads");
    invokeMock.mockResolvedValueOnce(chanReply({ rc: 0, said: "记下了 原话 3", path: null, took: null }));
    const r = await recordQuote("devbox", "/w", "beta", "读写分两半", ["分开做", "先读后写"]);
    expect(r.said).toBe("记下了 原话 3");
    const payload = (invokeMock.mock.calls[0][1] as { payload: number[] }).payload;
    const sent = JSON.parse(new TextDecoder().decode(new Uint8Array(payload))) as Record<string, unknown>;
    expect(sent).toEqual({ workspace: "/w", cmd: "record", slice: "beta", summary: "读写分两半", quotes: ["分开做", "先读后写"] });
  });
});
