import { Marked, type MarkedExtension } from "marked";
import DOMPurify from "dompurify";
import hljs from "highlight.js/lib/common";
import katex from "katex";
import markedKatex from "marked-katex-extension";
import "highlight.js/styles/github-dark-dimmed.css";
import "katex/dist/katex.min.css";
import { copyText } from "./copy-table";

/**
 * v2.3.1 (issue #1 性能): lazy 模式 ——
 * - **急**（默认 / live 模式）：renderMarkdown 走全功能 pipeline（hljs 同步高亮）
 * - **惰**（batch 重放期）：marked + DOMPurify + KaTeX 同步出 HTML，但代码块
 *   hljs **不跑**——留 `<div class="code-block code-pending">` 占位。IntersectionObserver
 *   在卡片进可视区时调 enhanceCard 跑 hljs。
 *
 * 〔W5-RENDER R4 · `设计/10 §3.5` D1〕**数学也 lazy 了**。原来这里写着「为什么单独 lazy hljs 而不 lazy KaTeX」
 * 三条理由（hljs 是大头 · KaTeX 触发条件严 · 拆 lazy 复杂度高收益小）；D1 逐字「KaTeX 从不 lazy —— 含 `$$` 的
 * 长公式在重放期同步阻塞主线程」。现打：40 个块公式 ＋ 40 个行内公式的一条 7 KB 消息，惰路 `renderMarkdown`
 * 518 ms（jsdom，每个公式 ~6.5 ms），期间 `katex.renderToString` 80 次。拆 lazy 并不复杂：惰实例对
 * `inlineKatex` / `blockKatex` 两个 token 另挂一个同名渲染器（marked 的同名扩展后注册者先试），
 * 只出一个带 TeX 原文的占位，`enhanceCard` 进视口时再算。
 *
 * 〔W5-RENDER R3 · `设计/10 §3.5` D3〕急 / 惰是**两个 `Marked` 实例**，各带各的代码块渲染器；
 * 本模块里零可变状态。原来是一个全局 `marked` ＋ 一个模块级 `currentLazy` 标志、靠
 * 「同步调用栈里 save / restore」撑着 —— `设计/10 §3.5` D3 逐字「安全性依赖『同步调用栈』这条隐式不变量」。
 * 两个实例之间没有任何共享的开关，谁先谁后、谁嵌套谁都串不了。
 * 判据：`tests/frontend/ui/render.vitest.ts`「D3」一组（模块顶层零 `let` ＋ 两个实例互不串味）。
 */

/**
 * KaTeX 扩展：
 * - `$...$` 行内、`$$...$$` 块级
 * - `nonStandard: true` 才认 `$...$`（默认只认 `\(...\)`，README 示例那是误导）
 * - throwOnError: false → 错误 LaTeX 渲染成红色源码而不是抛异常
 */
const KATEX_OPTIONS = {
  throwOnError: false,
  nonStandard: true,
} as const;

/** 急路与 `enhanceCard` 补算共用的同一套清洗配置（补算出来的 KaTeX 片段必须过同一道）。 */
const SANITIZE_OPTIONS = {
  USE_PROFILES: { html: true, svg: true, mathMl: true },
  ADD_ATTR: ["target", "rel", "data-copy", "data-external"],
};

/**
 * 〔W5-RENDER R4〕惰实例的数学：不算，出占位 `<span data-math-pending="display|inline">转义后的 TeX</span>`
 * （块级末尾照原渲染器补 `\n`）。用 data 属性、不用类名：`tests/frontend/ui/css-ledger.vitest.ts` 的悬空类棘轮只许降。
 * `enhanceCard` 读 `textContent`（转义往返无损）与 `data-math-pending` 补算，补算后的 DOM 与急路逐字相同
 * （`tests/frontend/ui/render.vitest.ts`「D1」）。
 */
function mathPlaceholder(tex: string, displayMode: boolean, newlineAfter: boolean): string {
  return (
    `<span data-math-pending="${displayMode ? "display" : "inline"}">${escapeHtml(tex)}</span>` +
    (newlineAfter ? "\n" : "")
  );
}

