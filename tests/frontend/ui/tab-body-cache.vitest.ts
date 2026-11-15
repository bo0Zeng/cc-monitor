/**
 * 〔骨架〕往上翻不空屏 · 建过的卡有上限 —— 真 `TabStreamView` ＋ 真 `MessageStream` · `RecordTimeline`，自带一维布局（卡一律 60px）。
 *
 * - 取多远与建多少分开：建卡只建视口 ±`OVERSCAN`，正文提前取到视口 ±`PREFETCH_SCREENS` 屏（只进正文缓存、不建卡）
 *   ⇒ 一屏一屏往上翻，翻到的那一屏正文已在手上，视口里不出「取回中」的占位。
 * - 交回占位的卡，正文留在按字节封顶的缓存里 ⇒ 翻回来不再按偏移取。
 * - 没接骨架的 tab：建过的卡超上限 ⇒ 离视口远的那一截交回尾部窗口（往上翻照补批补回来）。
 */
import { describe, expect, it, onTestFinished, vi } from "vitest";
import type { JsonlLinePayload } from "../../../src/frontend/ui/events";
import { SkeletonView } from "../../../src/frontend/ui/skeleton-view";
import { UNTIL_CEILING } from "../../test-support/wall-ceiling";
import { VIEW_H } from "../../test-support/skeleton-view-rig";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn(), openUrl: vi.fn() }));

const CARD_H = 60;
const reply = (seq: number) =>
  ({ agent: "claude", t: "reply", id: `u${seq}`, blocks: [{ type: "text", text: "x" }], autoReply: false, draw: "card", human: false }) as never;
const facts = (i: number) => ({ o: i * 100, n: 100, t: "reply", dr: "card", ch: 200, pl: 2 });

/**
 * 一维布局：`.stream-content` 的子元素自上而下堆，占位照内联高、卡一律 60px；scrollTop 随内容变矮夹回去（同真引擎）。
 * 各子元素的顶算一遍存着，DOM 一变（`MutationObserver.takeRecords` 同步取）就重算 —— 两三千张卡逐个现算是平方级。
 */
function layoutOf(scrollEl: HTMLElement, content: HTMLElement): { scrollTop: number } {
  const state = { scrollTop: 0 };
  const h = (el: Element): number => {
    const e = el as HTMLElement;
    if (e.classList.contains("proc-hidden") || e.classList.contains("proc-line") || e.classList.contains("stream-more-above")) return 0;
    if (e.style.height) return parseFloat(e.style.height);
    return CARD_H;
  };
  const mo = new MutationObserver(() => {});
  mo.observe(content, { childList: true, subtree: true, attributes: true, attributeFilter: ["style", "class"] });
  let offsets: Map<Element, number> | null = null;
  let sum = 0;
  const laid = (): Map<Element, number> => {
    if (mo.takeRecords().length > 0) offsets = null;
    if (offsets) return offsets;
    offsets = new Map();
    sum = 0;
    for (const c of Array.from(content.children)) {
      offsets.set(c, sum);
      sum += h(c);
    }
    return offsets;
  };
  const total = (): number => (laid(), sum);
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
    const at = laid().get(this);
    return at === undefined ? rect(0, 0) : rect(at - top(), h(this));
  };
  onTestFinished(() => {
    HTMLElement.prototype.getBoundingClientRect = orig;
    mo.disconnect();
  });
  return state;
}

