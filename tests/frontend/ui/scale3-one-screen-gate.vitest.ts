/**
 * 秤 3 ——「一屏门控」回归判据（表第 3 行）。
 *
 * 判据：「返回后视口内真实可见卡的累计高度 ≥ clientHeight」（或账本空了）。
 * 停手判据是 `TabManager::contentReachesBottom()`：读最后一张卡的真实 `getBoundingClientRect().bottom`
 * 有没有够到滚动容器下沿，不吃任何估值（代码在 `tabs.ts`）。
 *
 * jsdom 没有布局引擎 ⇒ 两段都驱动**真的** `TabManager` / `TabStreamView`，给 `getBoundingClientRect`
 * 装一个按 DOM 序堆叠的假布局（卡高按 styles.css token 手算，不是实测）。
 *
 * 复算：`npx vitest run tests/frontend/ui/scale3-one-screen-gate.vitest.ts`
 */
import { describe, it, expect, vi, afterEach } from "vitest";
import { owned } from "../../test-support/owned-tab-manager.ts";
import { announced } from "../../test-support/session-faces";

// ── 生产侧常数（逐字抄自源码，改了这里要回去核对住址）────────────────────────
/** `tabs.ts` 的 `MATERIALIZE_TAIL_K` —— 门控循环每轮取这么多条 payload */
const MATERIALIZE_TAIL_K = 150;
/** `TabManager::materializeUntilFilled` 一次同步调用的轮数上限（`round < 4`，`tabs.ts`） */
const MAX_ROUNDS = 4;
/** 典型视口。 */
const CLIENT_HEIGHT = 800;

// ── 真高：按 styles.css token 手算（**不是实测**）──────────────────────────────
const TRUE_H_PX = {
  // .card-api-retry: padding 3×2 + --font-size-xs 11px × 行高 1.55
  "card-api-retry": 3 * 2 + 11 * 1.55,
} as const;

// ─────────────────────────────────────────────────────────────────────────────
// 丁：驱动**生产的** `TabStreamView`（W5-RENDER R7）
// ─────────────────────────────────────────────────────────────────────────────
//
// 让**真的** `TabManager` / `TabStreamView` 跑一次启动重放：jsdom 没有布局 ⇒ 给 `getBoundingClientRect`
// 装一个按 DOM 序堆叠的假布局（卡高 = 上面同一份 token 手算真高 `TRUE_H_PX`，视口 800 px），
// 生产的 `contentReachesBottom` 就读得到「最后一张卡够没够到下沿」。
//
// 守的要求：「判据换成不依赖估值的量；**并连轮数封顶（`round < 4`）一起处置**」·
// 「物化循环硬上限 4 轮 × 150 条 —— 最稀疏那一档估高再准也补不满一屏」。
// 判据：重放结束、帧跑完之后，「真实可见卡高 ≥ 视口」或「账本空了」二者必居其一（稀疏风暴 8 块 × 150 条，每块 5 条 retry）；
// 同时**一次同步调用**的物化量仍有界（批结束那一下只取 ≤ 4 × 150 条）—— 余下的下一帧接着补。

vi.mock("@tauri-apps/api/core", async () => {
  const rig = await import("../../test-support/session-viewer-rig");
  const { withSessionReads } = await import("../../test-support/chan-fake");
  return {
    invoke: vi.fn(
      withSessionReads(async (cmd: string, args: { [k: string]: unknown }) =>
        cmd === "list_user_inputs" ? rig.answerListUserInputs(args as { fromOffset: number }) : undefined,
      ),
    ),
  };
});
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn().mockResolvedValue(undefined) }));
vi.mock("../../../src/frontend/ui/tasks-panel", () => ({ fetchSessionTasks: vi.fn().mockResolvedValue([]) }));
vi.mock("../../../src/frontend/ui/fork-flow", () => ({ runForkFlow: vi.fn().mockResolvedValue(undefined) }));

import { installViewerRig, line as rigLine, withSession, type RigPayload } from "../../test-support/session-viewer-rig";
import { TabManager, type Tab } from "../../../src/frontend/ui/tabs";

