// A5：isCompactRecord —— 换号重启 compact 完成检测的判定（复用卡片渲染同一套 extractText →
// stripInternalNoise → isCompactSummary）。锁：role/前缀/剥噪/内容形态/畸形输入。
import { describe, it, expect } from "vitest";
import { isCompactRecord } from "./index";

const PREFIX = "This session is being continued from a previous conversation";
const rec = (message: unknown) => ({ message, uuid: "u1" });

describe("isCompactRecord（A5 compact 检测）", () => {
  it("role:user + content 串以 compact 前缀开头 → true", () => {
    expect(isCompactRecord(rec({ role: "user", content: `${PREFIX}. 摘要…` }))).toBe(true);
  });
  it("role:user + content 为 text-block 数组 → true", () => {
    expect(
      isCompactRecord(rec({ role: "user", content: [{ type: "text", text: `${PREFIX}…` }] })),
    ).toBe(true);
  });
  it("前有 <system-reminder> 包装 → 剥噪后仍识别 → true", () => {
    const wrapped = `<system-reminder>foo</system-reminder>\n${PREFIX}…`;
    expect(isCompactRecord(rec({ role: "user", content: wrapped }))).toBe(true);
  });
  it("role:assistant + 前缀 → false（非 user 不算）", () => {
    expect(isCompactRecord(rec({ role: "assistant", content: `${PREFIX}…` }))).toBe(false);
  });
  it("role:user 但普通文本 → false", () => {
    expect(isCompactRecord(rec({ role: "user", content: "帮我改个 bug" }))).toBe(false);
  });
  it("畸形 / 缺 message / null → false（不崩）", () => {
    expect(isCompactRecord(null)).toBe(false);
    expect(isCompactRecord({})).toBe(false);
    expect(isCompactRecord(rec({ role: "user" }))).toBe(false); // 无 content
    expect(isCompactRecord(rec(null))).toBe(false);
  });
});
