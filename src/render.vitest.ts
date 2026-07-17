// F73（issue #42）：多行块级 LaTeX 公式渲染。preprocessMath 纯函数（规整 + \[..\]/\(..\) 翻译 +
// 代码保护）+ renderMarkdown 端到端（真 marked+katex，jsdom）。
import { describe, it, expect } from "vitest";
import { preprocessMath, renderMarkdown } from "./render";

describe("F73 preprocessMath（纯函数：规整块公式 / 翻译 \\[..\\] / 保护代码）", () => {
  it("\\[ ... \\] → 块级 $$（独占行 + 空行包裹）", () => {
    const out = preprocessMath("\\[a=b\\]");
    expect(out).toContain("$$\na=b\n$$");
  });
  it("多行 \\[ ... \\] → 块级 $$", () => {
    const out = preprocessMath("\\[\n\\begin{aligned}a&=b\\end{aligned}\n\\]");
    expect(out).toContain("$$\n\\begin{aligned}a&=b\\end{aligned}\n$$");
  });
  it("\\( ... \\) → 行内 $（不是块级 $$）", () => {
    const out = preprocessMath("看 \\(x^2\\) 这里");
    expect(out).toContain("$x^2$");
    expect(out).not.toContain("$$");
  });
  it("块公式前无空行（会被段落吞）→ 规整出前置空行", () => {
    const out = preprocessMath("结果是：\n$$\nE=mc^2\n$$");
    // $$ 前必须有空行（\n\n），否则 marked 把它并进上一段落。
    expect(out).toContain("\n\n$$\nE=mc^2\n$$");
  });
  it("同行开的多行块（$$\\begin...\\end$$）→ 规整成独占行", () => {
    const out = preprocessMath("$$\\begin{aligned}\na&=b\n\\end{aligned}$$");
    expect(out).toContain("$$\n\\begin{aligned}\na&=b\n\\end{aligned}\n$$");
  });
  it("单行 $$a^2$$ → 规整（不回归）", () => {
    expect(preprocessMath("$$a^2$$")).toContain("$$\na^2\n$$");
  });
  it("代码围栏里的 $$/\\[ 不动（保护）", () => {
    const fenced = "```\n$$x$$ and \\[y\\]\n```";
    expect(preprocessMath(fenced)).toBe(fenced); // 逐字不变
  });
  it("行内代码里的 $$ 不动", () => {
    const inline = "用 `$$x$$` 表示";
    expect(preprocessMath(inline)).toBe(inline);
  });
});

describe("F73 renderMarkdown 端到端（真 katex，jsdom）", () => {
  const hasKatex = (s: string): boolean => s.includes("katex");
  it("多行块公式（前无空行）→ 渲染成 KaTeX（本 bug 核心）", () => {
    expect(hasKatex(renderMarkdown("结果是：\n$$\nE=mc^2\n$$"))).toBe(true);
  });
  it("\\[ ... \\] → 渲染成 KaTeX（issue #42 点名）", () => {
    expect(hasKatex(renderMarkdown("\\[a=b\\]"))).toBe(true);
  });
  it("\\( x^2 \\) 行内 → 渲染成 KaTeX", () => {
    expect(hasKatex(renderMarkdown("看 \\(x^2\\) 这里"))).toBe(true);
  });
  it("单行 $$a^2$$ → 仍渲染（不回归）", () => {
    expect(hasKatex(renderMarkdown("$$a^2$$"))).toBe(true);
  });
  it("行内 $x_i$ → 仍渲染（不回归）", () => {
    expect(hasKatex(renderMarkdown("变量 $x_i$ 值"))).toBe(true);
  });
  it("代码围栏里的 $$x$$ → 不渲染，保持字面", () => {
    const html = renderMarkdown("```\n$$x$$\n```");
    expect(hasKatex(html)).toBe(false);
    expect(html).toContain("$$x$$");
  });
});