describe("秤 3 · 丁：生产门控（驱动真 TabStreamView ＋ 假布局）", () => {
  const VIEW_H = CLIENT_HEIGHT;
  const heightOf = (el: Element): number =>
    el.classList.contains("card-api-retry") ? TRUE_H_PX["card-api-retry"] : 0;
  const rect = (top: number, h: number): DOMRect =>
    ({ top, bottom: top + h, height: h, left: 0, right: 780, width: 780, x: 0, y: top, toJSON: () => ({}) }) as DOMRect;
  let restoreRect: (() => void) | null = null;

  /** 假布局：`.stream` = 视口；`.stream-content` 的直接子节点按 DOM 序堆叠、各取 token 手算真高。 */
  function installFakeLayout(): void {
    const proto = HTMLElement.prototype;
    const orig = proto.getBoundingClientRect;
    proto.getBoundingClientRect = function (this: HTMLElement): DOMRect {
      if (this.classList.contains("stream")) return rect(0, VIEW_H);
      const parent = this.parentElement;
      if (parent?.classList.contains("stream-content")) {
        let top = 0;
        for (const sib of Array.from(parent.children)) {
          if (sib === this) break;
          top += heightOf(sib);
        }
        return rect(top, heightOf(this));
      }
      return rect(0, 0);
    };
    restoreRect = () => {
      proto.getBoundingClientRect = orig;
    };
  }

  afterEach(() => {
    restoreRect?.();
    restoreRect = null;
    vi.unstubAllGlobals();
  });

  const retry = (seq: number): RigPayload =>
    withSession(
      rigLine(seq, { t: "retry", id: `r${seq}`, at: "2026-09-10T00:00:00.000Z", reason: "network", attempt: (seq % 5) + 1, max: 5 }),
      "s2",
    );
  const skip = (seq: number): RigPayload =>
    withSession(
      rigLine(seq, { t: "reply", id: `k${seq}`, at: "2026-09-10T00:00:00.000Z", blocks: [], autoReply: true, draw: "none", human: false }),
      "s2",
    );

  /** 稀疏风暴：`blocks` 块 × 150 条，每块前 `perBlock` 条是 retry、其余不建卡。 */
  function storm(blocks: number, perBlock: number): RigPayload[][] {
    const out: RigPayload[][] = [];
    for (let b = 0; b < blocks; b++) {
      const blk: RigPayload[] = [];
      for (let i = 0; i < MATERIALIZE_TAIL_K; i++) {
        const seq = b * MATERIALIZE_TAIL_K + i;
        blk.push(i < perBlock ? retry(seq) : skip(seq));
      }
      out.push(blk);
    }
    return out;
  }

  interface Reading {
    /** 批结束那一次同步调用之后：已建卡的真高 / 已出账（建过卡或跳过）的条数 */
    syncPx: number;
    syncTaken: number;
    /** 帧跑完之后 */
    settledPx: number;
    settledPending: number;
    frames: number;
  }

  function run(blocks: number, perBlock: number): Reading {
    const rig = installViewerRig();
    installFakeLayout();
    const barEl = document.createElement("div");
    const streamRootEl = document.createElement("div");
    document.body.append(barEl, streamRootEl);
    const tm = owned(announced(new TabManager(barEl, streamRootEl)));
    const tabOf = (sid: string): Tab =>
      (tm as unknown as { store: { tabs: Map<string, Tab> } }).store.tabs.get(sid)!;
    const blks = storm(blocks, perBlock);
    const total = blocks * MATERIALIZE_TAIL_K;
    // 启动重放的到达序：末块先发（钉 floor、直渲），其余块整块收纳
    tm.onBatchStart();
    for (const p of blks[blocks - 1]) tm.onLine(p as never);
    for (let b = 0; b < blocks - 1; b++) for (const p of blks[b]) tm.onLine(p as never);
    tm.switchTo("s2");
    tm.onBatchEnd();
    const tab = tabOf("s2");
    const px = (): number =>
      Array.from(tab.stream.contentElement.children).reduce((s, c) => s + heightOf(c), 0);
    const syncPx = px();
    const syncTaken = total - tab.window.pendingCount;
    let frames = 0;
    for (; frames < 50; frames++) {
      const before = tab.window.pendingCount;
      rig.flushRaf(1);
      if (tab.window.pendingCount === before) break;
    }
    return { syncPx, syncTaken, settledPx: px(), settledPending: tab.window.pendingCount, frames };
  }

  it("稀疏风暴（8 块 × 150 条、每块 5 条 retry）：帧跑完之后满一屏或账本空；一次同步调用 ≤ 末块 ＋ 4 × 150 条", () => {
    const r = run(8, 5);
    console.log(
      `[秤3·丁] 同步：真高 ${r.syncPx.toFixed(0)}px · 出账 ${r.syncTaken} 条 ｜ 帧跑完（${r.frames} 帧）：` +
        `真高 ${r.settledPx.toFixed(0)}px · 账本余 ${r.settledPending} 条`,
    );
    // 反空真：假布局真的被读到了（同步那一下至少末块的 5 张卡在）
    expect(r.syncPx).toBeGreaterThan(0);
    // 一次同步调用的量仍有界：末块直渲 150 ＋ 物化 ≤ 4 × 150
    expect(r.syncTaken).toBeLessThanOrEqual(MATERIALIZE_TAIL_K * (1 + MAX_ROUNDS));
    // 🔴 判据：满一屏，或者账本空了（真没有更多可补）
    expect(r.settledPx >= VIEW_H || r.settledPending === 0).toBe(true);
  });
});

