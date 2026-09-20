// F73（issue #42）：多行块级 LaTeX 公式渲染。preprocessMath 纯函数（规整 + \[..\]/\(..\) 翻译 +
// 代码保护）+ renderMarkdown 端到端（真 marked+katex，jsdom）。
import { describe, it, expect } from "vitest";
import {
  needsMathPreprocess,
  preprocessMath,
  preprocessMathUnguarded,
  renderMarkdown,
} from "../src/render";

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

/**
 * `设计/17 §7` 第 3 条 ＝ `§2.6`：**`preprocessMath` 前置闸**。
 *
 * # 它钉的那条链
 *
 * `preprocessMathUnguarded` 里那六遍 `replace` 是**无条件**从头扫到尾的（O(6·len) 时间、
 * O(len) 空间 × 多份峰值），而 `render.ts` 自己在上面写着「多数消息不含 LaTeX」。
 * 【`设计/17 §2.6` 现打】44 万字符 **5.81 ms**。⇒ 加一道单遍前置闸。
 *
 * # 判据钉什么 —— **不钉「快了多少」，钉「快路与慢路逐字节相同」**
 *
 * 「省了几毫秒」在 CI 上不可判（机器噪声）；可判的是**等价性**，而那正是这条改动
 * 唯一的风险面：闸判错 ⇒ 该跑六遍的没跑 ⇒ 公式静默不渲染。
 *
 * 三条咬在一起（缺一条另外两条都会变成空真）：
 *  - **量具自检**：语料里「该走快路」与「该走慢路」的条数各自 == 登记数，且都 > 0。
 *    全语料都走慢路的话，下面那条等价断言两边就是同一份代码 ⇒ 恒真。
 *  - **闸的判定** == 手写登记的期望（不是拿被测函数的返回值反推）。
 *  - **等价**：`preprocessMath(s)` 逐字节 == `preprocessMathUnguarded(s)`；
 *    且走快路的那些**逐字节 == 输入本身**。
 *  - **阳性对照**：慢路必须真的改到过东西（`unguarded(s) !== s` 的条数 == 登记数、> 0），
 *    否则「两边相等」在一个恒等的 `unguarded` 上也成立。
 *
 * # 它挡不住什么（诚实段）
 *
 * 语料是**手写登记**的，不是真机语料 ⇒ 它逮得住「闸漏了某个已登记的记号」，
 * 逮不住「有第六种记号能让慢路改到东西、而我们没想到」。后者的兜底是
 * `needsMathPreprocess` 头注里那份「闸认的记号 ↔ 六遍替换的入口」逐条对照 —— 那是**散文**，
 * 不是判据。真要钉死它得对 `unguarded` 做属性测试（随机语料上 `!needs(s) ⇒ unguarded(s)===s`），
 * 那超出本条「当天可上」的射程，**如实记在这里**。
 */
