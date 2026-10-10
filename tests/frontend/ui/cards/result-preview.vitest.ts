// 工具结果那一行的首行预览是核心出的一格（`results[id].preview`，截法只在核心一处：`agents/claudecode/steps.rs::preview_of`）。
// 出口省掉 `blocks[type=tool_result].content`（`omit` 声明）之后，结果那一行照样写「Output · 预览」；界面自己不从正文里截。
import { describe, it, expect } from "vitest";
import { renderMessage } from "../../../../src/frontend/ui/cards/index";
import { LOCAL_ORIGIN } from "../../../../src/frontend/ui/ipc/origin";
import type { LineRecord } from "../../../../src/frontend/ui/generated/LineRecord";

const ctx = () => ({
  parentPath: "/p/s.jsonl",
  origin: LOCAL_ORIGIN,
  toolUseNames: new Map(),
  toolUseElements: new Map(),
  pendingToolResults: new Map(),
  lazy: false,
});
const use = {
  agent: "claude",
  t: "reply",
  id: "a1",
  at: "2026-01-01T00:00:00.000Z",
  blocks: [{ type: "tool_use", id: "t1", name: "Bash" }],
  steps: { t1: { tool: "Bash", arg: "ls", known: true } },
  autoReply: false,
  endsTurn: false,
} as unknown as LineRecord;
const result = (content: unknown, preview?: string): LineRecord =>
  ({
    agent: "claude",
    t: "said",
    id: "r1",
    at: "2026-01-01T00:00:01.000Z",
    who: { speaker: { kind: "toolResult" }, text: "" },
    blocks: [{ type: "tool_result", for: "t1", isError: false, ...(content === undefined ? {} : { content }) }],
    results: { t1: { ok: true, ...(preview === undefined ? {} : { preview }) } },
  }) as unknown as LineRecord;

function summaryOf(res: LineRecord): string {
  const c = ctx();
  renderMessage(use, c as never);
  renderMessage(res, c as never);
  const host = c.toolUseElements.get("t1") as HTMLElement | undefined;
  expect(host, "台子没建出那一步的卡 —— 下面恒空").toBeDefined();
  return host!.querySelector(".block-tool-result-inline > .block-summary")?.textContent ?? "";
}

describe("结果那一行的首行预览", () => {
  it("★ 正文被出口省掉了 ⇒ 预览照样来自核心那一格", () => {
    expect(summaryOf(result(undefined, "第一行"))).toBe("Output · 第一行");
  });

  it("★ 界面不自己截：正文在、核心没给预览（结果是空的）⇒ 不从正文里截出一行", () => {
    expect(summaryOf(result([{ type: "text", text: "不该出现" }]))).not.toContain("不该出现");
  });
});
