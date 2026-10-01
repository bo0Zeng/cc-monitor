/**
 * 要求：「删会话 · 分叉仍经 monitor 这一跳转 …… 要先像广播那样收进后端，界面才谈得上直问」·
 * `§14.3` C 组（读会话正文 · 子 agent · 删会话 · 分叉「本机远端同一条路问那台后端」）。
 *
 * 界面经通道直说那台后端：分叉 `session-fork` · 删 `files-delete-session`（`src/frontend/ui/session-writes.ts`）。
 * ① 分叉成品两侧对拍：后端测试产出金样 `tests/__fixtures__/session-fork.golden.json`，这里的解码器读同一份（多一格 / 缺一格 / 类型不对 ⇒ 抛）；
 * ② 请求体恰是金样里那两格、发给调用方给的那一台、带期限；③ 删会话只交 sid。夹具只造结构，不含真会话。
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

import golden from "../../__fixtures__/session-fork.golden.json";
import { decodeFork, deleteSession, forkSession } from "../../../src/frontend/ui/session-writes";

beforeEach(() => {
  sent.length = 0;
});

describe("MIG-3b 分叉经通道直说那台后端", () => {
  it("★ 金样的成品原样收下；多一格 / 缺一格 / 类型不对 ⇒ 抛", () => {
    expect(decodeFork(golden.product)).toEqual(golden.product);
    for (const bad of [
      { ...golden.product, extra: 1 },
      { sessionId: golden.product.sessionId },
      { ...golden.product, jsonlPath: 1 },
      null,
    ]) {
      expect(() => decodeFork(bad), JSON.stringify(bad)).toThrow(/shape mismatch/);
    }
  });

  it("请求体恰是金样那两格、发给那一台、带期限；成品认不出 ⇒ 说「先别重试」", async () => {
    reply = golden.product;
    const r = await forkSession("devbox", golden.request.sid, golden.request.uuid);
    expect(r).toEqual(golden.product);
    expect(sent).toHaveLength(1);
    expect(sent[0].origin).toBe("devbox");
    expect(sent[0].op).toBe("session-fork");
    expect(sent[0].body).toEqual(golden.request);
    expect(typeof sent[0].until).toBe("number");
    reply = { sessionId: "x" };
    await expect(forkSession("<local>", "s", "u")).rejects.toThrow(/先别重试/);
  });
});

describe("MIG-3b 删会话经通道直说那台后端", () => {
  it("只交 sid；成品恰好一格 `path`，别的形状 ⇒ 抛", async () => {
    reply = { path: "/p/s1.jsonl" };
    await deleteSession("<local>", "s1");
    expect(sent).toEqual([{ origin: "<local>", op: "files-delete-session", body: { sid: "s1" }, until: sent[0].until }]);
    reply = { path: "/p", extra: 1 };
    await expect(deleteSession("devbox", "s1")).rejects.toThrow(/shape mismatch/);
  });
});
