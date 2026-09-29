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
import type { BranchRecord } from "../../../src/frontend/ui/branching";

const spy = vi.hoisted(() => ({ compute: 0, lastLen: -1 }));

vi.mock("../../../src/frontend/ui/branching", async (importOriginal) => {
  const real = await importOriginal<typeof import("../../../src/frontend/ui/branching")>();
  return {
    ...real,
    computeMainBranch: (recs: ReadonlyArray<BranchRecord>) => {
      spy.compute++;
      spy.lastLen = recs.length;
      return real.computeMainBranch(recs);
    },
  };
});

import { BranchFolder } from "../../../src/frontend/ui/branch-fold";

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

/** 与 `rec()` 同一套时间轴，单独拿出来给下面那条 DOM 对拍用。 */
function iso(i: number): string {
  return new Date(Date.UTC(2026, 7, 6, 0, 0, i)).toISOString();
}

/**
 * `设计/17 §7` 第 6 条 ＝ `§2.7` 档 3：**`addQueuedContent` 接上同一个帧末合批**。
 *
 * # 它钉的那条链
 *
 * `tabs.ts` 的 `onQueueOperation` 把**每一条** enqueue 记录喂给
 * `BranchFolder.addQueuedContent`，而那里原先在 live 模式下**同步**跑一次
 * `computeMain()`（`computeMainBranch` 是扫全部 records 的 Kahn 拓扑 ⇒ O(N)，
 * 变了还要再走一次 O(N) 的 DOM `rebuild()`）——
 * **与 F15 修掉之前的 `recordAdded` 是同一个形状**：M 条 enqueue × O(N) = O(M·N)，
 * 而且与同一帧里 `recordAdded` 排的那次**各算各的**。
 *
 * `设计/17 §2.7` 逐字把它列在「一个没合批的兄弟」下面，修法逐字是「接上合批 —— 一行，立刻做」。
 *
 * # 判据钉什么
 *
 * 与上面 F15 那一组同一套口径：钉 **`computeMainBranch` 被调了几次**（可判定），
 * 不钉「快不快」（那要真机 ＋ 大历史库，红线内做不到 —— 诚实边界）。
 * 另加一条**行为不变**的相等断言：合批只许改「什么时候算」，不许改「算出什么」。
 *
 * # 它挡不住什么（诚实段 · 死值验现打出来的）
 *
 * 🔴 最后那条 DOM 相等断言**逮不住「`addQueuedContent` 干脆不排程」** ——
 * 死值验 `M7`（把那行 `scheduleLiveRecompute()` 整个删掉）实测：那一条**照样绿**。
 * 成因是它的夹具里先喂了 records，`recordAdded` 已经替它排了同一帧的那次活，
 * 帧末真算时 `queuedContents` 早就写进去了 ⇒ 豁免照样生效。
 * ⇒ **那一格量的是「算出什么」，不是「谁排的活」**；「谁排的活」归上面
 * 「反向：不许变成永远不算」那条（`M7` 下它红），两条合起来才盖满。
 */