describe("`设计/17 §2.6` preprocessMath 前置闸：快路必须与慢路逐字节相同", () => {
  /**
   * 登记语料。`slow` 逐条**手写**：这一条该不该走那六遍全文正则。
   * `changes` 逐条手写：慢路在这一条上到底改没改到东西（阳性对照用）。
   */
  const CORPUS: ReadonlyArray<{ name: string; md: string; slow: boolean; changes: boolean }> = [
    // —— 该走快路的（一个数学记号都没有）——
    { name: "纯 CJK 散文", md: "这一段里一个数学记号都没有。", slow: false, changes: false },
    { name: "空串", md: "", slow: false, changes: false },
    {
      name: "带代码围栏（stub＋还原净效果必须是恒等）",
      md: "前面一句\n\n```ts\nconst a = [1, 2];\nfn(x);\n```\n\n后面一句",
      slow: false,
      changes: false,
    },
    {
      name: "带 ~~~ 围栏 ＋ 行内 code",
      md: "看 `fn(x)` 与 `arr[0]`：\n\n~~~py\nd = {'k': [1]}\n~~~\n",
      slow: false,
      changes: false,
    },
    {
      name: "markdown 表格 ＋ 列表（大量源码换行，最常见的长正文形态）",
      md: "| a | b |\n|---|---|\n| 1 | 2 |\n\n- 一\n- 二\n  - 三\n",
      slow: false,
      changes: false,
    },
    { name: "反斜杠但不是 \\[ / \\(", md: "路径 C:\\Users\\x 与 \\n \\t", slow: false, changes: false },
    { name: "方括号链接（[] 不带反斜杠）", md: "见 [文档](https://example.invalid/a[b])", slow: false, changes: false },

    // —— 该走慢路的（四个规格记号 ＋ 现打补的第五个）——
    // ⚠ 现打订正：这一条登记时手写成 `changes: true`，**阳性对照当场把它判红**。
    // 单个 `$…$` 走的是 `marked-katex-extension` 的**行内**规则，`preprocessMath`
    // 六遍里没有一遍碰它 ⇒ 慢路在这一条上是恒等。它仍然**必须走慢路**（闸不敢放行：
    // 放行就等于断言「`$` 永远不影响输出」，而 `$$` 块规则就住在同一个记号上）。
    { name: "$ 行内公式", md: "行内 $x_i$ 一个", slow: true, changes: false },
    { name: "$$ 块公式（前无空行）", md: "结果是：\n$$\nE=mc^2\n$$", slow: true, changes: true },
    { name: "\\[ 块公式", md: "\\[a=b\\]", slow: true, changes: true },
    { name: "\\( 行内公式", md: "\\(x^2\\)", slow: true, changes: true },
    { name: "CRLF 行尾（第 1 遍会归一）", md: "一行\r\n二行\r\n", slow: true, changes: true },
    {
      name: "🔴 \\u0000 占位符串（规格那 4 条之外、本轮现打补的第 5 条）",
      md: "正文里本来就带着 \u0000M0\u0000 这个串",
      slow: true,
      // 既存缺陷：慢路会把它当成 stub 去还原（stash 是空的 ⇒ 替成 "undefined"）。
      // 本轮**不修**，只是把它关进闸里，好让「快路 ⇒ 逐字节等于慢路」无条件成立。
      changes: true,
    },
    {
      name: "带 $ 但代码围栏保护住了（走慢路、慢路不改它）",
      md: "```sh\necho $HOME\n```\n",
      slow: true,
      changes: false,
    },
  ];

  /** 量具自检的登记值。语料一改这三个数就要跟着改 —— 那正是想要的。 */
  const EXPECT_FAST = 7;
  const EXPECT_SLOW = 7;
  const EXPECT_SLOW_CHANGES = 5;

  it("★ 量具自检：语料两侧都非空，且条数 == 登记数", () => {
    const fast = CORPUS.filter((c) => !c.slow).length;
    const slow = CORPUS.filter((c) => c.slow).length;
    expect(
      { fast, slow },
      "语料塌了一侧 ⇒ 下面那条「快路 == 慢路」的等价断言会变成拿同一份代码对拍自己（恒真）。",
    ).toEqual({ fast: EXPECT_FAST, slow: EXPECT_SLOW });
    expect(fast, "没有任何一条走快路 ⇒ 这一整组判据零命中地绿").toBeGreaterThan(0);
    expect(slow, "没有任何一条走慢路 ⇒ 等价断言两边同源").toBeGreaterThan(0);
  });

  it("★ 阳性对照：慢路在语料上真的改到过东西（== 登记数）", () => {
    const changed = CORPUS.filter((c) => preprocessMathUnguarded(c.md) !== c.md);
    expect(
      changed.map((c) => c.name).sort(),
      "慢路一条都没改到东西（或改到的条目与登记对不上）⇒ " +
        "「快路 == 慢路」在一个恒等的慢路上也成立，那条断言什么都没证明。",
    ).toEqual(
      CORPUS.filter((c) => c.changes)
        .map((c) => c.name)
        .sort(),
    );
    expect(changed.length).toBe(EXPECT_SLOW_CHANGES);
  });

  it("★ 闸的判定逐条 == 手写登记（不从被测函数的返回值反推）", () => {
    const actual = CORPUS.map((c) => ({ name: c.name, slow: needsMathPreprocess(c.md) }));
    expect(
      actual,
      "闸判错了：判成快路而慢路其实会改东西 ⇒ 公式静默不渲染；判成慢路只是白跑六遍。",
    ).toEqual(CORPUS.map((c) => ({ name: c.name, slow: c.slow })));
  });

  it("★ 逐条：`preprocessMath` 与 `preprocessMathUnguarded` 输出**逐字节相同**", () => {
    const diff = CORPUS.filter((c) => preprocessMath(c.md) !== preprocessMathUnguarded(c.md)).map(
      (c) => c.name,
    );
    expect(diff, "前置闸改变了输出 —— 它只许省时间，不许省语义").toEqual([]);
  });

  it("★ 走快路的那几条：输出**就是输入本身**（`return md`，零拷贝零改动）", () => {
    const bad = CORPUS.filter((c) => !c.slow && preprocessMath(c.md) !== c.md).map((c) => c.name);
    expect(bad, "闸放行了却还是改了东西 —— 那条快路不是恒等").toEqual([]);
  });

  /**
   * 读数（**不进门禁**）：闸省掉的那六遍到底值多少毫秒。
   *
   * 复算：`W2_COST=1 npx vitest run tests/render.vitest.ts`
   * 默认跳过 —— wall time 在 CI 上是噪声，**当断言用就是一条会自己红的假判据**。
   * 它只产读数，读数落 `tests/evidence/W2-17s7-readings.md`。
   */
  it.skipIf(!process.env.W2_COST)("读数：44 万字符无公式正文，慢路 vs 快路的 wall time", () => {
    // 结构照真的、内容合成（`设计/17 §6` 数据源纪律）：CJK 段落 + 代码围栏 + 表格，
    // 一个数学记号都没有 —— 这正是「多数消息」的形状。
    const para = "这是一段没有任何数学记号的中文正文，用来把长度堆到长尾那一档。";
    const block = `${para.repeat(8)}\n\n\`\`\`ts\nconst a = [1, 2, 3];\nfn(a);\n\`\`\`\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n`;
    let md = "";
    while (md.length < 440_000) md += block;
    expect(needsMathPreprocess(md), "语料里混进了数学记号 ⇒ 这份读数量的不是快路").toBe(false);

    const N = 20;
    const t0 = performance.now();
    for (let i = 0; i < N; i++) preprocessMathUnguarded(md);
    const slow = (performance.now() - t0) / N;
    const t1 = performance.now();
    for (let i = 0; i < N; i++) preprocessMath(md);
    const fast = (performance.now() - t1) / N;
    console.log(
      `[§2.6 读数] len=${md.length} · 慢路（六遍全文正则）${slow.toFixed(3)} ms/次 · ` +
        `快路（最多五次 indexOf）${fast.toFixed(3)} ms/次 · 省 ${(slow - fast).toFixed(3)} ms（${(
          (1 - fast / slow) *
          100
        ).toFixed(1)}%）`,
    );
  });

  it("★ 端到端不回归：带 $ 的仍渲染成 KaTeX，不带 $ 的照常出 HTML", () => {
    expect(renderMarkdown("行内 $x_i$ 一个").includes("katex")).toBe(true);
    const plain = renderMarkdown("这一段里一个数学记号都没有。");
    expect(plain.includes("katex")).toBe(false);
    expect(plain.includes("这一段里一个数学记号都没有")).toBe(true);
  });
});