const LAZY_MATH: MarkedExtension = {
  extensions: [
    {
      name: "inlineKatex",
      renderer: (token) => mathPlaceholder(String(token.text ?? ""), token.displayMode === true, false),
    },
    {
      name: "blockKatex",
      renderer: (token) => mathPlaceholder(String(token.text ?? ""), token.displayMode === true, true),
    },
  ],
};

/**
 * 代码块渲染：
 * 包一层 `<div class="code-block">`，含顶部小工具条（语言标签 + 复制按钮）
 * + `<pre><code class="hljs language-X">…</code></pre>` 高亮主体。
 * 复制按钮的 click handler 通过 main.ts 的全局事件代理处理。
 *
 * 仅引 `highlight.js/lib/common`（约 30 种主流语言）。
 */
// hljs.highlightAuto 会跑所有语言定义匹配最佳，10kB 无 lang 代码块单次 30-50ms。
// replay 大量代码块时累积秒级阻塞主线程（鼠标光标卡死的次要根因）。
// 改为：有显式 lang 才高亮；无 lang 直接转义当 plain text，保持代码块视觉但零开销。
/**
 * 代码块渲染（两个实例各一个）：
 * - **急**：现状路径，hljs 同步高亮
 * - **惰**：留占位 `<div class="code-block code-pending" data-lang="X">`，
 *   `<code class="language-X">` 内是 escape 过的纯文本，等 enhanceCard 时跑 hljs
 *
 * 占位也写完整 code-block / code-bar DOM 结构，让 CSS / 复制按钮立刻能 work。
 */
function codeRenderer(lazy: boolean): MarkedExtension {
  return {
    renderer: {
      code(token) {
        const lang = (token.lang ?? "").trim().split(/\s+/)[0];
        const code = token.text ?? "";

        if (lazy) {
          // lazy 路径：转义即可，hljs 留给 enhanceCard
          const cls = lang ? `language-${lang}` : "";
          const langLabel = lang || "text";
          return (
            `<div class="code-block code-pending"${lang ? ` data-lang="${escapeHtml(lang)}"` : ""}>` +
            `<div class="code-bar">` +
            `<span class="code-lang">${escapeHtml(langLabel)}</span>` +
            `<button type="button" class="code-copy" data-copy>${copyText("render.codeBlock.copy")}</button>` +
            `</div>` +
            `<pre><code class="${cls}">${escapeHtml(code)}</code></pre>` +
            `</div>`
          );
        }

        // 默认路径：同步 hljs
        let highlighted: string;
        try {
          if (lang && hljs.getLanguage(lang)) {
            highlighted = hljs.highlight(code, {
              language: lang,
              ignoreIllegals: true,
            }).value;
          } else {
            // 不再 highlightAuto —— 改为转义后原样输出
            highlighted = escapeHtml(code);
          }
        } catch {
          highlighted = escapeHtml(code);
        }
        const cls = lang ? `language-${lang} hljs` : "hljs";
        const langLabel = lang || "text";
        return (
          `<div class="code-block">` +
          `<div class="code-bar">` +
          `<span class="code-lang">${escapeHtml(langLabel)}</span>` +
          `<button type="button" class="code-copy" data-copy>${copyText("render.codeBlock.copy")}</button>` +
          `</div>` +
          `<pre><code class="${cls}">${highlighted}</code></pre>` +
          `</div>`
        );
      },
    },
  };
}

// v2.4.3 issue #13: 外链由系统默认浏览器打开。renderer 阶段给 http/https/mailto
// 链接打 data-external 标记 + target=_blank + rel=noopener noreferrer；main.ts
// 全局 click delegation 捕获 data-external 调 openUrl(). 相对路径 / 锚点保留
// 默认行为（默认就是站内导航，无 target）。
const LINK_RENDERER: MarkedExtension = {
  renderer: {
    link(token) {
      const href = token.href ?? "";
      const title = token.title ?? "";
      const text = this.parser.parseInline(token.tokens);
      const isExternal = /^(https?:|mailto:)/i.test(href);
      const safeHref = escapeHtml(href);
      const titleAttr = title ? ` title="${escapeHtml(title)}"` : "";
      if (isExternal) {
        return `<a href="${safeHref}"${titleAttr} target="_blank" rel="noopener noreferrer" data-external>${text}</a>`;
      }
      return `<a href="${safeHref}"${titleAttr}>${text}</a>`;
    },
  },
};

