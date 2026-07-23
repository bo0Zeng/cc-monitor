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

describe("#71 单波浪号不当删除线（覆盖 GFM del tokenizer，只认 ~~）", () => {
  it("闭合 ~ 贴非空白（~/foo~/bar,stock marked 会划）→ 覆盖后不划、路径原样", () => {
    // ★区分性:第二个 ~ 前是 `o`(非空白),GFM flanking 会把 `/foo` 划成 <del>(未修则失败)。
    // (`~/.claude … ~/.codex` 因空格 flanking 本就不触发、区分不出——故不用它当断言。)
    const html = renderMarkdown("见 ~/foo~/bar 目录");
    expect(html).not.toContain("<del>");
    expect(html).toContain("~/foo~/bar");
  });
  it("成对 a ~foo~ b（stock 会划）→ 覆盖后不划", () => {
    expect(renderMarkdown("a ~foo~ b")).not.toContain("<del>");
  });
  it("真·删除线 ~~text~~ 仍渲染成 <del>（不误伤合法用法）", () => {
    expect(renderMarkdown("这是 ~~废弃~~ 的").includes("<del>")).toBe(true);
  });
});

describe("#42 奇数/游离 $$ 不吞掉真公式 + 行边界回归", () => {
  const countKatexDisplay = (s: string): number => (s.match(/katex-display/g) ?? []).length;
  it("两块真公式间的散文 $$（元讨论）:散文不被当块、第三块真公式成块（在 preprocess 层断言——DOMPurify 会剥离 KaTeX annotation,故不在渲染后 HTML 里验区分性）", () => {
    const md = "$$\na=b\nc=d\n$$\n用 $$ 包裹显示公式。\n$$\ne=f\ng=h\n$$";
    const pre = preprocessMath(md);
    // ★区分:散文"用 $$ 包裹显示公式。"原样保留(含字面 $$)——旧全局正则会把它误规整成 $$\n包裹…\n$$
    expect(pre).toContain("用 $$ 包裹显示公式。");
    // ★区分:第三块真公式被规整成独立块——旧正则错位配对会丢掉它(得不到 $$\ne=f\ng=h\n$$ 块)
    expect(pre).toContain("$$\ne=f\ng=h\n$$");
    // 端到端 sanity:至少两块渲染成 KaTeX display（katex-display 类过 DOMPurify 保留）
    expect(countKatexDisplay(renderMarkdown(md))).toBeGreaterThanOrEqual(2);
  });
  it("尾标点 $$…$$。→ 公式仍渲染（修首版行尾过严回归 重要3）", () => {
    expect(renderMarkdown("公式\n$$\na=b\n$$。").includes("katex")).toBe(true);
  });
  it("开定界前有字 文字：$$⏎…⏎$$ → 公式仍渲染（修首版行首过严回归 重要2）", () => {
    expect(renderMarkdown("答案是：$$\nE=mc^2\n$$").includes("katex")).toBe(true);
  });
  it("CRLF 行尾块公式 → 仍渲染（#42 重要1）", () => {
    expect(renderMarkdown("结果\r\n$$\r\nE=mc^2\r\n$$").includes("katex")).toBe(true);
  });
  it("行中 $$x$$（前后有正文）不被块规则误吞（不 throw、有输出）", () => {
    const html = renderMarkdown("价格从 $$5 到 $$10 不等");
    expect(typeof html).toBe("string");
    expect(html.length).toBeGreaterThan(0);
  });
});
