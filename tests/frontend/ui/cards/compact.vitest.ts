// A5：isCompactRecord —— 换号重启 compact 完成检测的判定（与卡片渲染同一套：`userText.clean` → isCompactSummary）。
// 〔RENDER2 · J10〕剥注入噪声那一步在 monitor（`search-core::user_text`，判据住 search-core 的 lib_tests）；这里只锁「读成品」。
import { describe, it, expect } from "vitest";
import { isCompactRecord } from "../../../../src/frontend/ui/cards/index";
import type { JsonlRecord } from "../../../../src/frontend/ui/generated/JsonlRecord";

const PREFIX = "This session is being continued from a previous conversation";
const user = (clean: string, content = ""): JsonlRecord =>
  ({ type: "user", uuid: "u1", message: { role: "user", content }, userText: { clean, interrupt: false } }) as never;

describe("isCompactRecord（A5 compact 检测）", () => {
  it("user 记录、剥过噪声的正文以 compact 前缀开头 → true（正文原文有包装也不看）", () => {
    expect(isCompactRecord(user(`${PREFIX}. 摘要…`, `<system-reminder>x</system-reminder>${PREFIX}`))).toBe(true);
  });
  it("user 但成品是普通文本 / 剥空了 → false（原文以前缀开头也不看）", () => {
    expect(isCompactRecord(user("帮我改个 bug", `${PREFIX}…`))).toBe(false);
    expect(isCompactRecord(user(""))).toBe(false);
  });
  it("非 user → false（正文里有前缀也不算）", () => {
    const asst = { type: "assistant", uuid: "a1", message: { role: "assistant", content: `${PREFIX}…` } } as never;
    expect(isCompactRecord(asst)).toBe(false);
  });
});
