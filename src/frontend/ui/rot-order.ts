/**
 * 会话面板的顺序：照 `rotation-view` 会话形的 `lineup` 一号一行（第一格在用；「可挪」· 此刻用不了 · 先不用三组，后两组没有把手）。
 * 行：把手 · 序号 · 头像 · 号名＋记号（名下策略写的那一句）· 5h/7d 两条量 · 右格（后端写好的此刻怎样）· 行尾的勾。
 * 会话不轮换（`strategy` 空）⇒ 同一张表，没有拖动那句 · 勾 ·［编辑策略］；点一行照样［立即用］。
 *
 * - 字全是回包里的（右格 `state` · 名下 `why` · 不可挪的 `stay`）；可不可挪照 `movable`。界面不挑字、不判顺序。
 * - 拖一行（`rot-drag.ts` 那张状态表）：挪过 4px 拿起，只沿纵向跟手、夹在可挪那一组里；别的行跨中线 120ms 让位；贴边 40px 列表自己滚；
 *   悬停第一格写「松手换到 X」。松手交一次 `move {account, to}`，回包才定：等回包时推送记下不重画、不收新的拖动；
 *   成 ⇒ 回静止（调用方重读、照后端画，行 FLIP 滑到新位置）；拒 ⇒ 滑回；没回包 ⇒ 留在放下处标「没确认」。
 * - 键盘同一张表（`rot-keys.ts`）。点一行（没拖开）出小窗：只看 ＋［立即用］·［编辑策略］。
 * 判据：`tests/frontend/ui/rot-order.vitest.ts` · `rot-keys.vitest.ts`。
 */
import { copyText } from "./copy-table";
import type { Machine } from "./ipc/origin";
import { button } from "./kit/button";
import { el } from "./kit/el";
import { icon } from "./kit/icon";
import { closePopover, openPopover } from "./kit/popover";
import { attachTooltip } from "./kit/tooltip";
import { OrderDrag, type DragEnd, type DragRow } from "./rot-drag";
import { avatar, markEls, meters, nameIn, rowOf } from "./rot-board";
import { orderKeys } from "./rot-keys";
import type { RotLine } from "./generated/RotLine";
import type { RotationViewReply } from "./generated/RotationViewReply";
import s from "./rot-order.module.css";

/** 放下之后那一次写的结局：成 · 拒（滑回）· 说不清（没回包 · 断线：留在放下处标「没确认」）。 */
export type MoveEnd = "done" | "back" | "unsure";

/** 手势 ⇒ 命令（调用方发；`move` 等到回包才回）。 */
export interface OrderActs {
  move(account: string, to: number): Promise<MoveEnd>;
  off(account: string, off: boolean): void;
  /** 照策略排（清掉拖出来的顺序）。 */
  clear(): void;
  useNow(account: string): void;
  /** 开设置窗那台「轮换」栏、这个号的编辑小窗。 */
  editStrategy?: (account: string) => void;
}

/** 画一次要的：回包 · 被拦下的那一句（顺序上方）· 那台离线（不拿起）。 */
export interface OrderInput {
  view: RotationViewReply;
  refused: { account: string; text: string } | null;
  offline: boolean;
}

export interface Order {
  /** 一直是同一个节点（调用方重画时把它原样放回去）。 */
  el: HTMLElement;
  draw(input: OrderInput): void;
  /** 手上拿着一行 · 等回包：调用方的重读先记下，静止了再读。 */
  busy(): boolean;
}

/** 贴着滚动区上下边这么近就自己滚（px）· 每帧滚多少。 */
const EDGE_PX = 40;
const SCROLL_STEP = 8;

/** 往上找滚得动的那一层（抽屉的身子）。 */
function scrollerOf(e: HTMLElement): HTMLElement | null {
  for (let p = e.parentElement; p; p = p.parentElement) {
    const o = getComputedStyle(p).overflowY;
    if (o === "auto" || o === "scroll") return p;
  }
  return null;
}

/** 骨架字（不随数据变；右格 · 为什么那几句都是回包里的）。 */
const W = {
  order: copyText("rot.h.order"),
  hint: copyText("rot.h.orderHint"),
  manual: copyText("rot.order.manual"),
  reset: copyText("rot.order.reset"),
  groupOut: copyText("rot.order.groupOut"),
  groupOff: copyText("rot.order.groupOff"),
  dropFirst: copyText("rot.order.dropFirst"),
  unsure: copyText("rot.order.unconfirmed"),
  edit: copyText("rot.top.edit"),
  useNow: copyText("rot.order.useNow"),
};

