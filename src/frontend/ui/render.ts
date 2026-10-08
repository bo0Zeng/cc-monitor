import { Marked, type MarkedExtension } from "marked";
import DOMPurify from "dompurify";
import hljs from "highlight.js/lib/common";
import katex from "katex";
import markedKatex from "marked-katex-extension";
import "highlight.js/styles/github-dark-dimmed.css";
import "katex/dist/katex.min.css";
import { copyText } from "./copy-table";

/**
 * 两种渲染：
 * - 急（默认 / 实时）：全功能，代码块当场高亮、公式当场算；
 * - 惰（批量重放）：marked ＋ DOMPurify 出 HTML，代码块与公式只留占位（`code-pending` · `data-math-pending`），
 *   卡片进可视区时 `enhanceCard` 再补 —— 含几十个公式的长消息当场算要几百毫秒。
 *
 * 急 / 惰是两个 `Marked` 实例，各带各的渲染器，模块里零可变状态 ⇒ 谁先谁后、谁嵌套谁都串不了
 * （`tests/frontend/ui/render.vitest.ts`「D3」：模块顶层零 `let` ＋ 两个实例互不串味）。
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
 * 惰实例的数学：不算，出占位 `<span data-math-pending="display|inline">转义后的 TeX</span>`
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
            // 不认的语言不猜（不 highlightAuto）：转义后原样输出
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

// 外链由系统默认浏览器打开：http / https / mailto 链接打 data-external ＋ target=_blank ＋ rel=noopener noreferrer，
// main.ts 的全局 click 委托见 data-external 调 openUrl()。相对路径 / 锚点照默认（站内导航）。
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

// 删除线只认双波浪号 `~~x~~`：GFM 允许单 `~`，会把 `cd ~/a~/b` 这类路径静默划掉。
// 单 `~` 交回 `undefined`（不能是 `false`：marked 的 `use()` 包装见 `false` 会回退内建 del、又认单 `~`）。
// `~~~` 围栏是块级，不走这里。
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
 * 建一个实例：两个实例的扩展与顺序逐项相同（KaTeX → 代码块 → 外链 → `del`），
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

/** 基础 Markdown 渲染：GFM ＋ KaTeX ＋ 代码高亮 ＋ sanitize。`opts.lazy` 选惰实例。 */
/**
 * 多行块级公式预处理：`marked-katex-extension` 的块规则要 `$$` 独占一行、块前没空行会被段落吞掉、也不认 `\[...\]`。
 * 这里在 `marked.parse` 前把块公式规整成 `$$` 独占行 ＋ 前后空行，并把 `\[..\]` → `$$..$$`、`\(..\)` → `$..$`；先保护代码围栏与行内代码。
 * 边界：4 空格缩进的代码块不保护；正文里恰好配对的 `$$…$$`（"from $$5 to $$10"）会被当成公式。
 */
/** 代码 stub 的包边字符：Unicode 非字符 U+FDD0（永不分配，程序内部哨兵；控制字符会撞 eslint `no-control-regex`）。 */
const STUB_MARK = "\uFDD0";

export function preprocessMath(md: string): string {
  // 前置闸。见 `needsMathPreprocess` 头注。
  if (!needsMathPreprocess(md)) return md;
  return preprocessMathUnguarded(md);
}

/**
 * 六遍全文正则的前置闸：多数消息不含 LaTeX，没有这几个记号就跳过 `preprocessMathUnguarded`（最多五次 `indexOf`）。
 *
 * 闸认的记号就是那六遍里唯一可能改到东西的入口：`\r`（CRLF 归一）· `\[` / `\(`（翻译）· `$`（块级规整）·
 * `\uFDD0`（还原正则认的包边字符：正文里本来就带着它时慢路会错换，进闸是为了「走快路 ⇒ 与慢路逐字相同」无条件成立）。
 * 代码记号不必进闸：打 stub 与还原是一对，中间没东西可改时净效果恒等。
 * 导出给判据独立数「语料里几条走了快路」。
 */
export function needsMathPreprocess(md: string): boolean {
  return (
    md.indexOf("$") >= 0 ||
    md.indexOf("\\[") >= 0 ||
    md.indexOf("\\(") >= 0 ||
    md.indexOf("\r") >= 0 ||
    md.indexOf(STUB_MARK) >= 0
  );
}

