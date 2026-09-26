/**
 * 〔TAP · V124〕活卡的**画法**（`live-card.ts` 的 `LivePainter`）：把一个 tab 此刻的活卡整块画进它流尾巴上那一块。
 *
 * 为什么单独一个文件：`live-card.ts` 被主窗口与独立查看器两个入口共用（进共享块），而 `.module.css` 进共享块会让它的规则
 * 排在主窗口全局样式之前（`tests/entry-graphs.vitest.ts` 的次序判据）。只有主窗口订 `session-tap`、要画活卡 ⇒ 画法与样式
 * 住只有主窗口入口够得着的这一块，由 `main.ts` 装进 `TabManager`（`setLivePainter`）。
 * 要求出处：`设计/10 §2.3`「活卡 → 定稿」· 设计住仓外 `调研/第四波记录/TAP.md §5`。
 */
import { renderCardText, type LiveCardState } from "./live-card";
import s from "./live-card.module.css";

export function paintLiveCards(host: HTMLElement, cards: LiveCardState[]): void {
  host.replaceChildren(
    ...cards.map((c) => {
      const { head, body } = renderCardText(c);
      const el = document.createElement("div");
      el.className = c.phase === "streaming" ? s.card : `${s.card} ${s.awaiting}`;
      el.dataset.liveKey = c.key;
      const h = document.createElement("div");
      h.className = s.head;
      h.textContent = head;
      const b = document.createElement("div");
      b.className = s.body;
      b.textContent = body;
      el.append(h, b);
      return el;
    }),
  );
}
