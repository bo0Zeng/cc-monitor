/**
 * `/compact` 之后 Claude Code 写回的那段续接摘要（一条 user 记录，后端判为压缩摘要）。
 * 直接按普通用户消息渲染会出现一张超长卡片污染视图，所以折成一行，点开才看全文。
 */

import { copyText } from "../copy-table";

export function buildCompactSummaryCard(
  text: string,
  timestamp: string,
  formatTime: (iso: string) => string,
): HTMLElement {
  const d = document.createElement("details");
  d.className = "card card-compact";

  const s = document.createElement("summary");
  s.className = "card-compact-summary";
  s.textContent = copyText("compact.summary.title", { n: text.length.toLocaleString(), time: formatTime(timestamp) });
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
