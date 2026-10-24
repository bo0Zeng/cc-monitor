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
