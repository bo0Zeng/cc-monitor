import { marked } from "marked";
import DOMPurify from "dompurify";
import hljs from "highlight.js/lib/common";
import markedKatex from "marked-katex-extension";
import "highlight.js/styles/atom-one-dark.css";
import "katex/dist/katex.min.css";

marked.setOptions({
  gfm: true,
  breaks: false,
});

/**
 * 用 marked v9+ 的对象式 renderer 覆盖代码块输出，接 highlight.js。
 *
 * - 明确指定语言（` ```ts `）→ `hljs.highlight(code, { language })`
 * - 未指定语言 → `hljs.highlightAuto`（在 common 子集里猜）
 * - 任何异常 → 退回转义后的裸文本，永远不让 markdown 渲染崩
 *
 * 仅引 `highlight.js/lib/common`，约 30 种主流语言，bundle 体积约 100KB；
 * 比 full 包小一个数量级。
 */
/**
 * KaTeX 扩展：
 * - `$...$` 行内、`$$...$$` 块级
 * - throwOnError: false → 错误 LaTeX 渲染成红色源码而不是抛异常
 * - 字体由 npm 包内的 katex.min.css 引用，vite 自动打包，绝不走 CDN
 */
marked.use(
  markedKatex({
    throwOnError: false,
  }),
);

marked.use({
  renderer: {
    code(token) {
      const lang = (token.lang ?? "").trim().split(/\s+/)[0];
      const code = token.text ?? "";
      let highlighted: string;
      try {
        if (lang && hljs.getLanguage(lang)) {
          highlighted = hljs.highlight(code, {
            language: lang,
            ignoreIllegals: true,
          }).value;
        } else {
          highlighted = hljs.highlightAuto(code).value;
        }
      } catch {
        highlighted = escapeHtml(code);
      }
      const cls = lang ? `language-${lang} hljs` : "hljs";
      return `<pre><code class="${cls}">${highlighted}</code></pre>`;
    },
  },
});

/**
 * 基础 Markdown 渲染：GFM + sanitize + 代码高亮。
 * KaTeX 在 Stage 3 接入。
 */
export function renderMarkdown(md: string): string {
  const raw = marked.parse(md, { async: false }) as string;
  // 显式 profile：html + svg + mathMl 都允许，避免 KaTeX 的 MathML
  // 兜底输出在未来 DOMPurify 默认值变化时被静默丢掉
  return DOMPurify.sanitize(raw, {
    USE_PROFILES: { html: true, svg: true, mathMl: true },
    ADD_ATTR: ["target", "rel"],
  });
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
