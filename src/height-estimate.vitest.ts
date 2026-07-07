/**
 * Batch13-F38:估高单测。
 * 环境事实(D 审计实证):jsdom 里 pretext import 成功但首次 prepare 因无 canvas
 * 抛错 → 实际执行「运行时抛错 → 算术降级」分支;fallbackTextHeight 直测钉住降级
 * 算术;涉及 textHeight 的断言按降级路径的精确值写(若未来测试环境获得 canvas,
 * pretext 真值路径结果不同,这些断言会有意识地失败提醒重校)。
 */
import { describe, it, expect } from "vitest";
import {
  fallbackTextHeight,
  codeBlockHeight,
  extractProseText,
  estimateStreamNodeHeight,
  applyIntrinsicSize,
} from "./height-estimate";

const LH_PROSE = 15 * 1.65;
const LH_BASE = 14 * 1.55;
const LH_MONO = 13 * 1.55;

describe("fallbackTextHeight", () => {
  it("空文本高度为 0", () => {
    expect(fallbackTextHeight("", 15, LH_PROSE, 780)).toBe(0);
  });
  it("单短行 = 一个行高", () => {
    expect(fallbackTextHeight("hello", 15, LH_PROSE, 780)).toBeCloseTo(LH_PROSE);
  });
  it("硬换行逐行计数,CRLF 的 \\r 不污染宽度", () => {
    expect(fallbackTextHeight("a\r\nb\r\nc", 15, LH_PROSE, 780)).toBeCloseTo(3 * LH_PROSE);
  });
  it("CJK 按全宽折行(50 个汉字 @15px 在 375px 宽 = 2 行)", () => {
    expect(fallbackTextHeight("汉".repeat(50), 15, LH_PROSE, 375)).toBeCloseTo(2 * LH_PROSE);
  });
});

describe("codeBlockHeight", () => {
  it("行数线性(10 行比 1 行多 9 个 mono 行高)", () => {
    const one = codeBlockHeight("x");
    const ten = codeBlockHeight(Array(10).fill("x").join("\n"));
    expect(ten - one).toBeCloseTo(9 * LH_MONO);
  });
  it("空代码块按 1 行计", () => {
    expect(codeBlockHeight("")).toBe(codeBlockHeight("x"));
  });
});

describe("extractProseText(R1:块感知提取)", () => {
  it("<br> 还原为换行(user 卡 renderPlainText 形态)", () => {
    const el = document.createElement("div");
    el.innerHTML = "line1<br>line2<br>line3";
    expect(extractProseText(el).text).toBe("line1\nline2\nline3");
  });
  it("<p>/<li> 边界插换行并计块数", () => {
    const el = document.createElement("div");
    el.innerHTML = "<p>para1</p><p>para2</p><ul><li>a</li><li>b</li></ul>";
    const r = extractProseText(el);
    expect(r.text).toBe("para1\npara2\na\nb");
    expect(r.blockCount).toBe(4);
  });
  it("跳过 .code-block 与 .katex-mathml(单独估/aria 重复)", () => {
    const el = document.createElement("div");
    el.innerHTML =
      '<p>before</p><div class="code-block"><pre>const x=1</pre></div><span class="katex-mathml">dup</span><p>after</p>';
    expect(extractProseText(el).text).toBe("before\nafter");
  });
});

describe("estimateStreamNodeHeight", () => {
  it("折叠 details(工具组)= summary 常数,与内容量无关", () => {
    const d = document.createElement("details");
    d.className = "card card-tool-group";
    d.innerHTML = "<summary>🔧 工具调用 · 99 个</summary><div>" + "x".repeat(5000) + "</div>";
    expect(estimateStreamNodeHeight(d)).toBe(38);
  });
  it("展开的顶层 details 返回 null(走 CSS 兜底+auto 记忆)", () => {
    const d = document.createElement("details");
    d.open = true;
    d.innerHTML = "<summary>s</summary><div>body</div>";
    expect(estimateStreamNodeHeight(d)).toBeNull();
  });
  it("user 气泡:<br> 断行参与估高 = 降级路径精确值", () => {
    const el = document.createElement("div");
    el.className = "card card-user";
    el.innerHTML = '<div class="card-body">短行1<br>短行2<br>短行3</div>';
    const expected = fallbackTextHeight("短行1\n短行2\n短行3", 14, LH_BASE, 780 * 0.8 - 34);
    expect(estimateStreamNodeHeight(el)).toBeCloseTo(expected);
  });
  it("assistant 卡分块累加:prose + 嵌套代码块 + 折叠 details(R2)", () => {
    const el = document.createElement("div");
    el.className = "card card-assistant";
    const code = Array(10).fill("x").join("\n");
    el.innerHTML =
      '<div class="card-header">h</div><div class="card-body">' +
      '<div class="block-text"><p>hello</p><div class="code-block"><pre>' +
      code +
      "</pre></div></div>" +
      '<details class="block-collapsible"><summary>💭</summary></details>' +
      "</div>";
    const prose = fallbackTextHeight("hello", 15, LH_PROSE, 780) + 10; // + BLOCK_GAP
    const expected = 22 + prose + codeBlockHeight(code) + 24 + 38; // header + text块 + code+margin + details
    expect(estimateStreamNodeHeight(el)).toBeCloseTo(expected);
  });
  it("api-error/slash 细条卡走常数(非 120px 兜底)", () => {
    const err = document.createElement("div");
    err.className = "card card-api-error";
    expect(estimateStreamNodeHeight(err)).toBe(40);
    const slash = document.createElement("div");
    slash.className = "card card-slash";
    expect(estimateStreamNodeHeight(slash)).toBe(34);
  });
  it("认不出的形态返回 null(CSS 兜底接管)", () => {
    const el = document.createElement("div");
    el.className = "card-unknown-kind";
    expect(estimateStreamNodeHeight(el)).toBeNull();
  });
});

describe("applyIntrinsicSize(F39 复用的契约面)", () => {
  it("写入格式 auto <N>px、四舍五入、24px 下限", () => {
    const el = document.createElement("div");
    el.className = "card card-user";
    el.innerHTML = '<div class="card-body">hi</div>';
    applyIntrinsicSize(el);
    const v = el.style.getPropertyValue("contain-intrinsic-size");
    expect(v).toMatch(/^auto \d+px$/);
    expect(parseInt(v.slice(5), 10)).toBeGreaterThanOrEqual(24);
  });
  it("估不出(null)时不写 style", () => {
    const el = document.createElement("div");
    el.className = "card-unknown-kind";
    applyIntrinsicSize(el);
    expect(el.style.getPropertyValue("contain-intrinsic-size")).toBe("");
  });
});
