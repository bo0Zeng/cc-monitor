/**
 * 认不出的那一行（通用记录 `unread`）：照重试细条的样子画一行 —— 左 2px 琥珀、默认折起；展开是原文摘录 ＋［复制详情］。
 * 字（`text`）、语气、摘录都是核心写好的；这里不按 `type` / `why` 分支，相邻的几条后端已并好（`count`）。
 */
import { copyDetailButton } from "../kit/detail";

export function buildUnreadLine(args: { text: string; excerpt: string; time: string }): HTMLElement {
  const line = document.createElement("details");
  line.className = "card card-unread";
  const sum = document.createElement("summary");
  const what = document.createElement("span");
  what.textContent = args.text;
  sum.appendChild(what);
  if (args.time) {
    const ts = document.createElement("span");
    ts.className = "card-unread-ts";
    ts.textContent = args.time;
    sum.appendChild(ts);
  }
  line.appendChild(sum);
  const body = document.createElement("div");
  body.className = "card-unread-body";
  const pre = document.createElement("pre");
  pre.className = "card-unread-excerpt";
  pre.textContent = args.excerpt;
  body.appendChild(pre);
  const copy = copyDetailButton(args.text, args.excerpt);
  if (copy) body.appendChild(copy);
  line.appendChild(body);
  return line;
}
