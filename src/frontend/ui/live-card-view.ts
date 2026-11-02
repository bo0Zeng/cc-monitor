/**
 * 活卡的**画法**（`live-card.ts` 的 `LivePainter`）：主 tab 流尾巴那一块只画主运行此刻的活卡；
 * 子运行的活卡画在它自己时间线的尾巴上（agent 面板里点开的那一条）。同一张卡（`liveKey`）逐帧复用节点、只改变了的那几处：
 * 正文照后端解析好的树排（`md-view` 的 `buildBlocks`），一块一个节点；这一帧的前几块与上一帧是同一批对象 ⇒ 那几块的节点不动，
 * 从第一处不同起摘掉、只重建尾巴。活卡不拉高亮（代码照纯字，定稿那一张才上色）。
 *
 * 为什么单独一个文件：`live-card.ts` 被主窗口与独立查看器两个入口共用（进共享块），而 `.module.css` 进共享块会让它的规则
 * 排在主窗口全局样式之前（`tests/frontend/ui/entry-graphs.vitest.ts` 的次序判据）。只有主窗口订 `session-live-card`、要画活卡 ⇒ 画法与样式
 * 住只有主窗口入口够得着的这一块，由 `main.ts` 装进 `TabManager`（`setLivePainter`）。
 */
import type { MdBlock } from "./generated/MdBlock";
import type { LiveCardState, LivePainter } from "./live-card";
import { buildBlocks } from "./md-view";
import s from "./live-card.module.css";

/** 一张卡的节点：卡 · 头一行 · 正文那一格 ＋ 正文每一块的节点与它照的那一块。 */
interface CardEls {
  key: string;
  el: HTMLDivElement;
  head: HTMLDivElement;
  body: HTMLDivElement;
  blocks: readonly MdBlock[];
  nodes: Node[];
}

/** 画出来的卡 → 它的节点（卡被摘掉、没人引用就跟着回收）。 */
const drawn = new WeakMap<Element, CardEls>();

function build(key: string): CardEls {
  const el = document.createElement("div");
  el.dataset.liveKey = key;
  const head = document.createElement("div");
  head.className = s.head;
  const body = document.createElement("div");
  body.className = s.body;
  el.append(head, body);
  const els: CardEls = { key, el, head, body, blocks: [], nodes: [] };
  drawn.set(el, els);
  return els;
}

/** 正文：前面与上一帧同一批对象的那几块不动，从第一处不同起摘掉、照这一帧重建。 */
function setBody(els: CardEls, blocks: readonly MdBlock[]): void {
  const old = els.blocks;
  let keep = 0;
  while (keep < old.length && keep < blocks.length && old[keep] === blocks[keep]) keep++;
  for (const n of els.nodes.splice(keep)) n.parentNode?.removeChild(n);
  if (keep < blocks.length) {
    const { frag } = buildBlocks(blocks.slice(keep));
    els.nodes.push(...frag.childNodes);
    els.body.append(frag);
  }
  els.blocks = blocks;
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
    els.el.className = c.phase === "streaming" ? s.card : `${s.card} ${s.awaiting}`;
    if (els.head.textContent !== c.head) els.head.textContent = c.head;
    setBody(els, c.body);
    return els.el;
  });
  const same = host.childNodes.length === order.length && order.every((el, i) => host.childNodes[i] === el);
  if (!same) host.replaceChildren(...order);
}

/** 活卡的画法。 */
export const livePainter: LivePainter = { cards: paintLiveCards };