/** `n` 行的会话：重放推来的全收在尾部窗口里（都见过）；`noIndex` ⇒ 那台答不了骨架索引（没骨架的 tab）。 */
async function rig(n: number, noIndex = false, cap?: number) {
  const { invoke } = await import("@tauri-apps/api/core");
  const { isChanCall, chanArgsJson, chanReply, withSessionReads, UNSUPPORTED } = await import("../../test-support/chan-fake");
  const { packIndex } = await import("../../test-support/pack-index");
  const { TabStreamView } = await import("../../../src/frontend/ui/tab-stream-view");
  const { TabStore } = await import("../../../src/frontend/ui/tab-store");
  vi.stubGlobal("CSS", { escape: (s: string) => s });
  onTestFinished(() => void vi.unstubAllGlobals());
  const rows = Array.from({ length: n }, (_, i) => facts(i));
  const payload = (s: number) => ({ session_id: "c", cwd: "/p", path: "/p/c.jsonl", seq: s, record: reply(s) });
  /** 按偏移取正文问过的几段（seq 半开）。 */
  const ranges: Array<[number, number]> = [];
  const others = withSessionReads((which, a) => {
    if (which !== "read_session_range") return undefined;
    const lo = Number(a.offset) / 100;
    const hi = Number(a.until) / 100;
    ranges.push([lo, hi]);
    return Array.from({ length: hi - lo }, (_, k) => payload(lo + k));
  });
  vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
    if (isChanCall(cmd, args, "history-index")) {
      if (noIndex) throw UNSUPPORTED;
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
  const host = {
    onLine(p: JsonlLinePayload) {
      if (tsv.admit(tab as never, p)) tsv.ingest(tab as never, p);
    },
    userActive() {},
    forkFrom() {},
    runLabelOf: () => undefined,
    markNewRunCard() {},
  };
  const tsv = new TabStreamView(store, root, host, undefined, cap);
  onTestFinished(() => tsv.dispose());
  const dom = tsv.mountTabDom("c");
  // 轮次：一轮都没有（不折）
  (dom.turnFold as unknown as { read: unknown }).read = vi.fn(async () => ({ available: true, from: 0, end: n * 100, turns: [] }));
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
    unreadSaid: null,
    fullRecord: async () => null,
    facts: { reset() {}, markStale() {}, refresh: async () => {}, needsFetch: false },
  };
  store.tabs.set("c", tab as never);
  tsv.wireTab(tab as never);
  const layout = layoutOf(dom.streamEl, dom.stream.contentElement);
  for (let s = 0; s < n; s++) {
    tab.seenSeqs.add(s);
    tab.window.defer(payload(s) as never);
  }
  const content = dom.stream.contentElement;
  const built = (): number[] =>
    Array.from(content.children)
      .map((c) => c.getAttribute("data-id"))
      .filter((id): id is string => id !== null && /^u\d+$/.test(id))
      .map((id) => Number(id.slice(1)));
  /** 视口里「取回中」的占位（正文还在路上：这一刻视口里是空的）。 */
  const fetchingInView = (): string[] =>
    Array.from(content.querySelectorAll<HTMLElement>("[data-skeleton-fetching]"))
      .filter((g) => {
        const r = g.getBoundingClientRect();
        return r.height > 0 && r.bottom > 0 && r.top < VIEW_H;
      })
      .map((g) => `[${g.dataset.skeletonLo},${g.dataset.skeletonHi})`);
  const open = (): void => {
    store.activeId = "c";
    tsv.showOnly("c");
    tsv.activate(tab as never);
    tsv.scrollToBottom(tab as never);
  };
  /** 滚到 `y`：宿主的滚动监听同步跑完（补可见区 · 交回远处），返回这一刻视口里取回中的占位。 */
  const scrollTo = (y: number): string[] => {
    layout.scrollTop = y;
    dom.streamEl.dispatchEvent(new Event("scroll"));
    return fetchingInView();
  };
  return { tab, dom, layout, built, open, scrollTo, ranges, content, tsv };
}

const settle = async (): Promise<void> => {
  for (let k = 0; k < 3; k++) await new Promise((res) => setTimeout(res, 0));
};

