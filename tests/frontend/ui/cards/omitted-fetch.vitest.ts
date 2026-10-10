// 出口省掉的正文（工具入参 · 结果正文）展开那一下按记录 id 取回（`RenderContext.fullRecord`）：
// 取回还没落、又收起再展开（或一次点开来了两个 toggle）⇒ 不再发第二问、不画两份；取失败 ⇒ 下次展开再取。
import { describe, it, expect, vi } from "vitest";
import { renderMessage } from "../../../../src/frontend/ui/cards/index";
import { LOCAL_ORIGIN } from "../../../../src/frontend/ui/ipc/origin";
import type { LineRecord } from "../../../../src/frontend/ui/generated/LineRecord";

const folded = {
  agent: "claude",
  t: "reply",
  id: "a1",
  at: "2026-01-01T00:00:00.000Z",
  blocks: [{ type: "tool_use", id: "t1", name: "Grep" }],
  steps: { t1: { tool: "Grep", arg: "x", known: true } },
  autoReply: false,
  endsTurn: false,
} as unknown as LineRecord;
const full = { ...folded, blocks: [{ type: "tool_use", id: "t1", name: "Grep", input: { pattern: "ZQ-INPUT" } }] } as unknown as LineRecord;

function rig(fullRecord: (id: string) => Promise<LineRecord | null>) {
  const ctx = {
    parentPath: "/p/s.jsonl",
    origin: LOCAL_ORIGIN,
    toolUseNames: new Map(),
    toolUseElements: new Map(),
    pendingToolResults: new Map(),
    lazy: false,
    fullRecord,
  };
  renderMessage(folded, ctx as never);
  const card = ctx.toolUseElements.get("t1") as HTMLDetailsElement | undefined;
  expect(card, "台子没建出那一步的卡 —— 下面恒空").toBeDefined();
  // 只改 `open`：toggle 事件由 jsdom 自己发（同真浏览器，异步一次）。
  const toggle = async (open: boolean): Promise<void> => {
    card!.open = open;
    await settle();
  };
  return { card: card!, toggle };
}

const settle = () => new Promise((r) => setTimeout(r, 0));

describe("省掉的入参展开取回", () => {
  it("★ 取回还没落又展开一次 ⇒ 只问一次、只画一份", async () => {
    let land!: (r: LineRecord) => void;
    const ask = vi.fn(() => new Promise<LineRecord | null>((r) => (land = r)));
    const { card, toggle } = rig(ask);
    await toggle(true);
    await toggle(false);
    await toggle(true);
    expect(ask).toHaveBeenCalledTimes(1);
    land(full);
    await settle();
    expect(card.querySelectorAll(".block-args")).toHaveLength(1);
    expect(card.querySelectorAll(".block-omitted")).toHaveLength(0);
    expect(card.textContent).toContain("ZQ-INPUT");
  });

  it("★ 取失败 ⇒ 下次展开再取", async () => {
    const ask = vi.fn(() => Promise.reject(new Error("断了")));
    const { card, toggle } = rig(ask);
    await toggle(true);
    await toggle(false);
    await toggle(true);
    expect(ask).toHaveBeenCalledTimes(2);
    expect(card.querySelectorAll(".block-omitted"), "上一次失败那一句该换掉").toHaveLength(1);
  });
});
