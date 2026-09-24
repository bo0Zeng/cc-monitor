/**
 * 〔`设计/10` 骨架 · 子步 2〕骨架账本 + 第一级粗估的判据。
 *
 * 买到：总条数 / 总高 / 像素→seq / uuid→seq 四件事**不看 DOM、不看正文**就答得出；
 * 工具组「一串只出一张卡」在骨架里也成立；列宽变了能整份重估。
 * **买不到**：估得准不准 —— 那归秤 2 / 真引擎（本文件只钉算术与形状，常数本身没有对拍）。
 */
import { describe, expect, it } from "vitest";
import type { JsonlLinePayload } from "../src/events";
import { estimateFromFacts, skeletonKind, type SkeletonFacts } from "../src/height-estimate";
import { SkeletonLedger, TailWindow } from "../src/live-window";

const user = (ch: number, extra: Partial<SkeletonFacts> = {}): SkeletonFacts => ({
  o: 0,
  n: 1,
  t: "user",
  ch,
  pl: ch > 0 ? 1 : 0,
  ...extra,
});
const asst = (ch: number, extra: Partial<SkeletonFacts> = {}): SkeletonFacts => ({
  o: 0,
  n: 1,
  t: "assistant",
  ch,
  pl: ch > 0 ? 1 : 0,
  ...extra,
});
const toolOnly = (t = "assistant"): SkeletonFacts => ({ o: 0, n: 1, t, fd: 1 });
const none = (t = "attachment"): SkeletonFacts => ({ o: 0, n: 1, t });

describe("estimateFromFacts / skeletonKind", () => {
  it("不建卡的记录高 0：meta / attachment / ai-title / 解析不出的", () => {
    for (const f of [user(10, { mt: true }), none(), none("ai-title"), { o: 0, n: 1 }]) {
      expect(skeletonKind(f)).toBe("none");
      expect(estimateFromFacts(f, "card")).toBe(0);
    }
  });

  it("连续的纯工具记录只有第一条出高（并成一张工具组卡）", () => {
    const first = estimateFromFacts(toolOnly(), "card");
    expect(first).toBeGreaterThan(0);
    expect(estimateFromFacts(toolOnly("user"), "tool")).toBe(0);
  });

  it("正文越长越高；窄列比宽列高（宽度相关的那一半在前端算）", () => {
    const short = estimateFromFacts(asst(20), "none");
    const long = estimateFromFacts(asst(4000, { pl: 20 }), "none");
    expect(long).toBeGreaterThan(short);
    expect(estimateFromFacts(asst(4000, { pl: 20 }), "none", 400)).toBeGreaterThan(long);
  });

  it("代码行按等宽行高线性", () => {
    const one = estimateFromFacts(asst(1, { cb: 1, cl: 1 }), "none");
    const eleven = estimateFromFacts(asst(1, { cb: 1, cl: 11 }), "none");
    expect(eleven - one).toBeCloseTo(10 * 13 * 1.55);
  });
});

describe("SkeletonLedger", () => {
  // 8 行：user · asst · tool · attachment · tool · asst · none · user
  const rows: SkeletonFacts[] = [
    user(10, { u: "a" }),
    asst(100, { u: "b" }),
    toolOnly(),
    none(),
    toolOnly("user"),
    asst(50, { u: "f" }),
    none("ai-title"),
    user(5, { u: "h" }),
  ];

  it("总条数 / seq 边界 / uuid 定位", () => {
    const l = new SkeletonLedger(0, rows);
    expect(l.count).toBe(8);
    expect(l.endSeq).toBe(8);
    expect(l.uuidToSeq.get("f")).toBe(5);
    expect(l.uuidToSeq.has("zzz")).toBe(false);
  });

  it("工具组跨过不建卡的行仍并成一张（attachment 不打断）", () => {
    const l = new SkeletonLedger(0, rows);
    expect(l.heightOf(3, 5)).toBe(0); // attachment(0) + 第二条工具(0)
    expect(l.heightOf(2, 3)).toBeGreaterThan(0);
  });

  it("heightOf 是可加的，totalHeight = 全段", () => {
    const l = new SkeletonLedger(0, rows);
    expect(l.heightOf(0, 3) + l.heightOf(3, 8)).toBeCloseTo(l.totalHeight);
    expect(l.heightOf(5, 5)).toBe(0);
    expect(l.heightOf(-10, 999)).toBeCloseTo(l.totalHeight); // 越界按 0 算
  });

  it("seqAt：像素 → 覆盖它的那一条；零高行不会被单独命中", () => {
    const l = new SkeletonLedger(0, rows);
    expect(l.seqAt(0, 8, -5)).toBe(0);
    expect(l.seqAt(0, 8, 0)).toBe(0);
    const top1 = l.heightOf(0, 1);
    expect(l.seqAt(0, 8, top1)).toBe(1); // 恰好落在第 1 行顶
    expect(l.seqAt(0, 8, top1 - 0.01)).toBe(0);
    // 第 3、4 行高 0：第 2 行底之后的第一个像素属于第 5 行
    expect(l.seqAt(0, 8, l.heightOf(0, 3) + 0.01)).toBe(5);
    expect(l.seqAt(0, 8, 1e9)).toBe(7);
    // 子段内的相对坐标
    expect(l.seqAt(5, 8, 0)).toBe(5);
  });

  it("base ≠ 0（续传）：seq 从 base 起算；append 接上且工具串跨 append 边界仍连续", () => {
    const l = new SkeletonLedger(100, [toolOnly()]);
    expect(l.endSeq).toBe(101);
    l.append([toolOnly("user"), user(3, { u: "z" })]);
    expect(l.endSeq).toBe(103);
    expect(l.heightOf(101, 102)).toBe(0);
    expect(l.uuidToSeq.get("z")).toBe(102);
    expect(l.seqAt(100, 103, 0)).toBe(100);
  });

  it("relayout：换列宽整份重估，条数与 uuid 不变", () => {
    const l = new SkeletonLedger(0, [asst(3000, { pl: 5, u: "q" })]);
    const wide = l.totalHeight;
    l.relayout(300);
    expect(l.totalHeight).toBeGreaterThan(wide);
    expect(l.count).toBe(1);
    expect(l.uuidToSeq.get("q")).toBe(0);
  });
});

describe("TailWindow.takeRange（岛）", () => {
  const mk = (seq: number) =>
    ({ seq, session_id: "s", cwd: null, path: "/p", message: {} }) as unknown as JsonlLinePayload;

  it("取出 [lo,hi) 的那些、升序、出账；不动 floor", () => {
    const w = new TailWindow();
    w.pinFloor(50);
    for (const s of [40, 41, 42, 10, 11, 12, 13]) w.defer(mk(s)); // 乱序块
    const got = w.takeRange(11, 41);
    expect(got.map((p) => p.seq)).toEqual([11, 12, 13, 40]);
    expect(w.pendingCount).toBe(3);
    expect(w.floorSeq).toBe(50);
    // takeTail 仍取 floor 之下最近的
    expect(w.takeTail(2).map((p) => p.seq)).toEqual([41, 42]);
    expect(w.takeRange(0, 0)).toEqual([]);
  });
});