/**
 * 「物化队列重建只收 `floorSeq === null` 的 tab ⇒ 钉过水位但还有 pending 的 tab
 * 只剩上翻一条路」。判据：切进一个「钉过水位、没满一屏、`scrollHeight` 被占位撑高」的 tab 时，按真实布局补到满一屏。
 * 同一个假布局台子（本文件「丁」段）：卡高按类 —— retry 23.05 px、assistant 1000 px（一张就满屏）。
 */
describe("秤 3 · 戊：切进钉过水位、没满一屏的 tab 按真实布局补满", () => {
  const VIEW_H = CLIENT_HEIGHT;
  const heightOf = (el: Element): number =>
    el.classList.contains("card-api-retry")
      ? TRUE_H_PX["card-api-retry"]
      : el.classList.contains("card-assistant")
        ? 1000
        : 0;
  const rect = (top: number, h: number): DOMRect =>
    ({ top, bottom: top + h, height: h, left: 0, right: 780, width: 780, x: 0, y: top, toJSON: () => ({}) }) as DOMRect;
  let restore: Array<() => void> = [];

  function installFakeLayout(): void {
    const proto = HTMLElement.prototype;
    const orig = proto.getBoundingClientRect;
    proto.getBoundingClientRect = function (this: HTMLElement): DOMRect {
      if (this.classList.contains("stream")) return rect(0, VIEW_H);
      const parent = this.parentElement;
      if (parent?.classList.contains("stream-content")) {
        let top = 0;
        for (const sib of Array.from(parent.children)) {
          if (sib === this) break;
          top += heightOf(sib);
        }
        return rect(top, heightOf(this));
      }
      return rect(0, 0);
    };
    restore.push(() => {
      proto.getBoundingClientRect = orig;
    });
  }

  afterEach(() => {
    for (const f of restore) f();
    restore = [];
    vi.unstubAllGlobals();
  });

  const sys = (sid: string, seq: number, retryCard: boolean): RigPayload =>
    withSession(
      rigLine(
        seq,
        retryCard
          ? { t: "retry", id: `${sid}-r${seq}`, at: "2026-09-10T00:00:00.000Z", reason: "network", attempt: 1, max: 5, draw: "card", human: false }
          : { t: "reply", id: `${sid}-k${seq}`, at: "2026-09-10T00:00:00.000Z", blocks: [], autoReply: true, draw: "none", human: false },
      ),
      sid,
    );
  // 后台 tab 只进账本（判据在 skeleton-view-cold-open.vitest.ts）。

  it("切进钉过水位、没满一屏、scrollHeight 被占位撑高的 tab ⇒ 按真实布局补满", () => {
    const rig = installViewerRig();
    installFakeLayout();
    vi.stubGlobal("requestIdleCallback", () => 0); // 空闲队列不跑：只看切进来那一下
    const barEl = document.createElement("div");
    const streamRootEl = document.createElement("div");
    document.body.append(barEl, streamRootEl);
    const tm = owned(announced(new TabManager(barEl, streamRootEl)));
    const tabOf = (sid: string): Tab =>
      (tm as unknown as { store: { tabs: Map<string, Tab> } }).store.tabs.get(sid)!;
    tm.onLine(sys("act", 5000, true) as never);
    tm.onLine(sys("b", 5000, true) as never);
    tm.onBatchStart();
    for (let s = 0; s < 1200; s++) tm.onLine(sys("b", s, s % 30 === 0) as never);
    tm.onBatchEnd();
    const b = tabOf("b");
    // 占位把 scrollHeight 撑成「滚得动」（真浏览器里没建卡的那几行贡献的是骨架占位高）
    Object.defineProperty(b.streamEl, "scrollHeight", { configurable: true, get: () => 5000 });
    Object.defineProperty(b.streamEl, "clientHeight", { configurable: true, get: () => VIEW_H });
    tm.switchTo("b");
    for (let i = 0; i < 50; i++) rig.flushRaf(1);
    const px = Array.from(b.stream.contentElement.children).reduce((s, c) => s + heightOf(c), 0);
    console.log(`[秤3·戊] 切进 b 之后：真高 ${px.toFixed(0)}px · 账本余 ${b.window.pendingCount}`);
    expect(px >= VIEW_H || b.window.pendingCount === 0).toBe(true);
  });
});
