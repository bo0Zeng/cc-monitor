/**
 * 活卡的**画法**（`live-card.ts` 的 `LivePainter`）：主 tab 流尾巴那一块只画主运行此刻的活卡；
 * 子运行的活卡画在它自己时间线的尾巴上（agent 面板里点开的那一条）。同一张卡（`liveKey`）逐帧复用节点、只改变了的那几处：
 * 正文接着上一帧往下说 ⇒ 只追加新的那一截（`Text.appendData`），不重排整段长文本。
 *
 * 为什么单独一个文件：`live-card.ts` 被主窗口与独立查看器两个入口共用（进共享块），而 `.module.css` 进共享块会让它的规则
 * 排在主窗口全局样式之前（`tests/frontend/ui/entry-graphs.vitest.ts` 的次序判据）。只有主窗口订 `session-live-card`、要画活卡 ⇒ 画法与样式
 * 住只有主窗口入口够得着的这一块，由 `main.ts` 装进 `TabManager`（`setLivePainter`）。
 */
import type { LiveCardState, LivePainter } from "./live-card";
import s from "./live-card.module.css";

/** 一张卡的节点：卡 · 头一行 · 正文那一个文本节点。 */
interface CardEls {
  key: string;
  el: HTMLDivElement;
  head: HTMLDivElement;
  text: Text;
  /** 上一帧交来的正文（比对用它，不读回 `text.data`：追加过的节点值可能是拼出来的串，每读一次要拍平一遍）。 */
  body: string;
}

/** 画出来的卡 → 它的节点（卡被摘掉、没人引用就跟着回收）。 */
const drawn = new WeakMap<Element, CardEls>();

function build(key: string): CardEls {
  const el = document.createElement("div");
  el.className = s.card;
  el.dataset.liveKey = key;
  const head = document.createElement("div");
  head.className = s.head;
  const b = document.createElement("div");
  b.className = s.body;
  const text = document.createTextNode("");
  b.append(text);
  el.append(head, b);
  const els = { key, el, head, text, body: "" };
  drawn.set(el, els);
  return els;
}

/** 正文：新值以旧值开头 ⇒ 只追加差的那一截；否则（核心截了头 · 换了）整段换。 */
function setBody(els: CardEls, body: string): void {
  const old = els.body;
  if (body === old) return;
  if (old !== "" && body.startsWith(old)) els.text.appendData(body.slice(old.length));
  else els.text.data = body;
  els.body = body;
}

/** 活卡那一截：这一帧有的卡照先后摆，上一帧已画的那张复用节点，这一帧没有的摘掉。 */
export function paintLiveCards(host: HTMLElement, cards: LiveCardState[]): void {
  const had = new Map<string, CardEls>();
  for (const child of host.children) {
    const els = drawn.get(child);
    if (els) had.set(els.key, els);
  }
  const order = cards.map((c) => {
    const els = had.get(c.key) ?? build(c.key);
    els.el.classList.toggle(s.awaiting, c.phase !== "streaming");
    if (els.head.textContent !== c.head) els.head.textContent = c.head;
    setBody(els, c.body);
    return els.el;
  });
  const same = host.childNodes.length === order.length && order.every((el, i) => host.childNodes[i] === el);
  if (!same) host.replaceChildren(...order);
}

/** 活卡的画法。 */
export const livePainter: LivePainter = { cards: paintLiveCards };
