/**
 * audit-0805 F15：**live 模式下每来一行的主线重算，必须攒到帧末算一次**。
 *
 * # 它钉的那条链（V5 复核 + 本轮实测）
 *
 * `tabs.ts:810` 把 `onBranchRecord` 挂到 `tab.branchFolder.recordAdded`，
 * 而 `branch-fold.ts` 的 live 分支**每条记录都跑一次** `computeMain()` ——
 * 里面是 `computeMainBranch`（Kahn 拓扑，**扫全部 records**）+ `exemptQueuedLeaves`，
 * 变了还要再走一次 O(N) 的 DOM `rebuild()`。
 *
 * ⇒ N 条记录 = **O(N²)**。仓里已确诊过它的用户后果（`events.ts:150-152` 逐字：
 * 「每条 record 都走 per-record O(N) `computeMainBranch` ⇒ 启动后明显第二次卡顿
 * （**用户报告「先快一会儿然后变慢」**）」）——那条说的是同族的另一处，这里是本体。
 *
 * # 为什么这个文件是新建的
 *
 * `BranchFolder` **此前没有任何专属单测**：`tabs.vitest.ts` 把它整个 stub 成空壳，
 * `branch-button.vitest.ts` 只碰按钮。也就是说这条 O(N²) **今天完全没有判据看着** ——
 * 而 V5 已经指出它「行为上与不改完全等价（同样的卡、同样的折叠），**慢不会让任何测试变红**」。
 *
 * # 判据钉什么
 *
 * 钉的是 **`computeMainBranch` 被调了几次**，不是「快不快」。
 * 次数是可判定的；「卡不卡」要真机 + 大历史库，红线内做不到（诚实边界）。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import type { BranchRecord } from "./branching";

const spy = vi.hoisted(() => ({ compute: 0, lastLen: -1 }));

vi.mock("./branching", async (importOriginal) => {
  const real = await importOriginal<typeof import("./branching")>();
  return {
    ...real,
    computeMainBranch: (recs: ReadonlyArray<BranchRecord>) => {
      spy.compute++;
      spy.lastLen = recs.length;
      return real.computeMainBranch(recs);
    },
  };
});

import { BranchFolder } from "./branch-fold";

/** 一条 off-main 记录：parent 指向更早的节点，好让主线真的会变。 */
function rec(i: number, parent: string | null): BranchRecord {
  return {
    uuid: `u-${i}`,
    parentUuid: parent,
    timestamp: new Date(Date.UTC(2026, 7, 6, 0, 0, i)).toISOString(),
  } as BranchRecord;
}

function mount(): { el: HTMLElement; folder: BranchFolder } {
  const el = document.createElement("div");
  document.body.replaceChildren(el);
  // 容器里先摆几张带 uuid 的卡，rebuild 才有东西可折。
  for (let i = 0; i < 8; i++) {
    const card = document.createElement("div");
    card.dataset.uuid = `u-${i}`;
    el.append(card);
  }
  return { el, folder: new BranchFolder(el) };
}

describe("F15 live 模式的主线重算合批", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    spy.compute = 0;
    spy.lastLen = -1;
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("★ 一帧内连来 8 条 → `computeMainBranch` 只跑一次，不是八次", () => {
    const { folder } = mount();
    for (let i = 0; i < 8; i++) folder.recordAdded(rec(i, i === 0 ? null : `u-${i - 1}`));

    vi.advanceTimersByTime(50);
    // 抽取器自检：mock 没挂上 / 记录被去重吞掉时，下面那条会零命中地绿。
    // 用**算的时候手里有几条 records** 做自检 —— 它与「算了几次」是两个独立的量。
    expect(
      spy.lastLen,
      `computeMainBranch 最后一次拿到 ${spy.lastLen} 条 records（该是 8）—— ` +
        "要么 mock 没挂上、要么记录被去重吞了。那样下面的次数断言没有意义。",
    ).toBe(8);
    expect(
      spy.compute,
      `一帧内连来 8 条记录，computeMainBranch 跑了 ${spy.compute} 次。` +
        "它是 Kahn 拓扑、每次扫全部 records ⇒ 逐条跑就是 O(N²)，" +
        "而 N 是会话里的记录数（几百到几千）。攒到帧末算一次即可 —— " +
        "本类头注自己写着 live 的契约是「每条 **1 帧内**反映 fold 状态」，不是「每条立刻」。",
    ).toBe(1);
  });

  it("★ 反向：合批不许变成「永远不算」", () => {
    const { folder } = mount();
    folder.recordAdded(rec(0, null));
    folder.recordAdded(rec(1, "u-0"));
    vi.advanceTimersByTime(50);
    expect(spy.compute, "帧末也没算 —— 那不是合批，是把主线计算关掉了").toBeGreaterThan(0);
  });

  it("★ 帧内不算：排程之前不许有人偷偷同步算了", () => {
    const { folder } = mount();
    for (let i = 0; i < 3; i++) folder.recordAdded(rec(i, i === 0 ? null : `u-${i - 1}`));
    expect(
      spy.compute,
      `还没到帧末就已经算了 ${spy.compute} 次 —— live 分支仍在同步跑 computeMain`,
    ).toBe(0);
  });

  it("batch 模式照旧：攒着不算，flushPending 才算一次", () => {
    const { folder } = mount();
    folder.setBatchMode(true);
    for (let i = 0; i < 6; i++) folder.recordAdded(rec(i, i === 0 ? null : `u-${i - 1}`));
    vi.advanceTimersByTime(50);
    expect(spy.compute, "batch 模式下不该算").toBe(0);
    folder.flushPending();
    expect(spy.compute, "flushPending 该算一次").toBe(1);
  });

  it("`flushPending` 之后那次排程不许再空跑一遍 O(N)", () => {
    const { folder } = mount();
    for (let i = 0; i < 4; i++) folder.recordAdded(rec(i, i === 0 ? null : `u-${i - 1}`));
    folder.flushPending(); // 调用方自己先同步刷了
    const afterSync = spy.compute;
    expect(afterSync, "flushPending 自己该算一次").toBe(1);
    vi.advanceTimersByTime(50);
    expect(
      spy.compute,
      "帧末那次排程又白算了一遍 —— 同步刷过之后该把待办清掉，否则合批只省了一半",
    ).toBe(afterSync);
  });
});
