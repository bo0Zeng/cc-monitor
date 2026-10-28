/**
 * 会话顺序的拖动状态机（`rot-drag.ts`）：只管手势与手上那一格（拿起 · 跟手 · 让位 · 悬停第一格 · 放下 · 等回包），不判顺序。
 * 行的几何是夹具（每行高 40、行距 8）；可不可挪照回包那一格（夹具里写好）。期望手写。
 */
import { describe, expect, it } from "vitest";
import { OrderDrag, type DragRow } from "../../../src/frontend/ui/rot-drag";

/** 第 0 行在用 · 1–3 可挪 · 4 用不了。 */
const ROWS: DragRow[] = ["team", "lab", "personal", "api", "work"].map((account, k) => ({
  account,
  movable: k >= 1 && k <= 3,
  top: k * 48,
  height: 40,
}));

const fresh = (online = true): OrderDrag => new OrderDrag(() => online);

describe("鼠标：按下 · 拿起 · 跟手 · 让位", () => {
  it("★ 按下挪 30px：拿起、被拖的那一行跟着走 30、它下面那一行已让位；松手前什么都不交", () => {
    const d = fresh();
    d.press(ROWS, 2, 100);
    d.move(130);
    expect(d.phase).toBe("lifted");
    expect(d.offset(2)).toBe(30);
    expect(d.offset(3), "下面那一行往上让一格").toBe(-48);
    expect(d.offset(1)).toBe(0);
    expect(d.slot).toBe(3);
  });

  it("★ 没挪过 4px 就松开 ⇒ 当点；挪过 4px 才拿起", () => {
    const d = fresh();
    d.press(ROWS, 1, 60);
    d.move(63);
    expect(d.phase).toBe("pressed");
    expect(d.release()).toEqual({ kind: "click", account: "lab" });
    expect(d.phase).toBe("idle");
  });

  it("★ 不可挪的行（用不了 · 在用）挪过 4px 也不拿起，松开不当点也不交", () => {
    for (const i of [0, 4]) {
      const d = fresh();
      d.press(ROWS, i, i * 48 + 10);
      d.move(i * 48 + 40);
      expect(d.phase).toBe("pressed");
      expect(d.release()).toEqual({ kind: "none" });
    }
  });

  it("★ 跟手夹在可挪那一组里：拖进「用不了」那一组 ⇒ 停在可挪的末尾；往上拖到第一格 ⇒ 悬停第一格（在用那一行让到第二格）", () => {
    const d = fresh();
    d.press(ROWS, 1, 60);
    d.move(400);
    expect(d.slot).toBe(3);
    expect(d.offset(1), "浮起的行不出可挪那一组").toBe(96);
    d.move(60 - 48);
    expect(d.slot).toBe(0);
    expect(d.first).toBe(true);
    expect(d.offset(0), "在用那一行滑到第 2 格").toBe(48);
  });

  it("★ 松手 ⇒ 交拖的号与松在第几格，进等回包（列表不收新的拖动）；回包之后才回静止", () => {
    const d = fresh();
    d.press(ROWS, 3, 160);
    d.move(160 - 100);
    expect(d.release()).toEqual({ kind: "drop", account: "api", to: 1 });
    expect(d.phase).toBe("waiting");
    expect(d.press(ROWS, 1, 60), "等回包时按下不起").toBe(false);
    d.settle();
    expect(d.phase).toBe("idle");
    expect(d.offset(3)).toBe(0);
  });

  it("★ 空槽回到原位松开 · Esc ⇒ 退回，不交", () => {
    const d = fresh();
    d.press(ROWS, 2, 100);
    d.move(130);
    d.move(102);
    expect(d.release()).toEqual({ kind: "back" });
    d.press(ROWS, 2, 100);
    d.move(40);
    d.cancel();
    expect(d.phase).toBe("idle");
    expect(d.offset(1)).toBe(0);
  });

  it("★ 离线 ⇒ 不拿起", () => {
    const d = fresh(false);
    d.press(ROWS, 2, 100);
    d.move(160);
    expect(d.phase).toBe("pressed");
    expect(d.release()).toEqual({ kind: "none" });
  });
});

describe("键盘：拿起 · 挪 · 放下（同一张状态表）", () => {
  it("★ 第 3 行拿起 · ↑ · ↑ ⇒ 空槽到第一格；放下 ⇒ 交 to 0", () => {
    const d = fresh();
    expect(d.pick(ROWS, 2)).toBe(true);
    d.step(-1);
    expect(d.slot).toBe(1);
    expect(d.offset(2), "浮起的行挪到空槽上").toBe(-48);
    d.step(-1);
    d.step(-1);
    expect(d.slot, "到顶不再挪").toBe(0);
    expect(d.release()).toEqual({ kind: "drop", account: "personal", to: 0 });
  });

  it("★ 不可挪的行拿不起；往下挪不出可挪那一组；原位放下 ⇒ 退回", () => {
    const d = fresh();
    expect(d.pick(ROWS, 4)).toBe(false);
    expect(d.pick(ROWS, 3)).toBe(true);
    d.step(1);
    expect(d.slot).toBe(3);
    expect(d.release()).toEqual({ kind: "back" });
  });
});