describe("`设计/17 §2.7` 档 3：addQueuedContent 接上帧末合批", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    spy.compute = 0;
    spy.lastLen = -1;
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("★ 一帧内连来 8 条 enqueue → `computeMainBranch` 只跑一次，不是八次", () => {
    const { folder } = mount();
    // 先把 records 那一批喂进去并结掉，否则 N=0，「省掉一趟 O(N)」量的是空气。
    for (let i = 0; i < 4; i++) folder.recordAdded(rec(i, i === 0 ? null : `u-${i - 1}`));
    vi.advanceTimersByTime(50);
    expect(spy.compute, "前置不成立：records 那一批没在帧末算掉").toBe(1);
    // 量具自检：mock 没挂上 / 记录被去重吞掉时，下面的次数断言会零命中地绿。
    expect(
      spy.lastLen,
      `computeMainBranch 最后一次手里有 ${spy.lastLen} 条 records（该是 4）—— ` +
        "要么 mock 没挂上、要么记录被吞了。那样下面的次数断言没有意义。",
    ).toBe(4);

    spy.compute = 0;
    for (let i = 0; i < 8; i++) folder.addQueuedContent(`排队的第 ${i} 句`);
    expect(
      spy.compute,
      `8 条 enqueue 在帧内就算了 ${spy.compute} 次 —— 档 3 没接上合批，还在逐条同步跑 computeMain`,
    ).toBe(0);

    vi.advanceTimersByTime(50);
    expect(
      spy.compute,
      `一帧内连来 8 条 enqueue，computeMainBranch 跑了 ${spy.compute} 次（该是 1）`,
    ).toBe(1);
  });

  it("★ enqueue 与新记录混在同一帧 → 合成同一次，不是两次", () => {
    const { folder } = mount();
    folder.recordAdded(rec(0, null));
    folder.addQueuedContent("排队的那句");
    folder.recordAdded(rec(1, "u-0"));
    folder.addQueuedContent("排队的另一句");
    vi.advanceTimersByTime(50);
    expect(
      spy.compute,
      `同一帧里两条记录 ＋ 两条 enqueue 算了 ${spy.compute} 次。` +
        "两条路要排进**同一个**待办，不是各排各的 —— 否则合批只省一半。",
    ).toBe(1);
  });

  it("★ 反向：enqueue 那条路的合批不许变成「永远不算」", () => {
    const { folder } = mount();
    folder.addQueuedContent("排队的那句");
    expect(spy.compute, "前置：enqueue 不该在帧内同步算").toBe(0);
    vi.advanceTimersByTime(50);
    expect(
      spy.compute,
      "帧末也没算 —— 那不是合批，是把队列豁免整个关掉了（豁免从此永远不生效）",
    ).toBeGreaterThan(0);
  });

  it("★ 去重 / 空白那两条早退路径不许凭空排一次活", () => {
    const { folder } = mount();
    folder.addQueuedContent("同一句");
    vi.advanceTimersByTime(50);
    expect(spy.compute, "前置：第一次该算一次").toBe(1);

    spy.compute = 0;
    folder.addQueuedContent("同一句"); // 已登记 ⇒ 早退
    folder.addQueuedContent("   "); // trim 后为空 ⇒ 早退
    vi.advanceTimersByTime(50);
    expect(
      spy.compute,
      `早退路径排了 ${spy.compute} 次帧末重算 —— 豁免集合一个字没变，那趟 O(N) 是纯白跑`,
    ).toBe(0);
  });

  it("batch 模式照旧：enqueue 攒着不算，flushPending 才算一次", () => {
    const { folder } = mount();
    folder.setBatchMode(true);
    for (let i = 0; i < 5; i++) folder.addQueuedContent(`排队的第 ${i} 句`);
    vi.advanceTimersByTime(50);
    expect(spy.compute, "batch 模式下不该算").toBe(0);
    folder.flushPending();
    expect(spy.compute, "flushPending 该算一次").toBe(1);
  });

  it("★ 行为不变：队列豁免仍然生效，帧末的 DOM 与同步路径**逐字节相同**", () => {
    const QUEUED = "这句进过输入队列";
    // u-0 底下分叉：u-1（较早，user，正文 == 那句 enqueue）与 u-2（较晚，assistant）。
    // 主线走 u-0 → u-2 ⇒ u-1 是 off-main 叶子 ⇒ 正是 `exemptQueuedLeaves` 要捞回来的那条。
    const recs: BranchRecord[] = [
      { uuid: "u-0", timestamp: iso(0), type: "user", text: "根" },
      { uuid: "u-1", parentUuid: "u-0", timestamp: iso(1), type: "user", text: QUEUED },
      { uuid: "u-2", parentUuid: "u-0", timestamp: iso(2), type: "assistant" },
    ];

    // A 路 —— 本次改动这条：帧末合批。
    const a = mount();
    for (const r of recs) a.folder.recordAdded(r);
    a.folder.addQueuedContent(QUEUED);
    vi.advanceTimersByTime(50);

    // B 路 —— 对照组：同一批输入走**同步**出口（`rebuildNow`，改动前那段内联计算的等价物）。
    const b = mount();
    for (const r of recs) b.folder.recordAdded(r);
    b.folder.addQueuedContent(QUEUED);
    b.folder.rebuildNow();

    // C 路 —— **量具自检**：同一批记录，**不喂**那句 enqueue。
    const c = mount();
    for (const r of recs) c.folder.recordAdded(r);
    c.folder.rebuildNow();

    expect(
      c.el.innerHTML,
      "量具自检不成立：这个夹具里「喂不喂那句 enqueue」根本不改变 DOM ⇒ " +
        "下面那条相等断言是空真，得换一个真能让豁免生效的夹具。",
    ).not.toBe(a.el.innerHTML);

    expect(
      a.el.innerHTML,
      "帧末合批之后的折叠结果与同步路径不一致 —— 档 3 改的该只是「什么时候算」，不是「算出什么」。",
    ).toBe(b.el.innerHTML);
  });
});

/**
 * 〔W5-RENDER R13〕`设计/10 §3.4` C1 逐字：「`BranchFolder.rebuild()` 全量 unwrap ＋ 重新包裹」—— 三处「主动放弃增量」的 DOM 操作之一。
 * 修法：按段差量（恰好等于目标段的现存 wrap 原地不动，只拆 / 建归属变了的段）。
 * 判据（异源）：随机操作序列（插卡 —— 包括插进折叠段里、按 R6 的 `insertNode` 口径插在锚点前 —— ＋ 换主线集合），
 * 每一步之后差量重折的 DOM == 同一逻辑序列在**平铺容器**上从零折一遍的 DOM（另一个实例）；
 * 以及「主线没变 / 只在尾巴长一条」时一个节点都不搬。
 */
