import { marked } from "marked";
import DOMPurify from "dompurify";

marked.setOptions({
  gfm: true,
  breaks: false,
});

/**
 * 基础 Markdown 渲染：GFM + sanitize。
 * KaTeX / 代码语法高亮在 M3 接入。
 */
export function renderMarkdown(md: string): string {
  const raw = marked.parse(md, { async: false }) as string;
  return DOMPurify.sanitize(raw, { ADD_ATTR: ["target", "rel"] });
}

/** 纯文本（用户消息保守模式）：转义 + 保留换行 */
export function renderPlainText(text: string): string {
  const div = document.createElement("div");
  div.textContent = text;
  return div.innerHTML.replace(/\n/g, "<br>");
}
