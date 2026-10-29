/**
 * 〔A17-45-3-6-2〕开页时当前那个 tab 的尾段也只进账本，只建视口那一截。
 *
 * 台架开页（`perf-tabs`，WebKitGTK 3/3 趟）：壳起好、当前 tab 建出来之后约 1 s 来一个批头，当前 tab 的尾段（第 4729…5328 行）
 * 从老到新逐行进来，约 300 ms grace 之后批尾。从前按「批里当前 tab 首条钉 floor、之后直渲」把这一段整段建成卡（528 次插卡，
 * 首卡到安静 5–11 s）。改成：批里来的行不分当前 / 后台只进账本，批收尾照冷切那一条（视口物化 ＋ 停住接骨架）只建视口那一截。
 *
 * 真 `TabStreamView` ＋ 真 `MessageStream` · `RecordTimeline` · `TurnFold`；几何是一维布局模型（jsdom 没有布局引擎）：
 * 卡一律 60px，`proc-hidden` · 过程行 0；IPC 只有骨架索引 · 轮次 · 按偏移取正文三问是替身。
 * 买不到：真引擎里的排版与像素（台架那一份手量，见验收记录）。
 */
import { describe, expect, it, onTestFinished, vi } from "vitest";
import type { JsonlLinePayload } from "../../../src/frontend/ui/events";
import { SkeletonView } from "../../../src/frontend/ui/skeleton-view";
import { UNTIL_CEILING } from "../../test-support/wall-ceiling";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn(), openUrl: vi.fn() }));

globalThis.ResizeObserver = class {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
} as unknown as typeof ResizeObserver;

const VIEW_H = 800;
const N = 600;
const PER = 20;
const CARD_H = 60;
/** 视口 ±`KEEP_SCREENS` 屏放得下几张卡。 */
const keepCards = (1 + 2 * SkeletonView.KEEP_SCREENS) * Math.ceil(VIEW_H / CARD_H);

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
  folds: [
    {
      after: `u${k * PER}`,
      afterLine: k * PER,
      until: `u${k * PER + PER - 1}`,
      untilLine: k * PER + PER - 1,
      parts: [{ text: "p", tone: "plain" }],
      span: { text: "", from: null, to: null },
      phase: "idle",
      firstFail: null,
    },
  ],
});

/** 一维布局：`.stream-content` 的子元素自上而下堆叠；`scrollHeight` / `clientHeight` 给流的贴底。 */
function layoutOf(scrollEl: HTMLElement, content: HTMLElement): void {
  const state = { scrollTop: 0 };
  const h = (el: Element): number => {
    const e = el as HTMLElement;
    if (e.classList.contains("proc-hidden") || e.classList.contains("proc-line") || e.classList.contains("stream-more-above")) return 0;
    if (e.style.height) return parseFloat(e.style.height);
    return CARD_H;
  };
  const total = (): number => Array.from(content.children).reduce((a, c) => a + h(c), 0);
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
}

