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
} from "../src/height-estimate";

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
  // 🔴 下面三条钉的是 2026-09-18 那次回摆(秤 2 §2):R1 保留了**所有** \n,而 marked
  //    产出的 HTML 里绝大多数换行只是**源码排版**——浏览器折叠成空格,估高却当硬断行
  //    ⇒ 正文卡系统性 ~2× 虚高。口径:只有块边界与 <br> 算断行。
  it("HTML 源码里的排版换行归一成空格(不是硬断行)", () => {
    const el = document.createElement("div");
    el.innerHTML = "<p>one\ntwo</p>\n<p>three</p>";
    expect(extractProseText(el).text).toBe("one two\nthree");
  });
  it("表格一行算一行(一行 4 个单元格不是 4 行)", () => {
    const el = document.createElement("div");
    el.innerHTML =
      "<table><tbody>\n<tr>\n<td>a</td>\n<td>b</td>\n</tr>\n<tr>\n<td>c</td>\n<td>d</td>\n</tr>\n</tbody></table>";
    const r = extractProseText(el);
    expect(r.text).toBe("a b\nc d");
    expect(r.blockCount).toBe(2);
  });
  it("真 pre-wrap 容器(.block-body 非 md)里的换行原样保留", () => {
    const el = document.createElement("div");
    el.innerHTML = '<pre class="block-body block-body-json">{\n  "a": 1\n}</pre>';
    expect(extractProseText(el).text).toBe('{\n  "a": 1\n}');
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
    // 19 = 12px(--font-size-small) × 1.55 的 **content-box** 值(秤 2 的 B 段实测
    // `contain-intrinsic-size` 吃 content-box;原来的 34 是按 border-box 手算的,
    // 多了一份 padding 6×2 ⇒ p90 相对误差 82.9%)。
    // ⚠ 2026-09-18 之前 `applyIntrinsicSize` 的 24px 地板会把 19 顶成 24(写进 style 的是 24);
    //   地板已按 `99 条 75` / `设计/17 订正④` 去掉 ⇒ 现在 19 原样出货,见下面那个 describe。
    expect(estimateStreamNodeHeight(slash)).toBe(19);
  });
  it("认不出的形态返回 null(CSS 兜底接管)", () => {
    const el = document.createElement("div");
    el.className = "card-unknown-kind";
    expect(estimateStreamNodeHeight(el)).toBeNull();
  });
});

describe("applyIntrinsicSize(F39 复用的契约面)", () => {
  it("写入格式 auto <N>px、四舍五入、**不夹取**(地板已去掉)", () => {
    const el = document.createElement("div");
    el.className = "card card-user";
    el.innerHTML = '<div class="card-body">hi</div>';
    applyIntrinsicSize(el);
    const v = el.style.getPropertyValue("contain-intrinsic-size");
    expect(v).toMatch(/^auto \d+px$/);
    // 🔴 这一条 2026-09-18 之前断的是 `>= 24`(`applyIntrinsicSize` 里那个 `Math.max(24, …)` 地板)。
    //   地板已整个去掉(`99 条 75` / `设计/17 订正④`,读数 `tests/evidence/S22-floor-readings.md`):
    //   它防的两件事都是空集(0 不可达、负值写不出来且浏览器自己会拒),而它让 13/83 张卡虚高,
    //   300 张细条卡的风暴里总高虚高 +20.5%/+23.8%。保险职责搬到判据层 ——
    //   秤 2 的 **F1**「每 class 估值最小值 ≥ 登记常数」会红,`Math.max` 只会静默抹平。
    //   ⇒ 这里从"断一个下限"改成"断**没有**下限":写进去的必须逐位 = round(估值)。
    expect(v).toBe(`auto ${Math.round(estimateStreamNodeHeight(el) as number)}px`);
    // 这张卡的估值是 21.7(LH_BASE = 14 × 1.55)⇒ 落地 22;旧地板下会被顶成 24
    expect(v).toBe("auto 22px");
  });

  it("**低于旧地板 24 的估值原样出货**(地板去掉之后这一条才成立)", () => {
    for (const [cls, expected] of [
      ["card card-api-retry", "auto 17px"], // 常数 17,旧地板下写的是 24
      ["card card-slash", "auto 19px"], //    常数 19,旧地板下写的是 24
      ["card card-bash-input", "auto 19px"], // 常数 19,旧地板下写的是 24
    ] as const) {
      const el = document.createElement("div");
      el.className = cls;
      applyIntrinsicSize(el);
      expect(el.style.getPropertyValue("contain-intrinsic-size"), cls).toBe(expected);
    }
  });
  it("估不出(null)时不写 style", () => {
    const el = document.createElement("div");
    el.className = "card-unknown-kind";
    applyIntrinsicSize(el);
    expect(el.style.getPropertyValue("contain-intrinsic-size")).toBe("");
  });
});
