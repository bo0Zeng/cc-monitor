import { marked } from "marked";
import DOMPurify from "dompurify";
import hljs from "highlight.js/lib/common";
import markedKatex from "marked-katex-extension";
import "highlight.js/styles/github-dark-dimmed.css";
import "katex/dist/katex.min.css";

marked.setOptions({
  gfm: true,
  breaks: false,
});

/**
 * v2.3.1 (issue #1 性能): lazy 模式 ——
 * - **false**（默认 / live 模式）：renderMarkdown 走全功能 pipeline（hljs 同步高亮）
 * - **true**（batch 重放期）：marked + DOMPurify + KaTeX 同步出 HTML，但代码块
 *   hljs **不跑**——留 `<div class="code-block code-pending">` 占位。IntersectionObserver
 *   在卡片进可视区时调 enhanceCard 跑 hljs。
 *
 * 为什么单独 lazy hljs 而不 lazy KaTeX：
 * - 实测 hljs 是主要耗时（每代码块 0.5-5ms，含代码块的消息占 ~25%）
 * - KaTeX 触发条件严（只有 `$..$` 才会进 markedKatex），多数消息不含 LaTeX 跳过快
 * - markedKatex 是 marked 扩展深度集成的，拆 lazy 复杂度高，收益小
 *
 * P5.5 B 重构：lazy 完全走 caller 参数 `renderMarkdown(md, { lazy })`，删了原
 * setRenderLazyMode + currentLazy 全局 flag。模块内仍用 currentLazy 供 marked
 * code renderer 闭包访问（marked.use renderer 是 module singleton），但用
 * save/restore 模式同步调用栈内 try/finally 严格恢复——SessionViewer / Subagent
 * 不会被 tabs batch 期间错误共享 lazy 状态。
 */
let currentLazy = false;

/**
 * KaTeX 扩展：
 * - `$...$` 行内、`$$...$$` 块级
 * - `nonStandard: true` 才认 `$...$`（默认只认 `\(...\)`，README 示例那是误导）
 * - throwOnError: false → 错误 LaTeX 渲染成红色源码而不是抛异常
 */
marked.use(
  markedKatex({
    throwOnError: false,
    nonStandard: true,
  }),
);

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
 * 代码块渲染：
 * - **renderLazyMode = false**：现状路径，hljs 同步高亮
 * - **renderLazyMode = true**：留占位 `<div class="code-block code-pending" data-lang="X">`，
 *   `<code class="language-X">` 内是 escape 过的纯文本，等 enhanceCard 时跑 hljs
 *
 * 占位也写完整 code-block / code-bar DOM 结构，让 CSS / 复制按钮立刻能 work。
 */
marked.use({
  renderer: {
    code(token) {
      const lang = (token.lang ?? "").trim().split(/\s+/)[0];
      const code = token.text ?? "";

      if (currentLazy) {
        // lazy 路径：转义即可，hljs 留给 enhanceCard
        const cls = lang ? `language-${lang}` : "";
        const langLabel = lang || "text";
        return (
          `<div class="code-block code-pending"${lang ? ` data-lang="${escapeHtml(lang)}"` : ""}>` +
          `<div class="code-bar">` +
          `<span class="code-lang">${escapeHtml(langLabel)}</span>` +
          `<button type="button" class="code-copy" data-copy>复制</button>` +
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
        `<button type="button" class="code-copy" data-copy>复制</button>` +
        `</div>` +
        `<pre><code class="${cls}">${highlighted}</code></pre>` +
        `</div>`
      );
    },
  },
});

// v2.4.3 issue #13: 外链由系统默认浏览器打开。renderer 阶段给 http/https/mailto
// 链接打 data-external 标记 + target=_blank + rel=noopener noreferrer；main.ts
// 全局 click delegation 捕获 data-external 调 openUrl(). 相对路径 / 锚点保留
// 默认行为（默认就是站内导航，无 target）。
marked.use({
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
});

// #71：GFM strikethrough 规范允许**单**波浪号(`~x~`),marked 默认据此把成对单 `~` 之间的字划成
// `<del>`——静默改内容。触发要**闭合 `~` 贴非空白**(GFM flanking):`见 ~/foo~/bar`、`cd ~/a~/b`、
// `a ~foo~ b` 会被划;而 `~/.claude … ~/.codex` 因第二个 `~` 前是空格、flanking 已挡、**反而不触发**
// (此前注释举这个例子是错的)。覆盖内建 `del` tokenizer:**只认双波浪号 `~~x~~`**;单 `~` 返回
// **`undefined`**(★必须 undefined、**不能 `false`**——marked 的 `use()` 包装只在 `=== false` 时回退内建
// del、会复活单 `~` 把本修复静默还原)→ 词法器把单 `~` 当普通文本。真·删除线 `~~text~~` 仍渲染。
// (`~~~` 围栏是块级,由 preprocessMath/marked 代码 tokenizer 处理,不进本行内路径。)
marked.use({
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
});

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
  const prevLazy = currentLazy;
  // P5.5 B 重构：lazy 必须 caller 显式传（不传默认 false）；同步调用栈 save/restore
  // 防止 marked.use renderer 单例的 closure 看到错误 state。
  currentLazy = opts.lazy ?? false;
  try {
    // F73：数学预处理（多行块公式规整 + \[..\]/\(..\) 翻译）后再交给 marked。
    const raw = marked.parse(preprocessMath(md), { async: false }) as string;
    return DOMPurify.sanitize(raw, {
      USE_PROFILES: { html: true, svg: true, mathMl: true },
      ADD_ATTR: ["target", "rel", "data-copy", "data-external"],
    });
  } finally {
    currentLazy = prevLazy;
  }
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
 * **不处理 LaTeX**：KaTeX 在 markedKatex 扩展里同步处理过了，已经是最终 DOM。
 */
export function enhanceCard(el: HTMLElement): void {
  if (el.dataset.enhanced === "1") return;
  el.dataset.enhanced = "1";

  const pendings = el.querySelectorAll<HTMLElement>(".code-block.code-pending");
  if (pendings.length === 0) return; // fast path: 该卡片没代码块

  for (const block of pendings) {
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

/**
 * 全局 IntersectionObserver：观察 stream 内的卡片，进可视区调 enhanceCard，
 * 然后 unobserve（一次性，不来回触发）。
 *
 * rootMargin: 300px 让卡片在快滚到时就预先 enhance，避免视觉看到 hljs "弹"出来。
 */
const enhanceObserver =
  typeof IntersectionObserver !== "undefined"
    ? new IntersectionObserver(
        (entries) => {
          for (const e of entries) {
            if (e.isIntersecting && e.target instanceof HTMLElement) {
              enhanceCard(e.target);
              enhanceObserver?.unobserve(e.target);
            }
          }
        },
        { rootMargin: "300px" },
      )
    : null;

/**
 * 让一个卡片接受 lazy enhance 调度。TabManager 在 lazy 渲染期间挂卡片时调。
 * `enhanceObserver` 可能为 null（极老浏览器）→ 退化到立即 enhance。
 */
export function observeForEnhance(el: HTMLElement): void {
  if (enhanceObserver) {
    enhanceObserver.observe(el);
  } else {
    enhanceCard(el);
  }
}