// #71：GFM strikethrough 规范允许**单**波浪号(`~x~`),marked 默认据此把成对单 `~` 之间的字划成
// `<del>`——静默改内容。触发要**闭合 `~` 贴非空白**(GFM flanking):`见 ~/foo~/bar`、`cd ~/a~/b`、
// `a ~foo~ b` 会被划;而 `~/.claude … ~/.codex` 因第二个 `~` 前是空格、flanking 已挡、**反而不触发**
// (此前注释举这个例子是错的)。覆盖内建 `del` tokenizer:**只认双波浪号 `~~x~~`**;单 `~` 返回
// **`undefined`**(★必须 undefined、**不能 `false`**——marked 的 `use()` 包装只在 `=== false` 时回退内建
// del、会复活单 `~` 把本修复静默还原)→ 词法器把单 `~` 当普通文本。真·删除线 `~~text~~` 仍渲染。
// (`~~~` 围栏是块级,由 preprocessMath/marked 代码 tokenizer 处理,不进本行内路径。)
const DEL_TOKENIZER: MarkedExtension = {
  tokenizer: {
    del(src: string) {
      // 要求 `~~` 紧邻非空白（GFM:定界符不与内容间留空）,内容非贪婪、结尾非空白。
      const m = /^~~(?=\S)([\s\S]*?\S)~~/.exec(src);
      if (!m) return undefined; // 单 `~` / 未配对 `~~` → 交回词法器当普通文本
      return {
        type: "del",
        raw: m[0],
        text: m[1],
        tokens: this.lexer.inlineTokens(m[1]),
      };
    },
  },
};

/**
 * 建一个实例：两个实例的扩展与顺序**逐项相同**（KaTeX → 代码块 → 外链 → `del`，与原来全局 `marked.use` 的顺序一致），
 * 只有代码块渲染器按 `lazy` 分叉；惰实例在 KaTeX 之后多挂一层同名数学渲染器（`LAZY_MATH`，出占位）。
 */
function makeMarked(lazy: boolean): Marked {
  return new Marked(
    { gfm: true, breaks: false },
    markedKatex(KATEX_OPTIONS),
    ...(lazy ? [LAZY_MATH] : []),
    codeRenderer(lazy),
    LINK_RENDERER,
    DEL_TOKENIZER,
  );
}

const EAGER = makeMarked(false);
const LAZY = makeMarked(true);

export interface RenderMarkdownOptions {
  /** true = lazy hljs（启动 batch 期间用，避免 N 个代码块同步阻塞主线程） */
  lazy?: boolean;
}

/**
 * 基础 Markdown 渲染：GFM + KaTeX + 代码高亮 + sanitize。
 *
 * P2.3：opts.lazy 显式传入，用 save/restore 模式避免"另一个 caller 同时调时被
 * 错误共享"（session-viewer 历史 load 期间被 tabs batch 模式污染走 lazy 路径
 * 是已知 bug）。同步调用栈内 try/finally 严格恢复，并发也安全（JS 单线程）。
 */
/**
 * F73（issue #42）：多行块级 LaTeX 公式预处理。`marked-katex-extension` 的块规则要求 `$$` **独占
 * 行**、且块扩展无 `start()` 钩子会被段落**吞并**（块公式前无空行时）、且**完全不认 `\[...\]`**——
 * 导致「结果是：<换行>$$…多行…$$」这类最常见形态不渲染（单行 `$$x$$` 走行内规则不受影响，故
 * "单行能、多行不能"的错觉）。在 `marked.parse` 前把块公式规整成扩展唯一能吃的形态（`$$` 独占行
 * + 前后空行），并把 `\[..\]`→`$$..$$`、`\(..\)`→`$..$`。**先保护代码围栏/行内代码**（别动代码里
 * 的 `$$`/`\[`）。纯函数、可单测。占位符用不可见 `\u0000` 包裹防与正文串位。
 * 已知边界：4 空格缩进代码块不保护（Claude 输出几乎只用围栏）；prose 里恰好配对的 `$$…$$`（如
 * "from $$5 to $$10"）会误判为公式——与既有 `nonStandard:true` 对单 `$` 的误判同源、不新增暴露面。
 */
