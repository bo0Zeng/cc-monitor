/**
 * 派出子运行的那张卡**建出来那一刻**就照运行表标状态：会话流里运行表那一格（`runs`）先于记录行到（后端接上会话时先读子运行记录），
 * 卡又是懒建的 —— 只在运行表到的那一刻给已有的卡标，之后建的卡永远不标（「完成 / 在跑」那一格空着）。
 * 建卡时问上下文要（`RenderContext.onRunCard`，主窗口那一侧照它的运行表标、连「窗口已开」一起）。
 */
import { describe, expect, it, vi } from "vitest";
import { renderMessage } from "../../../../src/frontend/ui/cards/index";
import { LOCAL } from "../../../../src/frontend/ui/ipc/origin";
import type { LineRecord } from "../../../../src/frontend/ui/generated/LineRecord";
import type { RunInfo } from "../../../../src/frontend/ui/generated/RunInfo";
import { buildAgentCard, markRunCard } from "../../../../src/frontend/ui/cards/subagent";
import { copyText } from "../../../../src/frontend/ui/copy-table";

describe("派出卡建出来就交给上下文标运行状态", () => {
  it("卡型是 agent 的那次调用 ⇒ 建卡时报一声（调用 id ＋ 那张卡）", () => {
    const onRunCard = vi.fn();
    const ctx = { parentPath: "/p/s.jsonl", origin: LOCAL, toolUseNames: new Map(), toolUseElements: new Map(), pendingToolResults: new Map(), runCards: new Map(), lazy: false, onRunCard };
    const rec = {
      agent: "claude",
      t: "reply",
      id: "a",
      at: "2026-01-01T00:00:00.000Z",
      blocks: [{ type: "tool_use", id: "t1", name: "Task" }],
      cards: { t1: "agent" },
      runs: { t1: { label: "补测试", kind: "general-purpose" } },
      autoReply: false,
    } as unknown as LineRecord;
    renderMessage(rec, ctx as never);
    expect(onRunCard).toHaveBeenCalledTimes(1);
    expect(onRunCard.mock.calls[0][0]).toBe("t1");
    expect(onRunCard.mock.calls[0][1]).toBe(ctx.runCards.get("t1"));
  });
});

describe("卡上用了多久照抄运行表写好的那一句", () => {
  const info = (o: Partial<RunInfo>): RunInfo => ({ run: "r", state: "done", status_text: "核心·完成", label_text: "x", ...o }) as RunInfo;
  const stateOf = (card: HTMLElement): string | null | undefined => card.querySelector('[data-role="run-state"]')?.textContent;

  it("收场了 ⇒ 状态 · 写好的用时（不拿起止时刻现算）", () => {
    const card = buildAgentCard("t1", "Task", undefined);
    const i = info({ took_text: "核心·用时", started_ms: 0 });
    markRunCard(card, i, i);
    expect(stateOf(card)).toBe(["核心·完成", "核心·用时"].join(copyText("kit.text.sep")));
  });

  it("没有写好的用时 ⇒ 只写状态", () => {
    const card = buildAgentCard("t1", "Task", undefined);
    const i = info({ started_ms: 0 });
    markRunCard(card, i, i);
    expect(stateOf(card)).toBe("核心·完成");
  });
});
