/**
 * 会话顺序的键盘改先后（同 `rot-drag.ts` 那张状态表，放下交的是同一条命令）：
 * 没拿起时 ↑↓ 在行间挪焦点、Enter ＝ 点（出小窗）、空格拿起（不可挪 ⇒ 念回包那一句为什么）；
 * 拿起之后 ↑↓ 挪空槽（同让位动画）、空格 / Enter 放下、Esc 退回。每一步读屏念一句：拿起 · 空槽在第几格（第一格念「放下即换过去」）·
 * 放下 · 取消（骨架字；格数是界面手上的那一格，照念数）。
 * 判据：`tests/frontend/ui/rot-keys.vitest.ts`。
 */
import { copyText } from "./copy-table";
import type { DragEnd, DragRow, OrderDrag } from "./rot-drag";
import type { RotLine } from "./generated/RotLine";

/** 读屏念的骨架字。 */
const W = {
  lifted: copyText("rot.order.lifted"),
  atFirst: copyText("rot.order.atFirst"),
  dropped: copyText("rot.order.dropped"),
  cancel: copyText("rot.order.cancel"),
};

/** 顺序那一块交进来的几样（行 · 状态机 · 画 · 放下 · 点 · 念）。 */
export interface KeyCtx {
  drag: OrderDrag;
  lines(): HTMLElement[];
  geometry(): DragRow[];
  lineup(): readonly RotLine[];
  apply(): void;
  finish(end: DragEnd): void;
  cancel(): void;
  click(k: number): void;
  say(text: string): void;
}

/** 顺序上的一次按键；认了 ⇒ `true`（已挡掉默认动作）。 */
export function orderKeys(e: KeyboardEvent, c: KeyCtx): boolean {
  const { drag } = c;
  const lines = c.lines();
  const row = e.target instanceof HTMLElement ? e.target.closest<HTMLElement>("[data-rot-row]") : null;
  const k = row ? lines.indexOf(row) : -1;
  const take = (): true => {
    e.preventDefault();
    e.stopPropagation();
    return true;
  };
  const up = e.key === "ArrowUp";
  const down = e.key === "ArrowDown";
  const space = e.key === " ";
  const enter = e.key === "Enter";
  const esc = e.key === "Escape";
  if (drag.phase === "lifted") {
    if (up || down) {
      drag.step(up ? -1 : 1);
      c.apply();
      c.say(drag.slot === 0 ? W.atFirst : String(drag.slot + 1));
      return take();
    }
    if (space || enter) {
      const end = drag.release();
      c.say(end.kind === "drop" ? W.dropped : W.cancel);
      c.finish(end);
      return take();
    }
    if (esc) {
      c.cancel();
      c.say(W.cancel);
      return take();
    }
    return false;
  }
  if (k < 0) return false;
  if (up || down) {
    const next = lines[Math.max(0, Math.min(lines.length - 1, k + (up ? -1 : 1)))];
    for (const r of lines) r.tabIndex = r === next ? 0 : -1;
    next.focus();
    return take();
  }
  if (enter) {
    c.click(k);
    return take();
  }
  if (space) {
    const stay = c.lineup()[k]?.stay;
    if (drag.pick(c.geometry(), k)) {
      c.apply();
      c.say(W.lifted);
    } else if (stay !== undefined) c.say(stay);
    return take();
  }
  return false;
}