describe("C1 · 差量重折 == 从零折（`设计/10 §3.4`）", () => {
  type Priv = { lastMainBranch: Set<string>; rebuild(): void };
  const priv = (f: BranchFolder): Priv => f as unknown as Priv;
  function rng(seed: number): () => number {
    let a = seed >>> 0;
    return () => {
      a = (a + 0x6d2b79f5) >>> 0;
      let t = a;
      t = Math.imul(t ^ (t >>> 15), t | 1);
      t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
      return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
  }
  const card = (uuid: string | null): HTMLElement => {
    const el = document.createElement("div");
    el.className = "card";
    if (uuid) el.dataset.uuid = uuid;
    el.textContent = uuid ?? "sep";
    return el;
  };
  /** 逻辑序列：顶层子节点，wrap 展开读 inner */
  const logical = (c: HTMLElement): string[] => {
    const out: string[] = [];
    for (const ch of Array.from(c.children)) {
      if (ch.classList.contains("branch-fold-wrap")) {
        for (const k of Array.from(ch.querySelector(".branch-fold-body-inner")!.children)) out.push(k.textContent ?? "");
      } else out.push(ch.textContent ?? "");
    }
    return out;
  };

  it("随机 300 步：每一步差量结果 == 从零折", () => {
    const r = rng(20260926);
    const el = document.createElement("div");
    const folder = new BranchFolder(el);
    let next = 0;
    let checked = 0;
    let foldedSeen = 0;
    for (let step = 0; step < 300; step++) {
      const op = r();
      if (op < 0.45 || el.children.length === 0) {
        // 插一张卡：末尾，或插在随机一张现有卡前（它若在折叠段里就插进段里 —— R6 口径）
        const c = card(r() < 0.1 ? null : `u${next++}`);
        const all = Array.from(el.querySelectorAll<HTMLElement>(".card"));
        if (all.length === 0 || r() < 0.5) el.appendChild(c);
        else {
          const anchor = all[Math.floor(r() * all.length)];
          anchor.parentElement!.insertBefore(c, anchor);
        }
      } else {
        // 换主线集合：每张卡独立地以 0.7 的概率在主线上
        const main = new Set<string>();
        for (const c of Array.from(el.querySelectorAll<HTMLElement>(".card"))) {
          const u = c.dataset.uuid;
          if (u && r() < 0.7) main.add(u);
        }
        priv(folder).lastMainBranch = main;
      }
      priv(folder).rebuild();
      // 参照：同一逻辑序列的平铺容器，另一个实例从零折
      const flat = document.createElement("div");
      for (const t of logical(el)) flat.appendChild(card(t === "sep" ? null : t));
      const ref = new BranchFolder(flat);
      priv(ref).lastMainBranch = new Set(priv(folder).lastMainBranch);
      priv(ref).rebuild();
      expect(el.innerHTML, `第 ${step} 步`).toBe(flat.innerHTML);
      foldedSeen += el.querySelectorAll(".branch-fold-wrap").length;
      checked++;
    }
    expect(checked).toBe(300);
    expect(foldedSeen, "反空真：序列里真出现过折叠段").toBeGreaterThan(50);
  }, 30_000); // 满载下全量套件里跑过 5 s

  it("主线没变 / 只在尾巴长一条主线卡：零搬动（DOM 一次写都没有）", () => {
    const el = document.createElement("div");
    const folder = new BranchFolder(el);
    for (let i = 0; i < 10; i++) el.appendChild(card(`k${i}`));
    priv(folder).lastMainBranch = new Set(["k0", "k1", "k5", "k6", "k9"]);
    priv(folder).rebuild(); // 折出两段：k2–k4 · k7–k8
    expect(el.querySelectorAll(".branch-fold-wrap").length).toBe(2);
    const mo = new MutationObserver(() => {});
    mo.observe(el, { childList: true, subtree: true, attributes: true });
    priv(folder).rebuild(); // 主线没变
    el.appendChild(card("k10"));
    priv(folder).lastMainBranch = new Set([...priv(folder).lastMainBranch, "k10"]);
    const before = mo.takeRecords().length; // 只有那一次 append
    priv(folder).rebuild(); // 尾巴长了一条主线卡
    const after = mo.takeRecords().length;
    mo.disconnect();
    expect(before).toBe(1);
    expect(after).toBe(0);
  });
});
