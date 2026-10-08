// 卡上的时刻照抄后端写好的钟面（记录的 `timeText` · 轮次的 `startText` / `endText` · 子运行的 `started_text`），界面不从原始时刻换算。
// 夹具故意让原始时刻与钟面对不上（原始是 UTC 02:01，钟面写 17:45）：界面若还在自己换算，这几条就红。
import { describe, it, expect } from "vitest";
import { renderMessage, buildToolGroup, addToToolGroup } from "../../../../src/frontend/ui/cards/index";
import { buildNoticeLine, mergeNotice } from "../../../../src/frontend/ui/cards/speaker-bar";
import { factsOf } from "../../../../src/frontend/ui/agent-window-text";
import { LOCAL_ORIGIN } from "../../../../src/frontend/ui/ipc/origin";
import type { JsonlRecord } from "../../../../src/frontend/ui/generated/JsonlRecord";
import type { RunInfo } from "../../../../src/frontend/ui/generated/RunInfo";
import { copyText } from "../../../../src/frontend/ui/copy-table";

const ctx = () => ({ parentPath: "/p/s.jsonl", origin: LOCAL_ORIGIN, toolUseNames: new Map(), toolUseElements: new Map(), pendingToolResults: new Map(), lazy: false });
const AT = "2026-10-06T02:01:00.000Z";

describe("卡上的时刻照抄后端写好的钟面", () => {
  it("人说的那张卡、助手那张卡：卡头那一格 ＝ timeText", () => {
    const user = { type: "user", uuid: "u1", parentUuid: null, timestamp: AT, timeText: "17:45", message: { role: "user", content: "hi" }, userText: { speaker: { kind: "human" }, text: "hi" } } as unknown as JsonlRecord;
    const asst = { type: "assistant", uuid: "a1", parentUuid: null, timestamp: AT, timeText: "17:46", message: { role: "assistant", content: [{ type: "text", text: "ok" }] } } as unknown as JsonlRecord;
    for (const [rec, want] of [[user, "17:45"], [asst, "17:46"]] as const) {
      const r = renderMessage(rec, ctx());
      if (r.kind !== "card") throw new Error(r.kind);
      expect(r.element.querySelector(".card-header .ts")?.textContent).toBe(want);
    }
  });

  it("工具组收着那一行：起始时刻 ＝ 第一条的 timeText", () => {
    const rec = { type: "assistant", uuid: "a2", parentUuid: null, timestamp: AT, timeText: "17:47", message: { role: "assistant", content: [{ type: "tool_use", id: "t1", name: "Read", input: {} }] } } as unknown as JsonlRecord;
    const r = renderMessage(rec, ctx());
    if (r.kind !== "tool-group") throw new Error(r.kind);
    const g = buildToolGroup(r.time);
    addToToolGroup(g, r.units);
    expect(g.summary.textContent).toBe(copyText("cards.toolGroup.summary", { count: 1, since: "17:47" }));
  });

  it("相邻的后台通知并成一条：时段按时刻排先后、写的是各自的钟面", () => {
    const sp = { kind: "taskNotification", status: "completed", summary: "x" } as never;
    const late = buildNoticeLine(sp, "2026-10-06T02:09:00.000Z", "18:09");
    const early = buildNoticeLine(sp, "2026-10-06T02:03:00.000Z", "18:03");
    mergeNotice(late as HTMLDetailsElement, early as HTMLDetailsElement);
    expect(late.querySelector(".notice-head")?.textContent).toBe(copyText("speaker.notice.many", { n: 2, span: "18:03–18:09", fail: "" }));
  });

  it("agent 窗口「几点开始」＝ started_text", () => {
    const r: RunInfo = { run: "r1", state: "done", started_ms: Date.parse(AT), started_text: "17:48", ended_ms: Date.parse(AT) + 60_000 };
    expect(factsOf(r, 0, Date.parse(AT) + 120_000)[0]).toBe(copyText("agentWindow.facts.started", { time: "17:48" }));
  });
});
