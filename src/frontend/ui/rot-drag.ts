/**
 * 会话顺序改先后的状态机：静止 → 按着 →（挪过 4px 且这一行可挪）拿起 → 跟手 ⇄ 悬停第一格 → 等回包 → 静止。
 * 鼠标（按下 · 挪 · 松开）与键盘（拿起 · 一格一格挪 · 放下）走同一张表；Esc · 失焦 · 回到原位松开 ⇒ 退回、不交。
 *
 * - 只管「在看什么 · 手上正在改的」：拖的是哪一行、空槽在第几格、每一行此刻该让开多少（px）。不判顺序：
 *   哪几行可挪是回包那一格（`movable`），松在第几格交给后端认（`rotation-temp {move:{account, to}}`），回包之后整张照后端画。
 * - 落点只在可挪那一组（第 0 格在用的号也是落点：松在那里 ＝ 现在换过去）；用不了 · 先不用那两组不当落点。
 * - 行的几何由调用方量好交进来（`top` · `height`，同一个坐标系）；这里只做加减，不碰 DOM。
 * 判据：`tests/frontend/ui/rot-drag.vitest.ts`。
 */

/** 一行：号 · 能不能拖（回包给的）· 量好的上沿与高。 */
export interface DragRow {
  account: string;
  movable: boolean;
  top: number;
  height: number;
}

/** 松手的结局：点（没拖开）· 交（拖的号 · 松在第几格）· 退回（没挪 · 取消）· 什么都不做（不可挪 · 离线）。 */
export type DragEnd =
  | { kind: "click"; account: string }
  | { kind: "drop"; account: string; to: number }
  | { kind: "back" }
  | { kind: "none" };

export type DragPhase = "idle" | "pressed" | "lifted" | "waiting";

/** 挪过这么多才算拖开（px）。 */
const LIFT_PX = 4;

export class OrderDrag {
  phase: DragPhase = "idle";
  /** 拖的那一行原来在第几格 · 空槽此刻在第几格。 */
  from = -1;
  slot = -1;
  private rows: DragRow[] = [];
  private y0 = 0;
  private dy = 0;
  private moved = false;

  /** `lifts` ＝ 此刻能不能拿起（那台离线 ⇒ 不能）。 */
  constructor(private readonly lifts: () => boolean) {}

  /** 拖的那一行的号（没在拖 ⇒ `null`）。 */
  get account(): string | null {
    return this.from >= 0 ? (this.rows[this.from]?.account ?? null) : null;
  }

  /** 空槽在第一格（松手 ＝ 现在换过去）。 */
  get first(): boolean {
    return this.phase === "lifted" && this.slot === 0;
  }

  /** 按下一行；等回包时不收（`false`）。 */
  press(rows: DragRow[], i: number, y: number): boolean {
    if (this.phase === "waiting") return false;
    this.reset();
    this.rows = rows;
    this.from = this.slot = i;
    this.y0 = y;
    this.phase = "pressed";
    return true;
  }

  /** 指针挪到 `y`：挪过 4px 且可挪 ⇒ 拿起；拿着 ⇒ 跟手（夹在可挪那一组里）、空槽照越过的中线换格。 */
  move(y: number): void {
    if (this.phase === "pressed") {
      if (Math.abs(y - this.y0) <= LIFT_PX) return;
      this.moved = true;
      if (!this.rows[this.from]?.movable || !this.lifts()) return;
      this.phase = "lifted";
    }
    if (this.phase !== "lifted") return;
    const me = this.rows[this.from];
    const last = this.rows[this.last()];
    const lo = this.rows[0].top - me.top;
    const hi = last.top + last.height - me.height - me.top;
    this.dy = Math.max(lo, Math.min(hi, y - this.y0));
    this.slot = this.slotAt(me.top + this.dy, me.top + me.height + this.dy);
  }

  /** 键盘拿起第 `i` 行（不可挪 · 离线 · 等回包 ⇒ `false`）。 */
  pick(rows: DragRow[], i: number): boolean {
    if (this.phase === "waiting" || !rows[i]?.movable || !this.lifts()) return false;
    this.reset();
    this.rows = rows;
    this.from = this.slot = i;
    this.phase = "lifted";
    return true;
  }

  /** 键盘挪一格（夹在可挪那一组里）。 */
  step(d: -1 | 1): void {
    if (this.phase !== "lifted") return;
    this.slot = Math.max(0, Math.min(this.last(), this.slot + d));
    const me = this.rows[this.from];
    const at = this.rows[this.slot];
    this.dy = this.slot <= this.from ? at.top - me.top : at.top + at.height - me.top - me.height;
  }

  /** 第 `j` 行此刻该挪多少（px）：拖的那一行跟手；空槽经过的那几行让一格。 */
  offset(j: number): number {
    if (this.phase !== "lifted" && this.phase !== "waiting") return 0;
    if (j === this.from) return this.dy;
    const me = this.rows[this.from];
    const next = this.rows[this.from + 1] ?? this.rows[this.from - 1];
    const gap = next ? Math.abs(next.top - me.top) - (next.top > me.top ? me.height : next.height) : 0;
    const step = me.height + gap;
    if (this.slot > this.from && j > this.from && j <= this.slot) return -step;
    if (this.slot < this.from && j >= this.slot && j < this.from) return step;
    return 0;
  }

  /** 松手（鼠标）· 放下（键盘）。交了 ⇒ 等回包（行留在放下的位置）。 */
  release(): DragEnd {
    if (this.phase === "pressed") {
      const end: DragEnd = this.moved ? { kind: "none" } : { kind: "click", account: this.account ?? "" };
      this.reset();
      return end;
    }
    if (this.phase !== "lifted") return { kind: "none" };
    if (this.slot === this.from) {
      this.reset();
      return { kind: "back" };
    }
    this.phase = "waiting";
    return { kind: "drop", account: this.account ?? "", to: this.slot };
  }

  /** Esc · 失焦 · 指针出窗：退回，不交。 */
  cancel(): void {
    if (this.phase !== "waiting") this.reset();
  }

  /** 回包到了（成 · 拒 · 说不清都一样）：回静止，之后照后端重画。 */
  settle(): void {
    this.reset();
  }

  /** 可挪那一组的末格（第 0 格在用的号也算落点）。 */
  private last(): number {
    let k = 0;
    this.rows.forEach((r, j) => {
      if (r.movable) k = j;
    });
    return k;
  }

  /** 浮起的行上沿 `top` · 下沿 `bottom` 越过了哪几行的中线 ⇒ 空槽在第几格。 */
  private slotAt(top: number, bottom: number): number {
    const mid = (r: DragRow): number => r.top + r.height / 2;
    let slot = this.from;
    for (let j = this.from - 1; j >= 0; j--) if (top < mid(this.rows[j])) slot = j;
    for (let j = this.from + 1; j <= this.last(); j++) if (bottom > mid(this.rows[j])) slot = j;
    return slot;
  }

  private reset(): void {
    this.phase = "idle";
    this.from = this.slot = -1;
    this.dy = 0;
    this.moved = false;
  }
}