export function preprocessMath(md: string): string {
  // `设计/17 §2.6` 前置闸。见 `needsMathPreprocess` 头注。
  if (!needsMathPreprocess(md)) return md;
  return preprocessMathUnguarded(md);
}

/**
 * `设计/17 §2.6`：**六遍全文正则的前置闸**。
 *
 * 为什么值：`render.ts` 自己在上面写着「多数消息不含 LaTeX」，而
 * `preprocessMathUnguarded` 里那六遍 `replace` 是**无条件**从头扫到尾的。
 * 【现打】44 万字符 **5.81 ms**（`设计/17 §2.6`）⇒ 绝大多数调用从六遍全文正则
 * 降到最多五次 `indexOf`。读数见 `tests/evidence/W2-17s7-readings.md`。
 *
 * 闸认的记号 = 下面那六遍**唯一可能改到东西**的入口，逐条对得上：
 *  - `\r` ⇒ 第 1 遍（CRLF/CR 归一）
 *  - `\[` / `\(` ⇒ 第 2 遍（翻译成 `$$`/`$`）
 *  - `$` ⇒ 第 3 遍（块级 `$$` 规整）
 *  - 第 2a/2b 遍（代码围栏 / 行内 code 打 stub）与第 4 遍（还原）是**一对**：
 *    中间那两遍没东西可改时，打 stub 再原样还原**净效果是恒等** ⇒ 代码记号
 *    （``` ``` ``` / `~~~` / 反引号）**不必**进闸。
 *  - 🔴 `\u0000` ⇒ **规格那 4 条之外、本轮现打补的第 5 条**。第 4 遍的还原正则认的是
 *    `\u0000M<数字>\u0000`；正文里若**本来就**带着这个串，慢路会把它替换成
 *    `stash[i]`（多半是 `undefined`，或者错位成别人的代码块）。那是一条**既存**缺陷，
 *    本轮**不修**（它与本条改动无关，修它要改语义）。把它放进闸里，只是为了让
 *    「**走快路 ⇒ 逐字节等于走慢路**」这句话**无条件成立** —— 判据钉的就是这句，
 *    留个例外它就只能钉一半。
 *
 * 导出仅为单测：判据要能**独立**数出「语料里几条走了快路」，不能拿被测函数自己的
 * 返回值反推（那样恒等两侧同源，会恒真）。
 */
export function needsMathPreprocess(md: string): boolean {
  return (
    md.indexOf("$") >= 0 ||
    md.indexOf("\\[") >= 0 ||
    md.indexOf("\\(") >= 0 ||
    md.indexOf("\r") >= 0 ||
    md.indexOf("\u0000") >= 0
  );
}

/**
 * 闸之前的**原样实现**（`设计/17 §2.6` 只加闸、一个字节都没动这里面）。
 * 导出仅为单测：判据拿它当「慢路」的对照组，逐条对拍快路的返回值。
 */
