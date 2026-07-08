/** Batch13-F40a:TailWindow 账本单测(拍住单洞后缀语义) */
import { describe, expect, it } from "vitest";
import type { JsonlLinePayload } from "./events";
import { TailWindow } from "./live-window";

function mk(seq: number): JsonlLinePayload {
  return { seq, session_id: "s", cwd: null, path: "/p", message: { type: "x" } } as unknown as JsonlLinePayload;
}

describe("TailWindow", () => {
  it("virgin:floor 为 null,admit 恒 false", () => {
    const w = new TailWindow();
    expect(w.floorSeq).toBeNull();
    expect(w.admit(0)).toBe(false);
    expect(w.admit(999)).toBe(false);
  });

  it("pinFloor 后 admit 按边界判定(>=floor)", () => {
    const w = new TailWindow();
    w.pinFloor(100);
    expect(w.admit(100)).toBe(true);
    expect(w.admit(101)).toBe(true);
    expect(w.admit(99)).toBe(false);
  });

  it("pinFloor 幂等取 min:只降不升", () => {
    const w = new TailWindow();
    w.pinFloor(100);
    w.pinFloor(200);
    expect(w.floorSeq).toBe(100);
    w.pinFloor(50);
    expect(w.floorSeq).toBe(50);
  });

  it("顺序 defer(块内升序)→ takeTail 升序返回最高 k 条并压水位", () => {
    const w = new TailWindow();
    for (let i = 0; i < 10; i++) w.defer(mk(i));
    const t = w.takeTail(3);
    expect(t.map((p) => p.seq)).toEqual([7, 8, 9]);
    expect(w.floorSeq).toBe(7);
    expect(w.pendingCount).toBe(7);
  });

  it("乱序块(末块先到)归位:takeTail 仍取全局最高段", () => {
    const w = new TailWindow();
    // 到达序:块2(500-504)→ 块1(300-304)→ 块1.5(400-404)
    for (let s = 500; s < 505; s++) w.defer(mk(s));
    for (let s = 300; s < 305; s++) w.defer(mk(s));
    for (let s = 400; s < 405; s++) w.defer(mk(s));
    const t = w.takeTail(7);
    expect(t.map((p) => p.seq)).toEqual([403, 404, 500, 501, 502, 503, 504]);
    expect(w.floorSeq).toBe(403);
    expect(w.pendingCount).toBe(8);
  });

  it("takeTail 连续调用:水位逐段下降,段间升序衔接", () => {
    const w = new TailWindow();
    for (let i = 0; i < 10; i++) w.defer(mk(i));
    const a = w.takeTail(4);
    const b = w.takeTail(4);
    expect(a.map((p) => p.seq)).toEqual([6, 7, 8, 9]);
    expect(b.map((p) => p.seq)).toEqual([2, 3, 4, 5]);
    expect(w.floorSeq).toBe(2);
  });

  it("takeTail 超量:全弹光,账清空", () => {
    const w = new TailWindow();
    w.defer(mk(1));
    w.defer(mk(2));
    const t = w.takeTail(100);
    expect(t.map((p) => p.seq)).toEqual([1, 2]);
    expect(w.pendingCount).toBe(0);
    expect(w.floorSeq).toBe(1);
  });

  it("空账 takeTail:返回 [],floor 不动", () => {
    const w = new TailWindow();
    w.pinFloor(42);
    expect(w.takeTail(5)).toEqual([]);
    expect(w.floorSeq).toBe(42);
  });

  it("takeTail 后 floor 与已 pin 的更低水位取 min(不回升)", () => {
    const w = new TailWindow();
    w.pinFloor(3);
    w.defer(mk(10));
    w.takeTail(1);
    expect(w.floorSeq).toBe(3);
  });

  it("dispose 清账", () => {
    const w = new TailWindow();
    for (let i = 0; i < 5; i++) w.defer(mk(i));
    w.dispose();
    expect(w.pendingCount).toBe(0);
    expect(w.takeTail(3)).toEqual([]);
  });
});