/** 那一组的小标题（可挪那一组不写，顺序的标题就是它）。 */
function groupHead(g: RotLine["group"], text: string): HTMLElement {
  const h = el("div", s.rotOrderGroup, text);
  h.dataset.rotGroup = g;
  return h;
}

/** 勾的读屏名接到号名那一格上（一行一个，号名里可能有空格 · 引号，id 不拿它拼）。 */
let nameSeq = 0;

export function createOrder(origin: Machine, acts: OrderActs, onIdle: () => void): Order {
  const root = el("div", s.rotOrder);
  root.dataset.rotOrder = "";
  const live = el("div", s.rotOrderLive);
  live.setAttribute("aria-live", "polite");
  let input: OrderInput | null = null;
  let held: OrderInput | null = null;
  let lines: HTMLElement[] = [];
  const drag = new OrderDrag(() => !(input?.offline ?? true));
  let firstSlot: HTMLElement | null = null;
  /** 「没确认」那一次放下时手上的那一份回包：同一份再来 ⇒ 不重画（放下的位置留着）。 */
  let pinned: RotationViewReply | null = null;
  let scroll: { box: HTMLElement; top0: number; y: number; raf: number } | null = null;

  const view = (): RotationViewReply | null => input?.view ?? null;
  const geometry = (): DragRow[] =>
    lines.map((r, k) => {
      const b = r.getBoundingClientRect();
      return { account: r.dataset.account ?? "", movable: input?.view.lineup[k]?.movable ?? false, top: b.top, height: b.height };
    });

  /** 照状态机摆每一行（跟手的那一行不带过渡）；悬停第一格那一格的字。 */
  const apply = (): void => {
    lines.forEach((r, k) => {
      const dy = drag.offset(k);
      r.style.transform = dy === 0 ? "" : `translateY(${dy}px)`;
      if (k === drag.from && drag.phase === "lifted") r.dataset.lifted = "true";
      else delete r.dataset.lifted;
    });
    const account = drag.account;
    if (drag.first && account !== null) {
      firstSlot ??= el("div", s.rotOrderFirst);
      firstSlot.dataset.rotFirst = "";
      firstSlot.replaceChildren(el("span", "", W.dropFirst), el("b", s.rotOrderAcct, nameIn(view(), account)));
      if (!firstSlot.isConnected) lines[0]?.before(firstSlot);
    } else firstSlot?.remove();
  };

  /** 松手 · 放下：交 ⇒ 等回包，照结局回静止 / 滑回 / 标「没确认」；点 ⇒ 小窗。 */
  const finish = (end: DragEnd): void => {
    stopScroll();
    const clicked = end.kind === "click" ? lines.find((x) => x.dataset.account === end.account) : undefined;
    if (clicked) pop(clicked.dataset.account ?? "", clicked);
    apply();
    if (end.kind !== "drop") return idle();
    const row = lines[drag.from];
    row.dataset.waiting = "true";
    void acts.move(end.account, end.to).then((how) => {
      delete row.dataset.waiting;
      if (how === "unsure") {
        // 留在放下处、标「没确认」，直到下一份回包（连上之后重读）来了才照后端画。
        row.dataset.unconfirmed = "true";
        row.querySelector("[data-rot-state]")?.replaceChildren(W.unsure);
        pinned = input?.view ?? null;
        drag.phase = "idle";
        return idle(false);
      }
      drag.settle();
      apply();
      idle();
    });
  };

  /** 回静止：等着的那一次推送这会儿补画（`redraw` ＝ 先照手上最新的那一份重画）。 */
  const idle = (redraw = true): void => {
    if (redraw && held) {
      const h = held;
      held = null;
      paint(h);
    }
    onIdle();
  };

  const stopScroll = (): void => {
    if (scroll) cancelAnimationFrame(scroll.raf);
    scroll = null;
  };
  /** 贴边自己滚：滚过的距离折进指针位置（行的几何是按下那一刻量的）。 */
  const edgeScroll = (y: number): void => {
    const box = scroll?.box ?? scrollerOf(root);
    if (!box) return;
    if (!scroll) scroll = { box, top0: box.scrollTop, y, raf: 0 };
    scroll.y = y;
    const tick = (): void => {
      if (!scroll || drag.phase !== "lifted") return stopScroll();
      const b = scroll.box.getBoundingClientRect();
      const d = scroll.y < b.top + EDGE_PX ? -SCROLL_STEP : scroll.y > b.bottom - EDGE_PX ? SCROLL_STEP : 0;
      if (d === 0) return;
      scroll.box.scrollTop += d;
      drag.move(scroll.y + scroll.box.scrollTop - scroll.top0);
      apply();
      // 调度：自链 —— 贴边自动滚，每帧一步；离开边沿 · 松手 · 取消（stopScroll）即停
      scroll.raf = requestAnimationFrame(tick);
    };
    cancelAnimationFrame(scroll.raf);
    // 调度：自链 —— 同上，链的起点
    scroll.raf = requestAnimationFrame(tick);
  };

  const onPointerDown = (e: PointerEvent, k: number): void => {
    if (e.button !== 0) return;
    if (!drag.press(geometry(), k, e.clientY)) return;
    const move = (m: PointerEvent): void => {
      const y = m.clientY + (scroll ? scroll.box.scrollTop - scroll.top0 : 0);
      drag.move(y);
      if (drag.phase === "lifted") edgeScroll(m.clientY);
      apply();
    };
    const end = (): void => {
      document.removeEventListener("pointermove", move);
      document.removeEventListener("pointerup", end);
      document.removeEventListener("keydown", esc, true);
      window.removeEventListener("blur", cancel);
    };
    const up = (u: PointerEvent): void => {
      end();
      // 指针出了窗口才松开 ⇒ 退回。
      const out = u.clientX < 0 || u.clientY < 0 || u.clientX > window.innerWidth || u.clientY > window.innerHeight;
      if (out && drag.phase === "lifted") return cancel();
      finish(drag.release());
    };
    const cancel = (): void => {
      end();
      drag.cancel();
      stopScroll();
      apply();
      idle();
    };
    const esc = (k2: KeyboardEvent): void => {
      if (k2.key !== "Escape") return;
      k2.stopPropagation();
      cancel();
    };
    document.addEventListener("pointermove", move);
    document.addEventListener("pointerup", up);
    document.addEventListener("keydown", esc, true);
    window.addEventListener("blur", cancel);
  };

  /** 点一行出的小窗：只看（此刻 · 用量 · 它在这份策略里的规则）＋［立即用］·［编辑策略］。 */
  const pop = (account: string, anchor: HTMLElement): void => {
    const v = view();
    if (!v) return;
    const l = v.lineup.find((x) => x.account === account);
    const box = el("div", s.rotOrderPop);
    box.dataset.rotPop = "";
    const top = el("div", s.rotOrderPopTop);
    top.append(avatar(v, account, origin), el("b", s.rotOrderPopName, nameIn(v, account)));
    box.appendChild(top);
    const why = l?.why ?? l?.stay;
    if (why !== undefined) box.appendChild(el("div", s.rotOrderPopLine, why));
    const row = rowOf(v, account);
    if (row) box.appendChild(el("div", s.rotOrderPopLine, row.line));
    for (const r of row?.rules ?? []) box.appendChild(el("div", s.rotOrderPopLine, r));
    const btns = el("div", s.rotOrderPopTop);
    if (account !== v.current) {
      const use = button({
        label: W.useNow,
        kind: "secondary",
        size: "compact",
        onClick: () => {
          closePopover();
          acts.useNow(account);
        },
      });
      use.disabled = input?.offline ?? false;
      btns.appendChild(use);
    }
    const go = acts.editStrategy;
    if (go && v.can?.edit) {
      btns.appendChild(
        button({
          label: W.edit,
          kind: "ghost",
          size: "compact",
          onClick: () => {
            closePopover();
            go(account);
          },
        }),
      );
    }
    box.appendChild(btns);
    openPopover(anchor, box, { label: nameIn(v, account) });
  };

  /** 一行。 */
  const lineEl = (v: RotationViewReply, l: RotLine, k: number): HTMLElement => {
    const r = el("div", s.rotOrderRow);
    r.dataset.account = l.account;
    r.dataset.rotRow = "";
    r.dataset.group = l.group;
    r.dataset.key = `rot-row:${l.account}`;
    r.tabIndex = k === 0 ? 0 : -1;
    if (l.group === "use") r.dataset.inUse = "true";
    if (l.stay !== undefined) attachTooltip(r, l.stay);
    const handle = el("span", s.rotOrderHandle);
    if (l.movable) {
      handle.dataset.rotHandle = "";
      handle.appendChild(icon("drag", "compact"));
    }
    const name = el("span", s.rotOrderName);
    const head = el("span", s.rotOrderNameTop);
    const acct = el("b", s.rotOrderAcct, nameIn(v, l.account));
    acct.id = `rot-order-name-${++nameSeq}`;
    head.append(acct, ...markEls(rowOf(v, l.account), r));
    name.appendChild(head);
    if (l.why !== undefined) name.appendChild(el("span", s.rotOrderWhy, l.why));
    const f = v.accounts.find((x) => x.account === l.account);
    const state = el("span", s.rotOrderState, l.state);
    state.dataset.rotState = "";
    const end = el("span", s.rotOrderEnd);
    end.append(state);
    // 「先不用」那一勾：后端说这个会话能先不用一个号才出（`can.off`）。
    if (v.can?.off) {
      const tick = document.createElement("input");
      tick.type = "checkbox";
      tick.className = s.rotOrderTick;
      tick.checked = l.group !== "off";
      tick.disabled = input?.offline ?? false;
      tick.setAttribute("aria-labelledby", acct.id);
      tick.addEventListener("pointerdown", (e) => e.stopPropagation());
      tick.addEventListener("change", () => acts.off(l.account, !tick.checked));
      end.appendChild(tick);
    }
    r.append(handle, el("span", s.rotOrderIx, String(k + 1)), avatar(v, l.account, origin), name);
    if (f) r.appendChild(meters(f, undefined));
    r.appendChild(end);
    r.addEventListener("pointerdown", (e) => onPointerDown(e, lines.indexOf(r)));
    return r;
  };

  /** 照回包重画；行照旧位置 FLIP 滑到新位置（不整块跳）。 */
  const paint = (next: OrderInput): void => {
    const before = new Map(lines.map((r) => [r.dataset.account ?? "", r.getBoundingClientRect().top]));
    input = next;
    const v = next.view;
    const head = el("div", s.rotOrderHead);
    head.append(el("span", s.rotOrderHeadText, W.order));
    // 拖动那句跟着「能不能拖」（`can.reorder`，后端判）。
    if (v.can?.reorder) head.append(el("span", s.rotOrderHeadNote, W.hint));
    if (v.manual) {
      head.append(
        el("span", s.rotOrderSpacer),
        el("span", s.rotOrderHeadNote, W.manual),
        button({ label: W.reset, kind: "ghost", size: "compact", onClick: () => acts.clear() }),
      );
    }
    const heads: Partial<Record<RotLine["group"], HTMLElement>> = { out: groupHead("out", W.groupOut), off: groupHead("off", W.groupOff) };
    const kids: HTMLElement[] = [head];
    if (next.refused) {
      const why = el("div", s.rotOrderRefused, next.refused.text);
      why.setAttribute("role", "status");
      kids.push(why);
    }
    lines = [];
    let group: RotLine["group"] | null = null;
    v.lineup.forEach((l, k) => {
      const g = l.group === "use" ? "next" : l.group;
      if (g !== group) {
        const h = heads[g];
        if (h) kids.push(h);
        group = g;
      }
      const r = lineEl(v, l, k);
      lines.push(r);
      kids.push(r);
    });
    root.dataset.noLift = String(next.offline);
    root.replaceChildren(...kids, live);
    for (const r of lines) {
      const was = before.get(r.dataset.account ?? "");
      if (was === undefined) continue;
      const d = was - r.getBoundingClientRect().top;
      if (d === 0) continue;
      r.style.transition = "none";
      r.style.transform = `translateY(${d}px)`;
      // 调度：一次性 —— FLIP：下一帧放开过渡，行从旧位置滑到新位置
      requestAnimationFrame(() => {
        r.style.transition = "";
        r.style.transform = "";
      });
    }
  };

  root.addEventListener("keydown", (e) =>
    orderKeys(e, {
      drag,
      lines: () => lines,
      geometry,
      lineup: () => input?.view.lineup ?? [],
      apply,
      finish,
      cancel: () => {
        drag.cancel();
        apply();
        idle();
      },
      click: (k) => {
        const r = lines[k];
        if (r) pop(r.dataset.account ?? "", r);
      },
      say: (t) => (live.textContent = t),
    }),
  );

  return {
    el: root,
    draw: (next) => {
      // 手上拿着一行 · 等回包 ⇒ 这一份先记下，静止了再画（手上正在改的不被推送盖掉）。
      if (drag.phase !== "idle") {
        held = next;
        return;
      }
      if (pinned !== null && next.view === pinned) {
        input = next;
        return;
      }
      pinned = null;
      paint(next);
    },
    busy: () => drag.phase !== "idle",
  };
}
