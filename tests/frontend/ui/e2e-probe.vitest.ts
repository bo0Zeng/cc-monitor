/** Batch13-F40c:抖动探针的反转计数器单测(纯函数) */
import { afterEach, describe, expect, it } from "vitest";
import { countReversals, withTabRows } from "../../../src/frontend/ui/e2e-probe";

type Box = { left: number; top: number; width: number; height: number };
function boxed<T extends HTMLElement>(el: T, b: Box): T {
  el.getBoundingClientRect = () =>
    ({ ...b, x: b.left, y: b.top, right: b.left + b.width, bottom: b.top + b.height, toJSON: () => b }) as DOMRect;
  return el;
}
function row(b: Box, cls = "tab"): HTMLButtonElement {
  const r = boxed(document.createElement("button"), b);
  r.className = cls;
  return r;
}

// f40 套件点 tab 行的坐标从快照里来（栏顶那排图标 · 「需手动」条会把行往下推，套件不再写死像素）。
describe("withTabRows：快照带上每个看得见的 tab 行", () => {
  afterEach(() => {
    document.body.innerHTML = "";
  });

  it("行的点击点（设备像素）· 哪一行亮着；栏顶的图标、起动中的槽位行、收起的行、滚出列表的行不算", () => {
    const list = boxed(document.createElement("div"), { left: 0, top: 40, width: 240, height: 200 });
    list.className = "tab-list";
    const head = row({ left: 0, top: 0, width: 240, height: 36 }, "tab-bar-head");
    const a = row({ left: 0, top: 40, width: 240, height: 30 });
    const b = row({ left: 0, top: 70, width: 240, height: 30 }, "tab active");
    const folded = row({ left: 0, top: 0, width: 0, height: 0 });
    const below = row({ left: 0, top: 260, width: 240, height: 30 });
    const slot = row({ left: 0, top: 100, width: 240, height: 30 });
    slot.dataset.slot = "0";
    list.append(a, b, folded, below, slot);
    document.body.append(head, list);

    const snap = JSON.parse(withTabRows(JSON.stringify({ sid: "abcd1234" }), 2));
    expect(snap.sid).toBe("abcd1234");
    expect(snap.rows).toEqual([
      { x: 192, y: 110, active: false },
      { x: 192, y: 170, active: true },
    ]);
    expect(snap.activeRow).toBe(1);
  });

  it("页面上没有 tab 栏（整页换成了别的视图）⇒ rows 空、activeRow null", () => {
    const snap = JSON.parse(withTabRows(JSON.stringify({ active: null }), 1));
    expect(snap).toEqual({ active: null, rows: [], activeRow: null });
  });
});

describe("countReversals", () => {
  it("单调序列(贴底跟随内容增长上移)= 0 反转", () => {
    expect(countReversals([100, 90, 80, 70])).toBe(0);
    expect(countReversals([70, 80, 90])).toBe(0);
  });

  it("±0.5px 高频抖动(§21 病根形态)被计出", () => {
    expect(countReversals([100, 100.5, 100, 100.5, 100])).toBe(3);
  });

  it("阈值滤亚像素噪声:±0.2px 不算位移", () => {
    expect(countReversals([100, 100.2, 100.05, 100.15])).toBe(0);
  });

  it("空/单元素/恒定序列 = 0", () => {
    expect(countReversals([])).toBe(0);
    expect(countReversals([5])).toBe(0);
    expect(countReversals([5, 5, 5])).toBe(0);
  });

  it("单向大位移后回摆一次 = 1", () => {
    expect(countReversals([100, 50, 20, 24])).toBe(1);
  });
});
