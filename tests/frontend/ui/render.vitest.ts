// renderMarkdown 端到端（真 marked+katex，jsdom）：计划卡 · 计划格 · 说话人条 · 思考 · 工具结果的 MD 模式还在用它。
// 助手正文不走这里（后端交树）；块公式那几种写法（`\[..\]` · 段落紧挨着的 `$$` 块 · `\(..\)`）由 md-core 的金样钉。
import { describe, it, expect, vi } from "vitest";
import katex from "katex";
import { renderMessage } from "../../../src/frontend/ui/cards";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { enhanceCard, renderMarkdown } from "../../../src/frontend/ui/render";

describe("renderMarkdown 的公式（真 katex，jsdom）", () => {
  const hasKatex = (s: string): boolean => s.includes("katex");
  it("单行 $$a^2$$ → 渲染成 KaTeX", () => {
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

describe("游离 $$ 不吞正文", () => {
  it("行中 $$x$$（前后有正文）：整句留在同一个段落里，前后的正文都在", () => {
    const html = renderMarkdown("价格从 $$5 到 $$10 不等");
    const body = new DOMParser().parseFromString(html, "text/html").body;
    expect(body.children.length, html).toBe(1);
    expect(body.firstElementChild?.tagName, html).toBe("P");
    const text = body.textContent ?? "";
    for (const part of ["价格从", "10 不等"]) expect(text).toContain(part);
  });
});

/**
 * 「`currentLazy` 是模块单例，安全性依赖『同步调用栈』这条隐式不变量」。
 * 修法：急 / 惰是两个 `Marked` 实例，模块里没有任何可变状态可串。
 * 两格：① 源码顶层零 `let`（剥注释；正控：改之前那一行原文数得出 1）；② 两个实例交错调用互不串味。
 */
describe("D3 · 急 / 惰两个实例、模块零可变状态", () => {
  /** 剥注释后，数**顶格**（模块顶层）的 `let` 声明。 */
  const topLevelLets = (src: string): number =>
    src
      .replace(/\/\*[\s\S]*?\*\//g, "")
      .replace(/\/\/[^\n]*/g, "")
      .split("\n")
      .filter((l) => /^let\s/.test(l)).length;

  it("正控：改之前那一行原文数得出 1，缩进的局部 let 不算", () => {
    expect(topLevelLets("let currentLazy = false;\nfunction f() {\n  let x = 1;\n}\n")).toBe(1);
    expect(topLevelLets("// let a = 1;\n/* let b = 2; */\n")).toBe(0);
  });

  it("src/frontend/ui/render.ts 模块顶层零 `let`", () => {
    const src = readFileSync(resolve(__dirname, "../../../src/frontend/ui/render.ts"), "utf8");
    expect(src.length, "src/frontend/ui/render.ts 读出来是空的 —— 本条会零命中地绿").toBeGreaterThan(1000);
    expect(topLevelLets(src), "render.ts 又长出了模块级可变状态 —— ").toBe(0);
  });

  it("交错调用：急路恒同步高亮、惰路恒留占位", () => {
    const md = "```ts\nconst a = 1;\n```";
    for (let i = 0; i < 4; i++) {
      const lazy = renderMarkdown(md, { lazy: true });
      const eager = renderMarkdown(md);
      expect(lazy).toContain("code-pending");
      expect(lazy).not.toContain("hljs");
      expect(eager).toContain("hljs");
      expect(eager).not.toContain("code-pending");
    }
  });
});

/**
 * 「KaTeX 从不 lazy —— 含 `$$` 的长公式在重放期同步阻塞主线程
 * （代码高亮有 lazy，数学没有）」。修法：惰路只出占位，`enhanceCard` 进视口时补算。
 * 三格：① 惰路 ＋ 补算之后的 DOM 与急路逐字相同（异源：急路是 marked-katex-extension 自己的渲染器，
 * 补算是 `render.ts::enhanceMath`）；② 惰路期间 `katex.renderToString` 零次、补算次数 == 急路次数；
 * ③ 展开时才建的 thinking body 一律急路（不再留一块永远没人补的占位）。
 */
describe("D1 · 数学也 lazy", () => {
  const MATH = [
    "行内 $x_i^2$ 与 $$y=\\sqrt{2}$$ 同行",
    "结果是：\n\n$$\n\\begin{aligned}a&=b\\\\c&=d\\end{aligned}\n$$\n\n后文",
    "$$\nE=mc^2\n$$",
    "坏的 $\\frac{1}{$ 与 $$\\undefinedcmd$$",
    "costs $5 and $10 total",
    "- 列表里 $\\alpha$\n- 第二项 $$\\beta$$",
    "| a | $b$ |\n|---|---|\n| $1$ | 2 |",
  ];

  it("惰路 ＋ enhanceCard == 急路（逐字）", () => {
    let withPending = 0;
    for (const md of MATH) {
      const eager = document.createElement("div");
      eager.innerHTML = renderMarkdown(md);
      const lazy = document.createElement("div");
      lazy.innerHTML = renderMarkdown(md, { lazy: true });
      if (lazy.querySelector("[data-math-pending]")) withPending++;
      enhanceCard(lazy);
      expect(lazy.querySelector("[data-math-pending]"), md).toBeNull();
      expect(lazy.innerHTML, md).toBe(eager.innerHTML);
    }
    // 语料每一条都有惰路留的占位（「$5 and $10」那条按 nonStandard 也会被认成公式）
    expect(withPending).toBe(MATH.length);
  });

  it("惰路期间零次 renderToString；补算次数 == 急路次数", () => {
    const md = MATH.join("\n\n");
    const spy = vi.spyOn(katex, "renderToString");
    try {
      renderMarkdown(md);
      const eagerCalls = spy.mock.calls.length;
      spy.mockClear();
      const lazy = document.createElement("div");
      lazy.innerHTML = renderMarkdown(md, { lazy: true });
      expect(spy.mock.calls.length).toBe(0);
      enhanceCard(lazy);
      expect(eagerCalls).toBe(11); // 语料固定 ⇒ 急路算几次是定的
      expect(spy.mock.calls.length).toBe(eagerCalls);
    } finally {
      spy.mockRestore();
    }
  });

  it("展开时才建的 thinking body 走急路：零占位", () => {
    const res = renderMessage(
      {
        agent: "claude",
        t: "reply",
        id: "u1",
        at: "2026-01-01T00:00:00Z",
        blocks: [{ type: "thinking", text: "想想 $x^2$\n\n```ts\nconst a = 1;\n```" }],
        autoReply: false,
      } as never,
      { parentPath: "/p/s.jsonl", origin: "<local>", toolUseNames: new Map(), toolUseElements: new Map(), pendingToolResults: new Map(), lazy: true } as never,
    );
    expect(res.kind).toBe("steps");
    const d = (res as { units: HTMLElement[] }).units[0] as HTMLDetailsElement;
    d.open = true;
    d.dispatchEvent(new Event("toggle"));
    expect(d.querySelector(".katex")).not.toBeNull();
    expect(d.querySelector("[data-math-pending], .code-pending")).toBeNull();
  });
});

/**
 * IO 按滚动容器分（root 必填）。单元这一半：同 root 复用、不同 root 各一个、
 * 回调里补算并 unobserve、`releaseEnhanceRoot` 断开且之后再 observe 会新建。真渲染管线上的那一半在
 * `tests/frontend/ui/views/session-viewer-scroll.vitest.ts`「D2」。
 */
describe("D2 · 每个滚动容器一个 IO", () => {
  class FakeIO {
    static all: FakeIO[] = [];
    readonly observed = new Set<Element>();
    disconnected = false;
    constructor(
      readonly cb: IntersectionObserverCallback,
      readonly opts: IntersectionObserverInit = {},
    ) {
      FakeIO.all.push(this);
    }
    observe(el: Element): void {
      this.observed.add(el);
    }
    unobserve(el: Element): void {
      this.observed.delete(el);
    }
    disconnect(): void {
      this.disconnected = true;
    }
    fire(el: Element): void {
      this.cb([{ isIntersecting: true, target: el } as unknown as IntersectionObserverEntry], this as never);
    }
  }

  it("同 root 复用、不同 root 各一个；进视口即补算并 unobserve；release 之后新建", async () => {
    FakeIO.all = [];
    vi.stubGlobal("IntersectionObserver", FakeIO);
    try {
      const { observeForEnhance, releaseEnhanceRoot } = await import("../../../src/frontend/ui/render");
      const A = document.createElement("div");
      const B = document.createElement("div");
      const mk = (): HTMLElement => {
        const el = document.createElement("div");
        el.innerHTML = renderMarkdown("```ts\nconst a = 1;\n```", { lazy: true });
        return el;
      };
      const [a1, a2, b1] = [mk(), mk(), mk()];
      observeForEnhance(a1, A);
      observeForEnhance(a2, A);
      observeForEnhance(b1, B);
      expect(FakeIO.all.map((io) => io.opts.root)).toEqual([A, B]);
      const [ioA, ioB] = FakeIO.all;
      expect([...ioA.observed]).toEqual([a1, a2]);
      expect([...ioB.observed]).toEqual([b1]);
      ioA.fire(a1);
      expect(a1.querySelector(".code-pending")).toBeNull();
      expect(a2.querySelector(".code-pending")).not.toBeNull();
      expect([...ioA.observed]).toEqual([a2]);
      releaseEnhanceRoot(A);
      expect(ioA.disconnected).toBe(true);
      observeForEnhance(mk(), A);
      expect(FakeIO.all.length).toBe(3);
      expect(FakeIO.all[2].opts.root).toBe(A);
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("里面没有等着补的代码块 / 公式的卡不交给 IO（个个挂着 ＝ 滚动时每一帧都替它们算一遍交叉：300 轮的子运行 2495 个目标 → 299 个）", async () => {
    FakeIO.all = [];
    vi.stubGlobal("IntersectionObserver", FakeIO);
    try {
      const { observeForEnhance } = await import("../../../src/frontend/ui/render");
      const root = document.createElement("div");
      const plain = document.createElement("div");
      plain.innerHTML = renderMarkdown("只有字，没有代码块。", { lazy: true });
      const code = document.createElement("div");
      code.innerHTML = renderMarkdown("```ts\nconst a = 1;\n```", { lazy: true });
      const math = document.createElement("div");
      math.innerHTML = renderMarkdown("$$a^2$$", { lazy: true });
      observeForEnhance(plain, root);
      observeForEnhance(code, root);
      observeForEnhance(math, root);
      const io = FakeIO.all.find((x) => x.opts.root === root)!;
      expect([...io.observed]).toEqual([code, math]);
    } finally {
      vi.unstubAllGlobals();
    }
  });
});
