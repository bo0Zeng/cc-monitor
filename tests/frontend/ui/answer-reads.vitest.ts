/**
 * 答一条挂着的待答请求（`src/frontend/ui/answer-reads.ts` → 帧命令 `needs-answer`）：说对的命令、对的请求体，结局按码分三种。
 * - 交到了 ⇒ answered；
 * - `already_answered` · `closed_elsewhere`（别处先答了 / 撤了）⇒ gone，带核心那一句（块淡出、toast 照它）；
 * - 别的拒绝 · 连接那一跳没成 ⇒ failed，带那一句与复制详情（块照旧可答）。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const calls: { origin: string; op: string; args: unknown }[] = [];
let reply: () => Promise<Uint8Array> = async () => new Uint8Array();
vi.mock("../../../src/comms/inward/chan", async (orig) => ({
  ...(await orig<typeof import("../../../src/comms/inward/chan")>()),
  chan: {
    call: async (origin: string, op: string, body: Uint8Array, budget: { until: number }) => {
      if (typeof budget?.until !== "number") throw new Error("没给期限");
      calls.push({ origin, op, args: JSON.parse(new TextDecoder().decode(body)) });
      return reply();
    },
    cachedOffer: () => undefined,
    offer: async () => undefined,
  },
}));

import { ChanError } from "../../../src/comms/inward/chan";
import { answerNeeds } from "../../../src/frontend/ui/answer-reads";

const enc = (v: unknown) => new TextEncoder().encode(JSON.stringify(v));
const refused = (code: string, message: string) => new ChanError({ layer: "peer", why: "refused", body: enc({ code, message }) }, "d1");

beforeEach(() => {
  calls.length = 0;
});

describe("needs-answer", () => {
  it("★ 帧命令与请求体：sid ＋ ask ＋ choice（附话 · picks 有才带）；交到了 ⇒ answered", async () => {
    reply = async () => enc({ result: "answered" });
    expect(await answerNeeds("devbox", "s1", { ask: "ask-1", choice: "deny-note", note: "n" })).toEqual({ kind: "answered" });
    expect(await answerNeeds("<local>", "s2", { ask: "ask-2", choice: "answer", picks: { q0: { options: ["q0o0"], text: "" } } })).toEqual({ kind: "answered" });
    expect(calls).toEqual([
      { origin: "devbox", op: "needs-answer", args: { sid: "s1", ask: "ask-1", choice: "deny-note", note: "n" } },
      { origin: "<local>", op: "needs-answer", args: { sid: "s2", ask: "ask-2", choice: "answer", picks: { q0: { options: ["q0o0"], text: "" } } } },
    ]);
  });

  it("★ 别处先答了 / 撤了 ⇒ gone，带那台写好的那一句", async () => {
    for (const code of ["already_answered", "closed_elsewhere"]) {
      reply = async () => {
        throw refused(code, `核心·${code}`);
      };
      expect(await answerNeeds("devbox", "s1", { ask: "a", choice: "allow" })).toEqual({ kind: "gone", said: `核心·${code}` });
    }
  });

  it("★ 别的拒绝 · 够不着 ⇒ failed（那一句 ＋ 复制详情），不当成答了", async () => {
    reply = async () => {
      throw refused("bad_choice", "核心·该项不可选");
    };
    expect(await answerNeeds("devbox", "s1", { ask: "a", choice: "x" })).toEqual({ kind: "failed", said: "核心·该项不可选", detail: "d1" });
    reply = async () => {
      throw new ChanError({ layer: "hop", at: { idx: 1, tag: "open" }, reach: "NotSent", why: "Unreachable" }, "d2");
    };
    const r = await answerNeeds("devbox", "s1", { ask: "a", choice: "allow" });
    expect(r.kind).toBe("failed");
    // 回包认不出那一层不在这里判：needs-answer 是 typed 命令，形状由握手指纹管（第 8 批）。
  });
});
