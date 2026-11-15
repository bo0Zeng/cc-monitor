/**
 * 要求：「删会话 · 分叉仍经 monitor 这一跳转 …… 要先像广播那样收进后端，界面才谈得上直问」·
 * `§14.3` C 组（读会话正文 · 子 agent · 删会话 · 分叉「本机远端同一条路问那台后端」）。
 *
 * 界面经通道直说那台后端：删 `files-delete-session`（`src/frontend/ui/session-writes.ts`）。
 * 删会话只交 sid。夹具只造结构，不含真会话。（入参 · 回包都是生成类型，形状由握手指纹核。）
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const sent: { origin: string; op: string; body: unknown; until: unknown }[] = [];
let reply: unknown = null;
let refuse: Error | null = null;
vi.mock("../../../src/comms/inward/chan", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../../src/comms/inward/chan")>()),
  chan: {
    call: async (origin: string, op: string, body: Uint8Array, budget: { until: number }) => {
      sent.push({ origin, op, body: JSON.parse(new TextDecoder().decode(body)), until: budget?.until });
      if (refuse) throw refuse;
      return new TextEncoder().encode(JSON.stringify(reply));
    },
  },
}));

import { ChanError } from "../../../src/comms/inward/chan";
import { archiveSession, deleteSession, switchModel, unarchiveSession } from "../../../src/frontend/ui/session-writes";

beforeEach(() => {
  sent.length = 0;
  refuse = null;
});

describe("MIG-3b 删会话经通道直说那台后端", () => {
  it("只交 sid", async () => {
    reply = { path: "/p/s1.jsonl" };
    await deleteSession("<local>", "s1");
    expect(sent).toEqual([{ origin: "<local>", op: "files-delete-session", body: { sid: "s1" }, until: sent[0].until }]);
  });
});

describe("换模型只判没成", () => {
  it("★ 已发出（那一家不写读回）不算没成：照那台那一句，没有读回的字", async () => {
    reply = { result: "sent", model: null, said: "核心·已发出" };
    expect(await switchModel("<local>", "s1", "m1")).toEqual({ done: true, text: null, said: "核心·已发出" });
    reply = { result: "unsure", model: null, said: "核心·结果未知" };
    expect(await switchModel("<local>", "s1", "m1")).toEqual({ done: false, text: null, said: "核心·结果未知" });
  });
});

describe("归档 · 取消归档经通道直说那台后端", () => {
  it("各发那一条、只交 sid", async () => {
    reply = null;
    await archiveSession("<local>", "s1");
    await unarchiveSession("m2", "s2");
    expect(sent.map(({ origin, op, body }) => ({ origin, op, body }))).toEqual([
      { origin: "<local>", op: "files-archive-session", body: { sid: "s1" } },
      { origin: "m2", op: "files-unarchive-session", body: { sid: "s2" } },
    ]);
  });

  it("那台拒了 ⇒ 抛的错带那台那一句", async () => {
    const body = new TextEncoder().encode(JSON.stringify({ code: "refused", said: "核心·不归档" }));
    refuse = new ChanError({ layer: "peer", why: "refused", body }, "核心·不归档");
    await expect(archiveSession("<local>", "s1")).rejects.toThrow("核心·不归档");
    await expect(unarchiveSession("<local>", "s1")).rejects.toThrow("核心·不归档");
  });
});
