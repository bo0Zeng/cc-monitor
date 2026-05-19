/**
 * 识别 Claude Code `/compact` 留下的"续接消息"。
 *
 * 执行 /compact 后，Claude Code 会把整段 summary 当作一条 role: user
 * 写回 JSONL，下一轮启动靠它恢复上下文。正文固定以
 *   "This session is being continued from a previous conversation"
 * 开头。
 *
 * 直接按普通用户消息渲染会出现一张超长卡片污染视图，所以折成一行，
 * 点开才看全文。
 */

const PREFIX = "This session is being continued from a previous conversation";

export function isCompactSummary(text: string): boolean {
  return text.trimStart().startsWith(PREFIX);
}

export function buildCompactSummaryCard(
  text: string,
  timestamp: string,
  formatTime: (iso: string) => string,
): HTMLElement {
  const d = document.createElement("details");
  d.className = "card card-compact";

  const s = document.createElement("summary");
  s.className = "card-compact-summary";
  s.textContent = `📋 上下文摘要 · ${text.length.toLocaleString()} 字 · ${formatTime(timestamp)}`;
  d.appendChild(s);

  let rendered = false;
  d.addEventListener("toggle", () => {
    if (d.open && !rendered) {
      const body = document.createElement("pre");
      body.className = "card-compact-body";
      body.textContent = text;
      d.appendChild(body);
      rendered = true;
    }
  });
  return d;
}
