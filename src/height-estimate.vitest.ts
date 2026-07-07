/**
 * Batch13-F38:估高纯算术部分单测(pretext 在 jsdom 无 canvas,走降级路径——
 * 这正好把降级路径钉住;pretext 真值路径由 E2E/真机覆盖)。
 */
import { describe, it, expect } from "vitest";
import {
  fallbackTextHeight,
  codeBlockHeight,
  estimateStreamNodeHeight,
} from "./height-estimate";

describe("fallbackTextHeight", () => {
  it("空文本高度为 0", () => {
    expect(fallbackTextHeight("", 15, 24.75, 780)).toBe(0);
  });
  it("单短行 = 一个行高", () => {
    expect(fallbackTextHeight("hello", 15, 24.75, 780)).toBeCloseTo(24.75);
  });
  it("硬换行逐行计数", () => {
    expect(fallbackTextHeight("a\nb\nc", 15, 24.75, 780)).toBeCloseTo(3 * 24.75);
  });
  it("CJK 按全宽折行(50 个汉字 @15px 在 375px 宽 = 2 行)", () => {
    const text = "汉".repeat(50);
    expect(fallbackTextHeight(text, 15, 24.75, 375)).toBeCloseTo(2 * 24.75);
  });
});

describe("codeBlockHeight", () => {
  it("行数线性(10 行比 1 行多 9 个 mono 行高)", () => {
    const one = codeBlockHeight("x");
    const ten = codeBlockHeight(Array(10).fill("x").join("\n"));
    expect(ten - one).toBeCloseTo(9 * 13 * 1.55);
  });
});

describe("estimateStreamNodeHeight", () => {
  it("折叠 details(工具组)= summary 常数,与内容量无关", () => {
    const d = document.createElement("details");
    d.className = "card card-tool-group";
    d.innerHTML = "<summary>🔧 工具调用 · 99 个</summary><div>" + "x".repeat(5000) + "</div>";
    expect(estimateStreamNodeHeight(d)).toBe(38);
  });
  it("user 气泡按正文文本估高(非空、有限)", () => {
    const el = document.createElement("div");
    el.className = "card card-user";
    el.innerHTML = '<div class="card-body">你好,这是一条测试消息</div>';
    const h = estimateStreamNodeHeight(el);
    expect(h).toBeGreaterThan(20);
    expect(h).toBeLessThan(200);
  });
  it("认不出的形态返回 null(CSS 兜底接管)", () => {
    const el = document.createElement("div");
    el.className = "card-api-error";
    expect(estimateStreamNodeHeight(el)).toBeNull();
  });
});
