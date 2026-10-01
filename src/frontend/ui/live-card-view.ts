/**
 * 流尾巴那一块的**画法**（`live-card.ts` 的 `LivePainter`）：上面是每个在跑的子运行一行（点开是它的实时时间线），
 * 下面是主运行此刻的活卡。整块按此刻的状态重画；点开着的时间线由宿主留着（重画只是把它挂回原处，不重读）。
 *
 * 为什么单独一个文件：`live-card.ts` 被主窗口与独立查看器两个入口共用（进共享块），而 `.module.css` 进共享块会让它的规则
 * 排在主窗口全局样式之前（`tests/frontend/ui/entry-graphs.vitest.ts` 的次序判据）。只有主窗口订 `session-tap`、要画活卡 ⇒ 画法与样式
 * 住只有主窗口入口够得着的这一块，由 `main.ts` 装进 `TabManager`（`setLivePainter`）。
 */
import { renderCardText, type LiveCardState, type LivePainter } from "./live-card";
import type { RunRow } from "./runs";
import s from "./live-card.module.css";

/** 宿主给画法的口：某个子运行的时间线（点开时要；宿主留着、续读）· 关掉了。 */
export interface RunRowsHost {
  timeline(sid: string, run: string): HTMLElement;
  closed(sid: string, run: string): void;
}

/** 活卡那一截（主运行的；子运行时间线尾巴上那一截也用它）。 */
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

/** 整块的画法：子运行的行 ＋ 主运行的活卡。点开 / 收起一行只重画这一块。 */
export function livePainter(runsHost: RunRowsHost): LivePainter {
  const open = new Set<string>();
  const keyOf = (sid: string, run: string): string => `${sid}\u0000${run}`;
  const paint = (host: HTMLElement, sid: string, cards: LiveCardState[], rows: RunRow[]): void => {
    // 关着的行不再在跑 ⇒ 它的时间线交还宿主。
    for (const k of [...open]) {
      const [ksid, krun] = k.split("\u0000");
      if (ksid === sid && krun !== undefined && !rows.some((r) => r.run === krun)) {
        open.delete(k);
        runsHost.closed(sid, krun);
      }
    }
    const rowEls = rows.flatMap((r: RunRow) => {
      const row = document.createElement("div");
      row.className = s.runRow;
      row.dataset.run = r.run;
      row.setAttribute("role", "button");
      row.tabIndex = 0;
      row.textContent = r.text;
      const toggle = (): void => {
        const k = keyOf(sid, r.run);
        if (open.has(k)) {
          open.delete(k);
          runsHost.closed(sid, r.run);
        } else {
          open.add(k);
        }
        paint(host, sid, cards, rows);
      };
      row.addEventListener("click", toggle);
      row.addEventListener("keydown", (e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          toggle();
        }
      });
      if (!open.has(keyOf(sid, r.run))) return [row];
      row.setAttribute("aria-expanded", "true");
      return [row, runsHost.timeline(sid, r.run)];
    });
    const cardsBox = document.createElement("div");
    paintLiveCards(cardsBox, cards);
    host.replaceChildren(...rowEls, ...cardsBox.childNodes);
  };
  return { trailer: paint, cards: paintLiveCards };
}
