/**
 * 〔骨架〕冷开只建一屏 —— 真 `TabStreamView` ＋ 真 `MessageStream` · `RecordTimeline` · `TurnFold`，自带一维布局（卡一律 60px）。
 * 冷开 · 接骨架 · 轮次落地 · 切走切回这几步各自只建视口那一截，后台 tab 一张卡都不建。
 */
import { describe, expect, it, onTestFinished, vi } from "vitest";
import type { JsonlLinePayload } from "../../../src/frontend/ui/events";
import { SkeletonView } from "../../../src/frontend/ui/skeleton-view";
import { UNTIL_CEILING } from "../../test-support/wall-ceiling";
import { VIEW_H } from "../../test-support/skeleton-view-rig";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn(), openUrl: vi.fn() }));

// ===== 冷开只建视口那一截，折着的过程不建卡 =====
// 冷开一个没建过卡的 tab：尾部窗口里是重放推来的 600 行（壳留存每会话的尾巴），之前一轮 150 条、四轮全建（489 个条目，
// 205 个在折着的过程里 0 高）—— WebKit 冷切停住那一帧 1.6 s 跟着它走。改成：只建填满一屏的那一截，接上骨架、轮次落地之后
// 照账本补可见区（折着的扣掉）。真 `TabStreamView` ＋ 真 `MessageStream` · `RecordTimeline` · `TurnFold`；几何同上面的一维模型，
// 只是卡没有 `data-h`（真渲染出来的）⇒ 一律 60px，`proc-hidden` 的 0、过程行 0；IPC 只有骨架索引与轮次两问是替身。
describe("〔A17-45-3-2〕冷开只建一屏", () => {
  const N = 600;
  const PER = 20;
  const CARD_H = 60;
  const reply = (seq: number) =>
    ({ agent: "claude", t: "reply", id: `u${seq}`, blocks: [{ type: "text", text: "x" }], autoReply: false, draw: "card", human: false }) as never;
  const facts = (i: number) => ({ o: i * 100, n: 100, t: "reply", dr: "card", ch: 200, pl: 2 });
  /** 每轮：开头那一行露着、结尾（最后一行）露着，中间是过程（折着）。 */
  const turn = (k: number) => ({
    at: k * PER * 100,
    line: k * PER,
    uuid: `u${k * PER}`,
    start: "2026-10-06T02:01:00Z",
    end: "2026-10-06T02:04:02Z",
    startText: "02:01",
    endText: "02:04",
    said: "q",
    tools: 3,
    thinking: 0,
    agents: 0,
    background: 0,
    retries: 0,
    peers: 0,
    fails: 0,
    shown: [`u${k * PER + PER - 1}`],
    reply: "",
    done: true,
    phase: "idle",
    folds: [{ after: `u${k * PER}`, afterLine: k * PER, until: `u${k * PER + PER - 1}`, untilLine: k * PER + PER - 1, parts: [{ text: "p", tone: "plain" }], span: { text: "", from: null, to: null }, phase: "idle", firstFail: null }],
  });

  /** 一维布局（同上），外加 `scrollHeight` / `clientHeight`：流的贴底读它们。 */
  function layoutOf(scrollEl: HTMLElement, content: HTMLElement): { scrollTop: number } {
    const state = { scrollTop: 0 };
    const h = (el: Element): number => {
      const e = el as HTMLElement;
      if (e.classList.contains("proc-hidden") || e.classList.contains("proc-line") || e.classList.contains("stream-more-above")) return 0;
      if (e.style.height) return parseFloat(e.style.height);
      return CARD_H;
    };
    const total = (): number => Array.from(content.children).reduce((a, c) => a + h(c), 0);
    // 内容变矮了 scrollTop 跟着夹回去（同真引擎）
    const top = (): number => (state.scrollTop = Math.max(0, Math.min(state.scrollTop, total() - VIEW_H)));
    Object.defineProperty(scrollEl, "scrollTop", { get: top, set: (v: number) => ((state.scrollTop = v), top()), configurable: true });
    Object.defineProperty(scrollEl, "scrollHeight", { get: () => Math.max(VIEW_H, total()), configurable: true });
    Object.defineProperty(scrollEl, "clientHeight", { get: () => VIEW_H, configurable: true });
    const rect = (y: number, hh: number): DOMRect => ({ top: y, bottom: y + hh, height: hh, left: 0, right: 100, width: 100, x: 0, y }) as DOMRect;
    scrollEl.getBoundingClientRect = () => rect(0, VIEW_H);
    content.getBoundingClientRect = () => rect(-top(), total());
    const orig = HTMLElement.prototype.getBoundingClientRect;
    HTMLElement.prototype.getBoundingClientRect = function (this: HTMLElement): DOMRect {
      if (this.parentElement !== content) return orig.call(this);
      if (h(this) === 0 && !this.style.height) return rect(0, 0);
      let y = -top();
      for (const c of Array.from(content.children)) {
        const hh = h(c);
        if (c === this) return rect(y, hh);
        y += hh;
      }
      return rect(0, 0);
    };
    onTestFinished(() => {
      HTMLElement.prototype.getBoundingClientRect = orig;
    });
    return state;
  }

  /** `late` ⇒ 轮次那一问等测试放行；`slowIndex` ⇒ 骨架索引那一问等测试放行（整份读那一问比轮次慢那一形）。 */
  async function rig(late = false, slowIndex = false) {
    let letIndex!: () => void;
    const indexGate = slowIndex ? new Promise<void>((res) => (letIndex = res)) : Promise.resolve();
    const { invoke } = await import("@tauri-apps/api/core");
    const { isChanCall, chanArgsJson, chanReply, withSessionReads, UNSUPPORTED } = await import("../../test-support/chan-fake");
    let indexFails = false;
    const { packIndex } = await import("../../test-support/pack-index");
    const { TabStreamView } = await import("../../../src/frontend/ui/tab-stream-view");
    const { TabStore } = await import("../../../src/frontend/ui/tab-store");
    vi.stubGlobal("CSS", { escape: (s: string) => s });
    onTestFinished(() => void vi.unstubAllGlobals());
    const rows = Array.from({ length: N }, (_, i) => facts(i));
    // 骨架索引照后端：从 `offset` 起、至多 `limit` 行；按偏移取正文：第 i 行占字节 [100i, 100i + 100)
    const payload = (s: number) => ({ session_id: "c", cwd: "/p", path: "/p/c.jsonl", seq: s, record: reply(s) });
    const others = withSessionReads((which, a) =>
      which === "read_session_range"
        ? Array.from({ length: (Number(a.until) - Number(a.offset)) / 100 }, (_, k) => payload(Number(a.offset) / 100 + k))
        : which === "read_session_lines"
          ? (() => {
              // 按行号往后读：一页至多 250 行，读到末尾为止
              const from = Number(a.from);
              const next = Math.min(rows.length, a.until === undefined ? rows.length : Number(a.until), from + 250);
              return { from, next, eof: next >= rows.length, payloads: Array.from({ length: next - from }, (_, k) => payload(from + k)) };
            })()
          : undefined,
    );
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      if (isChanCall(cmd, args, "history-index")) {
        await indexGate;
        if (indexFails) throw UNSUPPORTED; // 那台后端比这一问老
        const b = chanArgsJson(args) as { offset: number; limit?: number };
        const lo = b.offset / 100;
        const hi = Math.min(rows.length, b.limit === undefined ? rows.length : lo + b.limit);
        return chanReply({ ...packIndex(lo * 100, hi * 100, rows.slice(lo, hi)), next: null });
      }
      return (await others(cmd, args)) ?? null;
    });
    onTestFinished(() => void vi.mocked(invoke).mockReset().mockResolvedValue(null));
    const root = document.createElement("div");
    document.body.replaceChildren(root);
    const store = new TabStore();
    // 按行号取回的行走宿主的 `onLine` 全套（这里就是去重 ＋ 门控）
    const host = {
      onLine(p: JsonlLinePayload) {
        if (tsv.admit(tab as never, p)) tsv.ingest(tab as never, p);
      },
      refreshTabBar() {},
      scheduleTabBarRefresh() {},
      userActive() {},
      forkFrom() {},
      runLabelOf: () => undefined,
      markNewRunCard() {},
    };
    const tsv = new TabStreamView(store, root, host);
    onTestFinished(() => tsv.dispose());
    const dom = tsv.mountTabDom("c");
    // 轮次那一问：`late` ⇒ 等测试放行（骨架先接上、轮次后到那一形）
    let letTurns!: () => void;
    const gate = late ? new Promise<void>((res) => (letTurns = res)) : Promise.resolve();
    const turns = vi.fn(async () => {
      await gate;
      return { available: true, from: 0, end: N * 100, turns: Array.from({ length: N / PER }, (_, k) => turn(k)) };
    });
    (dom.turnFold as unknown as { read: unknown }).read = turns;
    const tab = {
      sessionId: "c",
      ...dom,
      parentPath: "/p/c.jsonl",
      origin: "<local>",
      agent: null,
      face: null,
      needs: null,
      pending: [],
      retries: new Map(),
      toolUseNames: new Map(),
      runCards: new Map(),
      unread: 0,
      fullRecord: async () => null,
      facts: { reset() {}, markStale() {}, refresh: async () => {}, needsFetch: false },
    };
    store.tabs.set("c", tab as never);
    tsv.wireTab(tab as never); // 滚动 · 视口变大的补批（同 `TabManager` 建 tab 那一下）
    const layout = layoutOf(dom.streamEl, dom.stream.contentElement);
    // 重放推来的尾巴：全收纳（后台 tab，还没建过卡）
    for (let s = 0; s < N; s++) {
      tab.seenSeqs.add(s);
      tab.window.defer({ session_id: "c", cwd: "/p", path: "/p/c.jsonl", seq: s, record: reply(s) } as never);
    }
    const built = (): number[] =>
      Array.from(dom.stream.contentElement.children)
        .map((c) => c.getAttribute("data-id"))
        .filter((id): id is string => id !== null && /^u\d+$/.test(id))
        .map((id) => Number(id.slice(1)));
    const open = (): void => {
      store.activeId = "c";
      tsv.showOnly("c");
      tsv.activate(tab as never);
      tsv.scrollToBottom(tab as never); // 宿主切进来照「贴着底」贴底（`tabs.ts::switchTo`）
    };
    /** 实时来一行（记录文件同时长了一行：索引里也有它）。 */
    const live = (s: number): void => {
      rows.push(facts(s));
      const p = payload(s) as never;
      if (tsv.admit(tab as never, p)) tsv.ingest(tab as never, p);
    };
    const failIndex = (): void => {
      indexFails = true;
      letIndex?.();
    };
    return { tab, dom, layout, built, open, turns, letTurns: () => letTurns?.(), letIndex: () => letIndex?.(), failIndex, store, tsv, live };
  }

  /** 视口 ±`KEEP_SCREENS` 屏放得下几张卡。 */
  const keepCards = (1 + 2 * SkeletonView.KEEP_SCREENS) * Math.ceil(VIEW_H / CARD_H);

  it("冷开返回时：已建的只有填满一屏那一截（≤ 视口 ±KEEP_SCREENS 屏），贴底，最后一张卡顶到视口下沿", async () => {
    const r = await rig();
    r.open();
    const ids = r.built();
    expect(ids.length, `冷开建了 ${ids.length} 张卡`).toBeGreaterThan(0);
    expect(ids.length, `冷开建了 ${ids.length} 张卡（视口 ±${SkeletonView.KEEP_SCREENS} 屏放得下 ${keepCards} 张）`).toBeLessThanOrEqual(keepCards);
    expect(Math.max(...ids)).toBe(N - 1);
    const last = r.dom.stream.contentElement.querySelector<HTMLElement>(`[data-id="u${N - 1}"]`)!;
    expect(last.getBoundingClientRect().bottom, "没贴底").toBeCloseTo(VIEW_H, 0);
  });

  it.each([
    ["轮次先到", false],
    ["骨架先接上、轮次后到", true],
  ])("接上骨架、轮次落地之后（%s）：补可见区不建折着的过程，已建的都在视口 ±KEEP_SCREENS 屏里，视口里不空", async (_name, late) => {
    const r = await rig(late);
    r.open();
    const atOpen = new Set(r.built());
    await vi.waitFor(() => expect(r.tab.skeleton, "骨架没接上").not.toBeNull(), UNTIL_CEILING);
    r.letTurns();
    await vi.waitFor(() => expect(r.turns).toHaveBeenCalled(), UNTIL_CEILING);
    await new Promise((res) => setTimeout(res, 0));
    await new Promise((res) => setTimeout(res, 0));
    const sk = r.tab.skeleton!;
    const folded = sk.ledger.foldedRanges;
    expect(folded.length, "前置：账本拿到了折叠").toBeGreaterThan(0);
    const ids = r.built();
    const inFold = ids.filter((s) => !atOpen.has(s) && folded.some(([a, b]) => s >= a && s < b));
    expect(inFold, "骨架补可见区建了折着的过程").toEqual([]);
    expect(ids.length, `已建 ${ids.length} 张`).toBeLessThanOrEqual(keepCards);
    for (const s of ids) {
      const el = r.dom.stream.contentElement.querySelector<HTMLElement>(`[data-id="u${s}"]`)!;
      if (el.classList.contains("proc-hidden")) continue;
      const box = el.getBoundingClientRect();
      expect(box.bottom, `第 ${s} 行离视口超过 ${SkeletonView.KEEP_SCREENS} 屏`).toBeGreaterThan(-VIEW_H * SkeletonView.KEEP_SCREENS);
      expect(box.top).toBeLessThan(VIEW_H * (1 + SkeletonView.KEEP_SCREENS));
    }
    // 视口里有多高是卡（占位不算）：过程藏掉之后露出来的占位要补成卡，不许留着一块空的
    const covered = ids.reduce((a, s) => {
      const b = r.dom.stream.contentElement.querySelector<HTMLElement>(`[data-id="u${s}"]`)!.getBoundingClientRect();
      return a + Math.max(0, Math.min(b.bottom, VIEW_H) - Math.max(b.top, 0));
    }, 0);
    expect(covered, "过程藏掉之后视口里露出的占位没补").toBeGreaterThanOrEqual(VIEW_H - CARD_H);
  });

  // 真机里那一形（台架冷切长会话照样建满 489 个条目）：轮次先落地、过程藏成 0 高，屏不满、贴底那一下的 scroll 落进上翻触发区，
  // 骨架索引（整份读）还在路上 ⇒ 尾部窗口一批 200 条往上补、rAF 自链补到弹尽，全是看不见的过程卡。
  it("轮次先落地、骨架索引还在路上：滚动 · 下一帧都不从尾部窗口往上补；索引到了照账本补满视口", async () => {
    const r = await rig(false, true);
    r.open();
    await vi.waitFor(() => expect(r.turns).toHaveBeenCalled(), UNTIL_CEILING);
    await new Promise((res) => setTimeout(res, 0));
    await new Promise((res) => setTimeout(res, 0));
    for (let k = 0; k < 4; k++) {
      r.dom.streamEl.dispatchEvent(new Event("scroll"));
      await new Promise((res) => requestAnimationFrame(() => res(null)));
    }
    expect(r.tab.skeleton, "前置：骨架还没接上").toBeNull();
    expect(r.built().length, `骨架还没来就从尾部窗口建了 ${r.built().length} 张`).toBeLessThanOrEqual(keepCards);
    r.letIndex();
    await vi.waitFor(() => expect(r.tab.skeleton, "骨架没接上").not.toBeNull(), UNTIL_CEILING);
    await new Promise((res) => setTimeout(res, 0));
    const ids = r.built();
    expect(ids.length, `已建 ${ids.length} 张`).toBeLessThanOrEqual(keepCards);
    const covered = ids.reduce((a, s) => {
      const b = r.dom.stream.contentElement.querySelector<HTMLElement>(`[data-id="u${s}"]`)!.getBoundingClientRect();
      return a + Math.max(0, Math.min(b.bottom, VIEW_H) - Math.max(b.top, 0));
    }, 0);
    expect(covered, "接上骨架之后视口里没补满").toBeGreaterThanOrEqual(VIEW_H - CARD_H);
  });

  // 切进一个没在看的会话：那台从上流点起交，之下那一截整份重来、按行号从老到新取回（`catchUpTo`）。
  // 不按「批里首条钉 floor、之后直渲」把取回的 600 行整段建出来（台架冷切长会话 489 个条目走的就是这一条路）。
  it("切进来补上流前那一截（按行号从老到新取回 600 行）：取回的只进账本，取完只建视口那一截", async () => {
    const r = await rig();
    // 这一形手上一行都没有（台子默认收着重放推来的尾段）
    r.tab.seenSeqs.clear();
    r.tab.window.dropPending();
    r.store.activeId = "c";
    r.tsv.showOnly("c");
    r.tsv.catchUpTo(r.tab as never, "/p/c.jsonl", N);
    await vi.waitFor(() => expect(r.built().length, "取完了没建卡").toBeGreaterThan(0), UNTIL_CEILING);
    const ids = r.built();
    expect(ids.length, `取回 ${N} 行之后建了 ${ids.length} 张`).toBeLessThanOrEqual(keepCards);
    expect(Math.max(...ids)).toBe(N - 1);
  });

  it("轮次先落地、骨架等不来（老后端答不了索引）⇒ 接着从尾部窗口补，视口不空着", async () => {
    const r = await rig(false, true);
    r.open();
    await vi.waitFor(() => expect(r.turns).toHaveBeenCalled(), UNTIL_CEILING);
    await new Promise((res) => setTimeout(res, 0));
    await new Promise((res) => setTimeout(res, 0));
    const before = r.tab.window.pendingCount;
    r.failIndex();
    await vi.waitFor(() => expect(r.tab.skeletonFetch).toBe("done"), UNTIL_CEILING);
    await vi.waitFor(() => expect(r.tab.window.pendingCount, "骨架来不了也不接着补").toBeLessThan(before), UNTIL_CEILING);
    expect(r.tab.skeleton).toBeNull();
  });

  // 收起的 tab 只留视口那一截：冷开建的那一截尾巴与接骨架之后建的都要交得回去
  //（骨架索引只要 `[0, floor)` 的话，冷开建的尾巴落在账本外，`release` 认不到它的切点 ⇒ 一条都不交）。
  /** 一屏放得下几张卡，再加切点两头各一张。 */
  const screenCards = Math.ceil(VIEW_H / CARD_H) + 2;
  const settled = async (r: Awaited<ReturnType<typeof rig>>): Promise<void> => {
    await vi.waitFor(() => expect(r.tab.skeleton, "骨架没接上").not.toBeNull(), UNTIL_CEILING);
    await vi.waitFor(() => expect(r.turns).toHaveBeenCalled(), UNTIL_CEILING);
    await new Promise((res) => setTimeout(res, 0));
    await new Promise((res) => setTimeout(res, 0));
  };
  const hide = (r: Awaited<ReturnType<typeof rig>>): void => {
    r.store.activeId = "other";
    r.tsv.showOnly("other");
  };

  it("〔A17-45-3-3〕冷开、接上骨架之后切走：留下的已建条目 ≤ 一屏", async () => {
    const r = await rig();
    r.open();
    await settled(r);
    hide(r);
    const ids = r.built();
    expect(ids.length, `切走之后还留着 ${ids.length} 张：${ids.join(",")}`).toBeLessThanOrEqual(screenCards);
  });

  it("〔A17-45-3-3〕接上骨架之后又来了几屏实时行：切走 · 切回 · 再切走 ⇒ 留下的 ≤ 一屏（账本续到了末尾）", async () => {
    const r = await rig();
    r.open();
    await settled(r);
    for (let s = N; s < N + 40; s++) r.live(s);
    expect(r.built().filter((s) => s >= N).length, "前置：实时行建了卡").toBe(40);
    hide(r);
    await vi.waitFor(() => expect(r.tab.skeleton!.ledger.endSeq, "收起时没把账本续到末尾").toBe(N + 40), UNTIL_CEILING);
    r.store.activeId = "c";
    r.tsv.showOnly("c");
    r.tsv.activate(r.tab as never);
    await new Promise((res) => requestAnimationFrame(() => res(null)));
    hide(r);
    const kept = r.built();
    expect(kept.length, `切走之后还留着 ${kept.length} 张：${kept.join(",")}`).toBeLessThanOrEqual(screenCards);
  });

  it("〔A17-45-3-3〕往上翻过几屏再切走：留下的 ≤ 一屏；切回来建的 ≤ 一屏、视口不空", async () => {
    const r = await rig();
    r.open();
    await settled(r);
    for (let k = 0; k < 6; k++) {
      r.layout.scrollTop = Math.max(0, r.layout.scrollTop - VIEW_H);
      r.dom.streamEl.dispatchEvent(new Event("scroll"));
      await new Promise((res) => setTimeout(res, 0));
    }
    await vi.waitFor(() => expect(r.dom.stream.contentElement.querySelector("[data-skeleton-fetching]"), "取回中的还没落").toBeNull(), UNTIL_CEILING);
    expect(r.built().length, "前置：翻过之后建了卡").toBeGreaterThan(0);
    hide(r);
    const kept = r.built();
    expect(kept.length, `切走之后还留着 ${kept.length} 张：${kept.join(",")}`).toBeLessThanOrEqual(screenCards);
    // 切回来：照「切进来」那一路（非 virgin ⇒ 下一帧补可见区）
    r.store.activeId = "c";
    r.tsv.showOnly("c");
    r.tsv.activate(r.tab as never);
    await new Promise((res) => requestAnimationFrame(() => res(null)));
    await vi.waitFor(() => expect(r.dom.stream.contentElement.querySelector("[data-skeleton-fetching]")).toBeNull(), UNTIL_CEILING);
    const back = r.built();
    // 补可见区多建视口上下各半屏（`OVERSCAN`）⇒ 至多两屏
    expect(back.length - kept.length, "切回来建了不止视口 ±半屏").toBeLessThanOrEqual(2 * Math.ceil(VIEW_H / CARD_H) + 2);
    const covered = back.reduce((a, s) => {
      const b = r.dom.stream.contentElement.querySelector<HTMLElement>(`[data-id="u${s}"]`)!.getBoundingClientRect();
      return a + Math.max(0, Math.min(b.bottom, VIEW_H) - Math.max(b.top, 0));
    }, 0);
    expect(covered, "切回来视口里空着").toBeGreaterThanOrEqual(VIEW_H - CARD_H);
  });

  // 后台 tab 只进账本：冷切只建一屏之后不值得提前建，看不见的 tab 一张卡都不建（批结束后空闲时也不一截 20 条地补建）。
  it("〔A17-45-3-4〕重放批结束、过了好一阵：后台 tab（能开终端的活会话、还没建过卡）一张卡都不建", async () => {
    const { goldenFace } = await import("../../test-support/session-faces");
    const { hasTerminal } = await import("../../../src/frontend/ui/face-can");
    const r = await rig();
    (r.tab as { face: unknown }).face = goldenFace("live-idle");
    expect(hasTerminal(r.tab.face as never), "前置：能开终端（活会话那一类）").toBe(true);
    r.store.activeId = "other";
    vi.useFakeTimers();
    onTestFinished(() => void vi.useRealTimers());
    r.tsv.batchEnd();
    await vi.advanceTimersByTimeAsync(10_000);
    expect(r.built(), "后台 tab 在空闲时建了卡").toEqual([]);
    expect(r.tab.window.pendingCount, "账本照留").toBe(N);
  });
});