export function preprocessMathUnguarded(md: string): string {
  // #42:先把 CRLF/CR 归一成 LF——preprocessMath 跑在 marked 内部换行归一**之前**,下面所有基于 `\n`
  // 的块规则/代码保护才对 Windows(`\r\n`)行尾可靠(否则 `$$\r\n…` 的块识别不出、露字面 `$$`)。
  md = md.replace(/\r\n?/g, "\n");
  const stash: string[] = [];
  const stub = (m: string): string => {
    stash.push(m);
    return `\u0000M${stash.length - 1}\u0000`;
  };
  // 1) 保护代码：围栏（``` / ~~~）+ 行内 `code`——避免动到代码里的 $$ / \[。
  md = md.replace(/(^|\n)(```|~~~)[\s\S]*?\n\2[^\n]*(?=\n|$)/g, (m) => stub(m));
  md = md.replace(/`[^`\n]*`/g, (m) => stub(m));
  // 2) \[ ... \] → 块级 $$；\( ... \) → 行内 $。
  md = md.replace(/\\\[([\s\S]*?)\\\]/g, (_m, x: string) => `\n\n$$\n${x.trim()}\n$$\n\n`);
  md = md.replace(/\\\(([\s\S]*?)\\\)/g, (_m, x: string) => `$${x.trim()}$`);
  // 3) 块级 $$ ... $$ 统一规整成独占行 + 空行包裹。#42:一个 `$$` 只有**贴着行边界**才算**块定界符**
  //    ——开定界符 = 行首(`(^|\n)[ \t]*$$`)**或**其后紧跟换行(`$$[ \t]*\n`);闭定界符 = 行尾
  //    (`$$[ \t]*(\n|$)`)**或**其前紧接换行(`\n[ \t]*$$`)。据此把散文里游离/未配对的 `$$`(漏闭合、或
  //    "用 $$ 包裹显示公式"这类元讨论:`$$` 前后都是正文、不贴行边界)排除;否则原全局按数配对器遇**奇数
  //    `$$`** 会错位、吞掉后一个真公式的开 `$$`、把散文当公式渲染并**丢真公式**(比留字面更糟)。
  //    "开=行首或后接换行 / 闭=行尾或前接换行"修掉首版"须行首/行尾"过严的回归(CRLF已归一、`文字：$$⏎…`
  //    开前有字、`…⏎$$。`闭后有标点)。行中 `$$x$$`(前后有正文)不匹配、留 marked-katex 行内。去掉 `[^$]`
  //    守卫(误伤 `$$$…`)。lookbehind 需 V8/Chromium(WebView2 ✓;Node/vitest ✓)。
  md = md.replace(
    /(?:(?<=^|\n)[ \t]*\$\$|\$\$(?=[ \t]*\n))([\s\S]*?)(?:(?<=\n[ \t]{0,32})\$\$|\$\$(?=[ \t]*(?:\n|$)))/g,
    (_m, x: string) => `\n\n$$\n${x.trim()}\n$$\n\n`,
  );
  // 4) 还原代码。
  return md.replace(/\u0000M(\d+)\u0000/g, (_m, i: string) => stash[Number(i)]);
}

export function renderMarkdown(md: string, opts: RenderMarkdownOptions = {}): string {
  // lazy 必须 caller 显式传（不传默认 false）；〔W5-RENDER R3〕选实例，不再改任何共享状态。
  const m = opts.lazy ? LAZY : EAGER;
  // F73：数学预处理（多行块公式规整 + \[..\]/\(..\) 翻译）后再交给 marked。
  const raw = m.parse(preprocessMath(md), { async: false }) as string;
  return DOMPurify.sanitize(raw, SANITIZE_OPTIONS);
}

/** 纯文本（用户消息保守模式）：转义 + 保留换行 */
export function renderPlainText(text: string): string {
  const div = document.createElement("div");
  div.textContent = text;
  return div.innerHTML.replace(/\n/g, "<br>");
}

function escapeHtml(s: string): string {
  const div = document.createElement("div");
  div.textContent = s;
  return div.innerHTML;
}

/**
 * v2.3.1 (issue #1) Phase 2：找卡片内**剩下没处理的代码块** (`.code-block.code-pending`)
 * 跑 hljs 高亮。idempotent — 标 `data-enhanced` 防重复。
 *
 * 没 pending 时是 fast path：单次 querySelector 找不到东西后立即标记返回。
 *
 * 〔W5-RENDER R4〕**也补数学**：惰路留下的 `[data-math-pending]` 占位在这里算 KaTeX、过同一道清洗、原位替换
 * （原来这里写「不处理 LaTeX：KaTeX 在 markedKatex 扩展里同步处理过了」—— D1 之后不再成立）。
 */
export function enhanceCard(el: HTMLElement): void {
  if (el.dataset.enhanced === "1") return;
  el.dataset.enhanced = "1";

  const pendings = el.querySelectorAll<HTMLElement>(".code-block.code-pending, [data-math-pending]");
  if (pendings.length === 0) return; // fast path: 该卡片没代码块、没公式

  for (const block of pendings) {
    if (block.hasAttribute("data-math-pending")) {
      enhanceMath(block);
      continue;
    }
    const lang = block.dataset.lang ?? "";
    const codeEl = block.querySelector("code");
    if (!codeEl) {
      block.classList.remove("code-pending");
      continue;
    }
    if (lang && hljs.getLanguage(lang)) {
      try {
        const raw = codeEl.textContent ?? "";
        codeEl.innerHTML = hljs.highlight(raw, {
          language: lang,
          ignoreIllegals: true,
        }).value;
        codeEl.classList.add("hljs");
      } catch (e) {
        console.warn("[enhance] hljs failed:", e);
      }
    } else {
      // 无 lang 或 lang unknown：仍 escape 文本不变，只去掉 pending 标记
      codeEl.classList.add("hljs");
    }
    block.classList.remove("code-pending");
  }
}

/** 〔W5-RENDER R4〕一个数学占位 → KaTeX（与急路同一组选项、同一道清洗），原位替换。 */
function enhanceMath(ph: HTMLElement): void {
  const displayMode = ph.getAttribute("data-math-pending") === "display";
  const tex = ph.textContent ?? "";
  let html: string;
  try {
    html = katex.renderToString(tex, { ...KATEX_OPTIONS, displayMode });
  } catch (e) {
    // throwOnError:false 下不该走到这里；真走到就留着原文、去掉标记（不反复重试）
    console.warn("[enhance] katex failed:", e);
    ph.removeAttribute("data-math-pending");
    return;
  }
  const tpl = document.createElement("template");
  tpl.innerHTML = DOMPurify.sanitize(html, SANITIZE_OPTIONS);
  ph.replaceWith(...Array.from(tpl.content.childNodes));
}

/**
 * 〔W5-RENDER R5 · `设计/10 §3.5` D2〕**每个滚动容器一个** IntersectionObserver（root = 那个容器）：
 * 观察容器内的卡片，进可视区（± 300px）调 enhanceCard，然后 unobserve（一次性，不来回触发）。
 *
 * 原来是**一个**模块级 IO、没有 root ⇒ 量的是浏览器视口，而真正的滚动容器是 `.stream` —— D2 逐字
 * 「查看器里几何不同 ⇒ lazy 高亮触发时机不可靠」；主窗里各 tab 的 `.stream` 叠在同一块区域（后台 tab 是
 * `visibility:hidden`，仍有几何），视口 root 分不清卡属于哪一条流。
 *
 * - root 必填（类型上就交不出「没有 root」）；同一个 root 复用一个 IO（`WeakMap`，root 被摘掉即可回收）。
 * - 容器销毁时调 {@link releaseEnhanceRoot} 断开它的 IO（`TabStreamView.disposeTab` · `SessionViewer`）。
 * - 没有 IO 的环境（jsdom / 极老浏览器）⇒ 退化到立即 enhance。
 * rootMargin: 300px 让卡片在快滚到时就预先 enhance，避免视觉看到 hljs "弹"出来。
 */
const enhanceObservers = new WeakMap<HTMLElement, IntersectionObserver>();

/**
 * 让一个卡片接受 lazy enhance 调度（`root` = 它所在的滚动容器）。实时 tab 与查看器在 lazy 渲染期间挂卡片时调。
 */
export function observeForEnhance(el: HTMLElement, root: HTMLElement): void {
  if (typeof IntersectionObserver === "undefined") {
    enhanceCard(el);
    return;
  }
  let io = enhanceObservers.get(root);
  if (!io) {
    io = new IntersectionObserver(
      (entries, obs) => {
        for (const e of entries) {
          if (e.isIntersecting && e.target instanceof HTMLElement) {
            enhanceCard(e.target);
            obs.unobserve(e.target);
          }
        }
      },
      { root, rootMargin: "300px" },
    );
    enhanceObservers.set(root, io);
  }
  io.observe(el);
}

/** 滚动容器销毁：断开它那一个 IO（还没进过视口的卡不再补，容器都没了）。 */
export function releaseEnhanceRoot(root: HTMLElement): void {
  const io = enhanceObservers.get(root);
  if (!io) return;
  io.disconnect();
  enhanceObservers.delete(root);
}
