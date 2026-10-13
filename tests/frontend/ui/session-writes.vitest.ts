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
vi.mock("../../../src/comms/inward/chan", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../../src/comms/inward/chan")>()),
  chan: {
    call: async (origin: string, op: string, body: Uint8Array, budget: { until: number }) => {
      sent.push({ origin, op, body: JSON.parse(new TextDecoder().decode(body)), until: budget?.until });
      return new TextEncoder().encode(JSON.stringify(reply));
    },
  },
}));

import { deleteSession } from "../../../src/frontend/ui/session-writes";

beforeEach(() => {
  sent.length = 0;
});

describe("MIG-3b 删会话经通道直说那台后端", () => {
  it("只交 sid", async () => {
    reply = { path: "/p/s1.jsonl" };
    await deleteSession("<local>", "s1");
    expect(sent).toEqual([{ origin: "<local>", op: "files-delete-session", body: { sid: "s1" }, until: sent[0].until }]);
  });
});
