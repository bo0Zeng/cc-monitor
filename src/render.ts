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

/**
 * 基础 Markdown 渲染：GFM + KaTeX + 代码高亮 + sanitize。
 */
export function renderMarkdown(md: string): string {
  const raw = marked.parse(md, { async: false }) as string;
  // 显式 profile：html + svg + mathMl 都允许，避免 KaTeX 的 MathML
  // 兜底输出在未来 DOMPurify 默认值变化时被静默丢掉
  return DOMPurify.sanitize(raw, {
    USE_PROFILES: { html: true, svg: true, mathMl: true },
    ADD_ATTR: ["target", "rel", "data-copy"],
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
