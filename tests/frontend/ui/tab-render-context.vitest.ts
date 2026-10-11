/**
 * 主窗口 tab 建卡的上下文带上「按记录 id 取回全文」（`Tab.fullRecord`）：出口省掉了入参 / 结果正文的卡，
 * 展开那一下经它取回那一行全文；不带 ⇒ 主窗口里展开一律「无法取回全文」（查看器那一路带着，主窗口这一路漏了）。
 */
import { describe, expect, it } from "vitest";
import { tabRenderContext } from "../../../src/frontend/ui/tab-stream-view";
import type { Tab } from "../../../src/frontend/ui/tab-model";

describe("主窗口 tab 建卡的上下文", () => {
  it("带着这个 tab 的「按记录 id 取回全文」", () => {
    const fullRecord = (): Promise<null> => Promise.resolve(null);
    const tab = {
      sessionId: "s1",
      origin: "<local>",
      agent: "claude",
      parentPath: "/home/user/.claude/projects/p/s1.jsonl",
      toolUseNames: new Map(),
      toolUseElements: new Map(),
      runCards: new Map(),
      pendingToolResults: new Map(),
      needs: null,
      pending: [],
      retries: new Map(),
      fullRecord,
    } as unknown as Tab;
    const ctx = tabRenderContext(tab, { runLabelOf: () => undefined }, false);
    expect(ctx.fullRecord).toBe(fullRecord);
  });
});
