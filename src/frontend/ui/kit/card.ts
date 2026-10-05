/**
 * 卡片（C7）：`--bg` 上的块，标题行 ＋ 右侧幽灵动作。「需要你」时左侧琥珀竖条 ＋ 标题前状态点，整条可点（不整卡涂色、不用强调色）。
 */
import s from "./card.module.css";

export interface CardSpec {
  title: string;
  actions?: HTMLElement[];
  body?: HTMLElement | string;
  /** 「需要你」：左条 ＋ 标题前那颗点由调用方放进 `lead`。 */
  needsYou?: boolean;
  lead?: HTMLElement;
}

export function card(spec: CardSpec): HTMLElement {
  const c = document.createElement("section");
  c.className = s.card;
  if (spec.needsYou) c.dataset.needsYou = "true";
  const head = document.createElement("div");
  head.className = s.cardHead;
  if (spec.lead) head.appendChild(spec.lead);
  const t = document.createElement("span");
  t.className = s.cardTitle;
  t.textContent = spec.title;
  head.appendChild(t);
  if (spec.actions?.length) {
    const a = document.createElement("span");
    a.className = s.cardActions;
    a.append(...spec.actions);
    head.appendChild(a);
  }
  c.appendChild(head);
  if (spec.body !== undefined) {
    const b = document.createElement("div");
    b.className = s.cardBody;
    if (typeof spec.body === "string") b.textContent = spec.body;
    else b.appendChild(spec.body);
    c.appendChild(b);
  }
  return c;
}
