// A5：isCompactRecord —— 换号重启 compact 完成检测：只读后端判好的来源（`userText.speaker`）。
// 认压缩摘要那一步在后端（`agents/claudecode/text.rs::user_text`，判据住 `tests/backend/agents/claudecode/text_tests.rs`）；这里只锁「读成品」。
import { describe, it, expect } from "vitest";
import { isCompactRecord } from "../../../../src/frontend/ui/cards/index";
import type { JsonlRecord } from "../../../../src/frontend/ui/generated/JsonlRecord";

const PREFIX = "This session is being continued from a previous conversation";
const user = (kind: string, text: string, content = ""): JsonlRecord =>
  ({ type: "user", uuid: "u1", message: { role: "user", content }, userText: { speaker: { kind }, text } }) as never;

describe("isCompactRecord（A5 compact 检测）", () => {
  it("后端判为压缩摘要 → true（正文长什么样都不看）", () => {
    expect(isCompactRecord(user("compactSummary", "甲乙摘要", "甲乙摘要"))).toBe(true);
  });
  it("后端判为别的来源 → false（正文以前缀开头也不看）", () => {
    expect(isCompactRecord(user("human", `${PREFIX}…`, `${PREFIX}…`))).toBe(false);
    expect(isCompactRecord(user("system", ""))).toBe(false);
  });
  it("非 user → false（正文里有前缀也不算）", () => {
    const asst = { type: "assistant", uuid: "a1", message: { role: "assistant", content: `${PREFIX}…` } } as never;
    expect(isCompactRecord(asst)).toBe(false);
  });
});