async function rig() {
  const { invoke } = await import("@tauri-apps/api/core");
  const { isChanCall, chanArgsJson, chanReply, withSessionReads } = await import("../../test-support/chan-fake");
  const { packIndex } = await import("../../test-support/pack-index");
  const { TabStreamView } = await import("../../../src/frontend/ui/tab-stream-view");
  const { TabStore } = await import("../../../src/frontend/ui/tab-store");
  vi.stubGlobal("CSS", { escape: (s: string) => s });
  onTestFinished(() => void vi.unstubAllGlobals());
  const rows = Array.from({ length: N }, (_, i) => facts(i));
  const payload = (s: number): JsonlLinePayload => ({ session_id: "c", cwd: "/p", path: "/p/c.jsonl", seq: s, record: reply(s) }) as never;
  const others = withSessionReads((which, a) =>
    which === "read_session_range" ? Array.from({ length: (Number(a.until) - Number(a.offset)) / 100 }, (_, k) => payload(Number(a.offset) / 100 + k)) : undefined,
  );
  vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
    if (isChanCall(cmd, args, "history-index")) {
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
  const turns = vi.fn(async () => ({ available: true, from: 0, end: rows.length * 100, turns: Array.from({ length: N / PER }, (_, k) => turn(k)) }));
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
  tsv.wireTab(tab as never);
  layoutOf(dom.streamEl, dom.stream.contentElement);
  // 开页：壳起好、它就是当前那一个（宿主建 tab 时 `showOnly` ＋ `activate`；那一刻账本还空着）
  store.activeId = "c";
  tsv.showOnly("c");
  tsv.activate(tab as never);
  const built = (): number[] =>
    Array.from(dom.stream.contentElement.children)
      .map((c) => c.getAttribute("data-id"))
      .filter((id): id is string => id !== null && /^u\d+$/.test(id))
      .map((id) => Number(id.slice(1)));
  /** 台架开页那一段的帧序列：批头 → 尾段从老到新逐行 → 批尾（grace 到点，宿主 `onBatchEnd`）。 */
  const bootBatch = (): void => {
    store.inBatch = true;
    for (let s = 0; s < N; s++) host.onLine(payload(s));
    store.inBatch = false;
    tsv.batchEnd();
  };
  /** 实时来一行（记录文件同时长了一行：索引里也有它）。 */
  const live = (s: number): void => {
    rows.push(facts(s));
    host.onLine(payload(s));
  };
  return { tab, dom, built, bootBatch, live, turns };
}

type Rig = Awaited<ReturnType<typeof rig>>;

const settle = async (r: Rig): Promise<void> => {
  await vi.waitFor(() => expect(r.tab.skeleton, "骨架没接上").not.toBeNull(), UNTIL_CEILING);
  await vi.waitFor(() => expect(r.turns).toHaveBeenCalled(), UNTIL_CEILING);
  await new Promise((res) => setTimeout(res, 0));
  await new Promise((res) => setTimeout(res, 0));
};

const boxOf = (r: Rig, s: number): DOMRect => r.dom.stream.contentElement.querySelector<HTMLElement>(`[data-id="u${s}"]`)!.getBoundingClientRect();

describe("〔A17-45-3-6-2〕开页时当前 tab 的尾段只建视口那一截", () => {
  it("重放批里来的尾段：批收尾只建一屏那一截、贴底，余下的在账本里", async () => {
    const r = await rig();
    r.bootBatch();
    const ids = r.built();
    expect(ids.length, `开页批收尾建了 ${ids.length} 张卡（视口 ±${SkeletonView.KEEP_SCREENS} 屏放得下 ${keepCards} 张）`).toBeLessThanOrEqual(keepCards);
    expect(ids.length, "批收尾一张都没建（视口空着）").toBeGreaterThan(0);
    expect(Math.max(...ids)).toBe(N - 1);
    expect(r.tab.window.pendingCount, "余下的没进账本").toBe(N - ids.length);
    expect(boxOf(r, N - 1).bottom, "没贴底").toBeCloseTo(VIEW_H, 0);
  });

  it("流停下来、接上骨架之后：流里已建的条目只落在视口 ±KEEP_SCREENS 屏以内，视口里铺满、贴底", async () => {
    const r = await rig();
    r.bootBatch();
    await settle(r);
    const ids = r.built();
    expect(ids.length, `安静后已建 ${ids.length} 张`).toBeLessThanOrEqual(keepCards);
    for (const s of ids) {
      const el = r.dom.stream.contentElement.querySelector<HTMLElement>(`[data-id="u${s}"]`)!;
      if (el.classList.contains("proc-hidden")) continue;
      const b = el.getBoundingClientRect();
      expect(b.bottom, `第 ${s} 行离视口超过 ${SkeletonView.KEEP_SCREENS} 屏`).toBeGreaterThan(-VIEW_H * SkeletonView.KEEP_SCREENS);
      expect(b.top, `第 ${s} 行离视口超过 ${SkeletonView.KEEP_SCREENS} 屏`).toBeLessThan(VIEW_H * (1 + SkeletonView.KEEP_SCREENS));
    }
    const covered = ids.reduce((a, s) => {
      const b = boxOf(r, s);
      return a + Math.max(0, Math.min(b.bottom, VIEW_H) - Math.max(b.top, 0));
    }, 0);
    expect(covered, "视口里空着").toBeGreaterThanOrEqual(VIEW_H - CARD_H);
    expect(boxOf(r, N - 1).bottom, "没贴底").toBeCloseTo(VIEW_H, 0);
  });

  it("开页之后来的实时行（比账本里最新的还新）照旧当场建卡、贴着底", async () => {
    const r = await rig();
    r.bootBatch();
    await settle(r);
    for (let s = N; s < N + 3; s++) r.live(s);
    const ids = r.built();
    for (let s = N; s < N + 3; s++) expect(ids, `实时行 ${s} 没建卡`).toContain(s);
    expect(boxOf(r, N + 2).bottom, "实时行没贴底").toBeCloseTo(VIEW_H, 0);
  });
});
