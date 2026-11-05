// 卡上的时刻照抄后端写好的钟面（记录的 `timeText` · 轮次的 `startText`），界面不从原始时刻换算（子运行那一句见 `agent-window.vitest.ts`）。
// 夹具故意让原始时刻与钟面对不上（原始是 UTC 02:01，钟面写 17:45）：界面若还在自己换算，这几条就红。
import { describe, it, expect } from "vitest";
import { renderMessage } from "../../../../src/frontend/ui/cards/index";
import { LOCAL } from "../../../../src/frontend/ui/ipc/origin";
import type { LineRecord } from "../../../../src/frontend/ui/generated/LineRecord";

const ctx = () => ({ parentPath: "/p/s.jsonl", origin: LOCAL, toolUseNames: new Map(), toolUseElements: new Map(), pendingToolResults: new Map(), lazy: false });
const AT = "2026-10-06T02:01:00.000Z";

describe("卡上的时刻照抄后端写好的钟面", () => {
  it("人说的那张卡、助手那张卡：卡头那一格 ＝ timeText", () => {
    const user = { agent: "claude", t: "said", id: "u1", at: AT, timeText: "17:45", blocks: [{ type: "text", text: "hi" }], who: { speaker: { kind: "human" }, text: "hi" }, draw: "card", human: true } as LineRecord;
    const asst = { agent: "claude", t: "reply", id: "a1", at: AT, timeText: "17:46", blocks: [{ type: "text", text: "ok" }], autoReply: false, draw: "card", human: false } as LineRecord;
    for (const [rec, want] of [[user, "17:45"], [asst, "17:46"]] as const) {
      const r = renderMessage(rec, ctx());
      if (r.kind !== "card") throw new Error(r.kind);
      expect(r.element.querySelector(".card-header .ts")?.textContent).toBe(want);
    }
  });
});