describe("〔A18-4-1-113 ④〕往上翻：正文提前取、交回的卡正文留着", () => {
  async function attached(n: number) {
    const r = await rig(n);
    r.open();
    await vi.waitFor(() => expect(r.tab.skeleton, "骨架没接上").not.toBeNull(), UNTIL_CEILING);
    await settle();
    expect(r.tab.window.pendingCount, "前置：接上骨架之后尾部窗口只留尾巴，更早的正文不在手上").toBeLessThan(n);
    return r;
  }

  it("一屏一屏往上翻（每屏之间正文来得及落地）：翻到的那一屏视口里没有取回中的占位", async () => {
    const r = await attached(1200);
    // 跳到中段（那里的正文本来就不在手上：这一下取回中是应当的），等它落地
    r.scrollTo(r.layout.scrollTop / 2);
    await settle();
    expect(r.ranges.length, "前置：中段那一截是按偏移取回的").toBeGreaterThan(0);
    for (let k = 0; k < 8 && r.layout.scrollTop > 0; k++) {
      const empty = r.scrollTo(r.layout.scrollTop - VIEW_H);
      expect(empty, `往上翻第 ${k + 1} 屏：视口里还在取`).toEqual([]);
      await settle();
    }
  });

  it("往上翻过几屏再翻回来：交回占位的那几屏正文从缓存拿，不再按偏移取", async () => {
    const r = await attached(1200);
    r.scrollTo(r.layout.scrollTop / 2);
    await settle();
    const start = r.layout.scrollTop;
    const seen = new Set(r.built());
    for (let k = 0; k < 6; k++) {
      r.scrollTo(r.layout.scrollTop - VIEW_H);
      await settle();
      for (const s of r.built()) seen.add(s);
    }
    const kept = new Set(r.built());
    const gaveBack = [...seen].filter((s) => !kept.has(s));
    expect(gaveBack.length, "前置：翻过的远处已交回占位").toBeGreaterThan(2 * Math.ceil(VIEW_H / CARD_H));
    const asked = r.ranges.length;
    while (r.layout.scrollTop < start - 1) {
      const empty = r.scrollTo(Math.min(start, r.layout.scrollTop + VIEW_H));
      expect(empty, "翻回来视口里在取").toEqual([]);
      await settle();
    }
    // 翻回来时视口下方更远处（从没建过的）照预取取，交回过的那几行一行都不再取
    const again = r.ranges.slice(asked).filter(([a, b]) => gaveBack.some((s) => s >= a && s < b));
    expect(again, "翻回来又按偏移取了交回过的那几行").toEqual([]);
  });

  it("关掉 tab：正文缓存跟着清", async () => {
    const r = await attached(600);
    r.scrollTo(0);
    await settle();
    const cache = (r.tsv as unknown as { bodies: WeakMap<object, { size: number }> }).bodies.get(r.tab);
    expect(cache, "tab 上没有正文缓存").toBeDefined();
    expect(cache!.size, "前置：缓存里有东西").toBeGreaterThan(0);
    r.tsv.disposeTab(r.tab as never);
    expect(cache!.size, "关掉之后缓存还攥着正文").toBe(0);
  });

  it("取的带比留卡的带宽（取到的地方先于建卡的地方）", () => {
    expect(SkeletonView.PREFETCH_SCREENS).toBeGreaterThan(SkeletonView.KEEP_SCREENS);
  });
});

describe("〔A18-4-1-113 ⑥〕没接骨架的 tab：建过的卡有上限", () => {
  /** 上限在这里取小一点（jsdom 里几千张卡的 DOM 慢）；它照样远大于视口 ±KEEP_SCREENS 屏放得下的卡数。 */
  const CAP = 300;
  it("往上一路翻到顶、再一屏一屏翻回底：已建的卡不超上限，翻回顶时照补批补回来", async () => {
    const n = CAP + 600;
    const r = await rig(n, true, CAP);
    r.open();
    await vi.waitFor(() => expect(r.tab.skeletonFetch).toBe("done"), UNTIL_CEILING);
    expect(r.tab.skeleton, "前置：没接骨架").toBeNull();
    // 往上：贴顶一下补一批（上翻补批），直到全建出来
    for (let k = 0; k < 200 && r.tab.window.pendingCount > 0; k++) {
      r.scrollTo(0);
      await new Promise((res) => requestAnimationFrame(() => res(null)));
    }
    expect(r.tab.window.pendingCount, "前置：翻到顶全建出来了").toBe(0);
    // 往下一屏一屏翻回底
    const bottom = (): number => r.dom.streamEl.scrollHeight - VIEW_H;
    for (let k = 0; k < 2 * n && r.layout.scrollTop < bottom() - 1; k++) r.scrollTo(r.layout.scrollTop + VIEW_H);
    expect(r.built().length, `翻回底还建着 ${r.built().length} 张`).toBeLessThanOrEqual(CAP);
    expect(Math.max(...r.built()), "尾巴还在").toBe(n - 1);
    // 再往上翻：交回窗口的那几行照补批补回来，一行不缺
    for (let k = 0; k < 400 && r.tab.window.pendingCount > 0; k++) {
      r.scrollTo(0);
      await new Promise((res) => requestAnimationFrame(() => res(null)));
    }
    expect(Math.min(...r.built()), "翻回顶缺了最老那几行").toBe(0);
  });
});
