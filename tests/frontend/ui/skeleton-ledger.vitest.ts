/**
 * 〔骨架〕骨架账本 + 第一级粗估的判据。
 *
 * 买到：总条数 / 总高 / 像素→seq / 每行的料四件事**不看 DOM、不看正文**就答得出（账本按列收帧面，`factsOf` 现拼与逐行逐格相等）；
 * 纯工具记录一条一张「几步」卡（界面不并组）；列宽变了能整份重估。
 * **买不到**：估得准不准 —— 那归秤 2 / 真引擎（本文件只钉算术与形状，常数本身没有对拍）。
 */
import { describe, expect, it } from "vitest";
import type { JsonlLinePayload } from "../../../src/frontend/ui/events";
import { estimateFromFacts, SKEL_OUTER, skeletonKind, type SkeletonFacts } from "../../../src/frontend/ui/height-estimate";
import { SkeletonLedger, TailWindow } from "../../../src/frontend/ui/live-window";
import { indexPages } from "../../test-support/pack-index";

const ledger = (base: number, rows: SkeletonFacts[], colW?: number): SkeletonLedger => new SkeletonLedger(base, indexPages(rows), colW);

const user = (ch: number, extra: Partial<SkeletonFacts> = {}): SkeletonFacts => ({
  o: 0,
  n: 1,
  t: "said",
  ch,
  pl: ch > 0 ? 1 : 0,
  ...extra,
});
const asst = (ch: number, extra: Partial<SkeletonFacts> = {}): SkeletonFacts => ({
  o: 0,
  n: 1,
  t: "reply",
  ch,
  pl: ch > 0 ? 1 : 0,
  ...extra,
});
const toolOnly = (t: SkeletonFacts["t"] = "reply"): SkeletonFacts => ({ o: 0, n: 1, t, fd: 1 });
const none = (t?: SkeletonFacts["t"]): SkeletonFacts => (t ? { o: 0, n: 1, t } : { o: 0, n: 1 });

describe("estimateFromFacts / skeletonKind", () => {
  it("不建卡的记录高 0：系统注入 · 本地命令回显 / 不进界面的行 / 标题 / 解析不出的", () => {
    for (const f of [user(10, { sp: "system" }), user(10, { sp: "commandOutput" }), none(), none("title"), { o: 0, n: 1 }]) {
      expect(skeletonKind(f)).toBe("none");
      expect(estimateFromFacts(f)).toBe(0);
    }
  });

  it("认的是通用记录的类：said / queued 按人那一侧的气泡、reply 按回复卡、retry 是细条；盘上的原词不再认", () => {
    expect(skeletonKind(user(10))).toBe("card");
    expect(skeletonKind({ o: 0, n: 1, t: "queued", ch: 10, pl: 1 })).toBe("card");
    expect(estimateFromFacts({ o: 0, n: 1, t: "queued", ch: 10, pl: 1 })).toBe(estimateFromFacts(user(10)));
    expect(skeletonKind(asst(10))).toBe("card");
    expect(estimateFromFacts({ o: 0, n: 1, t: "retry" })).toBe(SKEL_OUTER.system);
    for (const raw of ["user", "assistant", "system"]) expect(skeletonKind({ o: 0, n: 1, t: raw as never, ch: 10, pl: 1 })).toBe("none");
  });

  it("后端判为建卡的来源照常出高（斜杠命令 · 压缩摘要 · 派给子 agent 的活）", () => {
    for (const sp of ["slashCommand", "compactSummary", "agentTask"]) expect(skeletonKind(user(10, { sp }))).toBe("card");
    // agent 来话 · 另一会话 · 主会话后来的话 · 后台通知 · 中断标记画成事件条，照常出高。
    for (const sp of ["agentMessage", "peerSession", "coordinator", "taskNotification", "interrupt"]) expect(skeletonKind(user(10, { sp }))).toBe("card");
  });

  it("纯工具记录：回复那一侧一条一张「几步」卡（每步一行 ＋ 步间距 ＋ 外距，不看左邻居）；人那一侧的工具结果注入进调用那一行、高 0", () => {
    expect(estimateFromFacts(toolOnly())).toBe(38 + SKEL_OUTER.steps);
    expect(estimateFromFacts({ ...toolOnly(), fd: 3 })).toBe(3 * 38 + 2 * 4 + SKEL_OUTER.steps);
    expect(estimateFromFacts(toolOnly("said"))).toBe(0);
  });

  it("正文越长越高；窄列比宽列高（宽度相关的那一半在前端算）", () => {
    const short = estimateFromFacts(asst(20));
    const long = estimateFromFacts(asst(4000, { pl: 20 }));
    expect(long).toBeGreaterThan(short);
    expect(estimateFromFacts(asst(4000, { pl: 20 }), 400)).toBeGreaterThan(long);
  });

  it("代码行按等宽行高线性", () => {
    const one = estimateFromFacts(asst(1, { cb: 1, cl: 1 }));
    const eleven = estimateFromFacts(asst(1, { cb: 1, cl: 11 }));
    expect(eleven - one).toBeCloseTo(10 * 13 * 1.55);
  });
});