/**
 * 闸之前的**原样实现**（只加闸、一个字节都没动这里面）。
 * 导出仅为单测：判据拿它当「慢路」的对照组，逐条对拍快路的返回值。
 */
export function preprocessMathUnguarded(md: string): string {
  // #42:先把 CRLF/CR 归一成 LF——preprocessMath 跑在 marked 内部换行归一**之前**,下面所有基于 `\n`
  // 的块规则/代码保护才对 Windows(`\r\n`)行尾可靠(否则 `$$\r\n…` 的块识别不出、露字面 `$$`)。
  md = md.replace(/\r\n?/g, "\n");
  const stash: string[] = [];
  const stub = (m: string): string => {
    stash.push(m);
    return `${STUB_MARK}M${stash.length - 1}${STUB_MARK}`;
  };
  // 1) 保护代码：围栏（``` / ~~~）+ 行内 `code`——避免动到代码里的 $$ / \[。
  md = md.replace(/(^|\n)(```|~~~)[\s\S]*?\n\2[^\n]*(?=\n|$)/g, (m) => stub(m));
  md = md.replace(/`[^`\n]*`/g, (m) => stub(m));
  // 2) \[ ... \] → 块级 $$；\( ... \) → 行内 $。
  md = md.replace(/\\\[([\s\S]*?)\\\]/g, (_m, x: string) => `\n\n$$\n${x.trim()}\n$$\n\n`);
  md = md.replace(/\\\(([\s\S]*?)\\\)/g, (_m, x: string) => `$${x.trim()}$`);
  // 3) 块级 $$ ... $$ 规整成独占行 ＋ 空行包裹。`$$` 贴着行边界才算块定界符：开 = 行首或其后紧跟换行，闭 = 行尾或其前紧接换行；
  //    散文里游离 / 未配对的 `$$`（前后都是正文）不算 —— 按个数配对会在奇数个时错位、吞掉后一个真公式。
  //    行中 `$$x$$` 不匹配，留给 marked-katex 的行内规则。lookbehind 要 V8 / Chromium。
  md = md.replace(
    /(?:(?<=^|\n)[ \t]*\$\$|\$\$(?=[ \t]*\n))([\s\S]*?)(?:(?<=\n[ \t]{0,32})\$\$|\$\$(?=[ \t]*(?:\n|$)))/g,
    (_m, x: string) => `\n\n$$\n${x.trim()}\n$$\n\n`,
  );
  // 4) 还原代码。
  return md.replace(/\uFDD0M(\d+)\uFDD0/g, (_m, i: string) => stash[Number(i)]);
}

export function renderMarkdown(md: string, opts: RenderMarkdownOptions = {}): string {
  // lazy 由调用方显式传（不传 = 急）；只选实例，不改任何共享状态。
  const m = opts.lazy ? LAZY : EAGER;
  // 数学预处理（多行块公式规整 ＋ \[..\] / \(..\) 翻译）后再交给 marked。
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
 * 补卡片里惰路留下的占位：代码块（`.code-block.code-pending`）跑高亮，数学（`[data-math-pending]`）算 KaTeX、过同一道清洗、原位替换。
 * 幂等：标 `data-enhanced`；没有占位时一次 querySelector 就返回。
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

/** 一个数学占位 → KaTeX（与急路同一组选项、同一道清洗），原位替换。 */
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
 * 每个滚动容器一个 IntersectionObserver（root = 那个容器）：卡片进可视区（± 300px，快滚到时就补、不让高亮「弹」出来）调 enhanceCard，然后 unobserve。
 * root 必填：主窗里各 tab 的 `.stream` 叠在同一块区域（后台 tab 是 `visibility:hidden`，仍有几何），拿视口当 root 分不清卡属于哪一条流。
 *
 * - 同一个 root 复用一个 IO（`WeakMap`）；容器销毁时调 {@link releaseEnhanceRoot}（`TabStreamView.disposeTab` · `SessionViewer`）。
 * - 没有 IO 的环境（jsdom）⇒ 立即 enhance。
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