describe("SkeletonLedger", () => {
  // 8 行：user · asst · tool · attachment · tool · asst · none · user
  const rows: SkeletonFacts[] = [
    user(10),
    asst(100),
    toolOnly(),
    none(),
    toolOnly("said"),
    asst(50),
    none("title"),
    user(5),
  ];

  it("折着的过程（setFolds 的那几段）按 0 高：区间高 · 总高 · seqAt 都跳过它；带过程行的开头加一条行高；换一组 / 清空 ⇒ 回原高；精算 · 列宽重估后照样 0", () => {
    const l = ledger(0, rows);
    const fold = (folded: Array<[number, number]>, lines: number[] = [], linePx = 0) => l.setFolds({ folded, lines, linePx });
    const full = l.totalHeight;
    const mid = l.heightOf(1, 3);
    const head = l.heightOf(0, 1);
    expect(fold([[1, 3]])).toBe(true);
    expect(l.heightOf(1, 3)).toBe(0);
    expect(l.totalHeight).toBeCloseTo(full - mid);
    expect(l.heightOf(0, 8)).toBeCloseTo(full - mid);
    expect(l.seqAt(0, 8, l.heightOf(0, 1) + 0.5)).toBeGreaterThanOrEqual(3);
    expect(fold([[1, 3]])).toBe(false);
    expect(fold([[1, 3]], [0], 40)).toBe(true);
    expect(l.heightOf(0, 1)).toBeCloseTo(head + 40);
    expect(l.totalHeight).toBeCloseTo(full - mid + 40);
    l.refine([[1, 999]]);
    expect(l.heightOf(1, 3)).toBe(0);
    l.relayout(500);
    expect(l.heightOf(1, 3)).toBe(0);
    expect(fold([])).toBe(true);
    expect(l.heightOf(1, 3)).toBeCloseTo(ledger(0, rows, 500).heightOf(1, 3));
    expect(l.heightOf(1, 3)).toBeGreaterThan(0);
  });

  it("总条数 / seq 边界", () => {
    const l = ledger(0, rows);
    expect(l.count).toBe(8);
    expect(l.endSeq).toBe(8);
    expect(l.factsOf(-1)).toBeUndefined();
    expect(l.factsOf(8)).toBeUndefined();
  });

  it("按列收：factsOf 现拼的每一行与逐行的料逐格相等（偏移 · 类 · 稀疏的正文与来源 · 跨两页、两页的类表不同）", () => {
    const a: SkeletonFacts[] = [
      { o: 0, n: 40, t: "said", ch: 12, cj: 3, pl: 2 },
      { o: 41, n: 9 },
      { o: 50, n: 70, t: "reply", ch: 900, pl: 4, cb: 1, cl: 7, fd: 2 },
      { o: 120, n: 30, t: "said", sp: "system" },
    ];
    const b: SkeletonFacts[] = [
      { o: 150, n: 5, t: "retry" },
      { o: 155, n: 60, t: "reply", fd: 1 },
      { o: 300, n: 12, t: "said", ch: 1, pl: 1, sp: "agentMessage" },
    ];
    const l = new SkeletonLedger(10, [...indexPages(a), ...indexPages(b)]);
    expect(Array.from({ length: l.count }, (_, i) => l.factsOf(10 + i))).toEqual([...a, ...b]);
  });

  it("不建卡的行 · 人那一侧的工具结果高 0；回复那一侧的纯工具记录各自出高", () => {
    const l = ledger(0, rows);
    expect(l.heightOf(3, 5)).toBe(0); // attachment(0) + 工具结果(0)
    expect(l.heightOf(2, 3)).toBe(estimateFromFacts(toolOnly()));
  });

  it("heightOf 是可加的，totalHeight = 全段", () => {
    const l = ledger(0, rows);
    expect(l.heightOf(0, 3) + l.heightOf(3, 8)).toBeCloseTo(l.totalHeight);
    expect(l.heightOf(5, 5)).toBe(0);
    expect(l.heightOf(-10, 999)).toBeCloseTo(l.totalHeight); // 越界按 0 算
  });

  it("seqAt：像素 → 覆盖它的那一条；零高行不会被单独命中", () => {
    const l = ledger(0, rows);
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
    const l = ledger(100, [toolOnly()]);
    expect(l.endSeq).toBe(101);
    l.append(indexPages([toolOnly("said"), user(3)]));
    expect(l.endSeq).toBe(103);
    expect(l.heightOf(101, 102)).toBe(0);
    expect(l.factsOf(102)?.t).toBe("said");
    expect(l.seqAt(100, 103, 0)).toBe(100);
  });

  it("relayout：换列宽整份重估，条数不变", () => {
    const l = ledger(0, [asst(3000, { pl: 5 })]);
    const wide = l.totalHeight;
    l.relayout(300);
    expect(l.totalHeight).toBeGreaterThan(wide);
    expect(l.count).toBe(1);
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
