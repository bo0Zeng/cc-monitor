/**
 * 冷开一截卡铺满视口之后轮次才落地、过程折起来：那一截缩到不足一屏。整份骨架索引还没到（几百 MB 的记录后端从头扫要几十秒）——
 * 这时照尾段那一截的索引补到一屏：要骨架那一下同时从尾段的字节起点要一份（后端只读那一截），先接上；没有滚动事件来叫，
 * 等整份就是视口下半截一直空着（「画到一半停住」）。整份随后到了 ⇒ 同一个骨架换上整份账本，上面那一截成普通占位，视口不动。
 *
 * 真 `TabStreamView` ＋ 真 `MessageStream` · `RecordTimeline` · `TurnFold` · `SkeletonView`；几何是一维布局模型（jsdom 没有布局引擎）：
 * 卡一律 60px，`proc-hidden` · 过程行 0，占位照它的内联高。IPC 只有骨架索引（整份那一问测试放行才回 · 尾段那一问照回）· 轮次（测试放行才回）是替身。
 */
import { describe, expect, it, onTestFinished, vi } from "vitest";
import type { JsonlLinePayload } from "../../../src/frontend/ui/events";
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
/** 一轮 40 行：开头那一行与结尾那一行露着，中间 38 行是过程（折着）⇒ 折起来以后一轮只剩 120px。 */
const PER = 40;
const CARD_H = 60;
/** 每行 100 字节：第 s 行占 `[s·100, (s+1)·100)`。 */
const LINE_B = 100;

const reply = (seq: number) =>
  ({ agent: "claude", t: "reply", id: `u${seq}`, blocks: [{ type: "text", text: "x" }], autoReply: false, draw: "card", human: false }) as never;
const turn = (k: number) => ({
  at: k * PER * LINE_B,
  line: k * PER,
  uuid: `u${k * PER}`,
  start: "2026-10-06T02:01:00Z",
  end: "2026-10-06T02:04:02Z",
  startText: "02:01",
  said: "q",
  tools: 3,
  thinking: 0,
  agents: 0,
  background: 0,
  retries: 0,
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

/** 骨架索引的行（后端成品的逐行形，`pack-index.ts` 译成按列）：从字节 `from` 起到末尾。 */
const indexFrom = (from: number) => ({
  available: true,
  from,
  end: N * LINE_B,
  rows: Array.from({ length: N }, (_, s) => ({ o: s * LINE_B, n: LINE_B, t: "reply", dr: "card" })).filter((r) => r.o >= from),
});

/** 一维布局：`.stream-content` 的子元素自上而下堆叠。 */
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
  const { withSessionReads } = await import("../../test-support/chan-fake");
  const { TabStreamView } = await import("../../../src/frontend/ui/tab-stream-view");
  const { TabStore } = await import("../../../src/frontend/ui/tab-store");
  const { SKELETON_GAP_CLASS } = await import("../../../src/frontend/ui/skeleton-view");
  vi.stubGlobal("CSS", { escape: (s: string) => s });
  onTestFinished(() => void vi.unstubAllGlobals());
  // 开页那一批的每一行都带它的末字节（壳照后端 `line.byte_offset` 交）。
  const payload = (s: number): JsonlLinePayload =>
    ({ session_id: "c", cwd: "/p", path: "/p/c.jsonl", seq: s, end: (s + 1) * LINE_B, record: reply(s) }) as never;
  const asked: number[] = [];
  let landWhole: () => void = () => undefined;
  const whole = new Promise<void>((r) => (landWhole = r));
  const answer = withSessionReads((cmd, args) => {
    if (cmd === "read_session_index") {
      const from = (args.fromOffset as number | undefined) ?? 0;
      asked.push(from);
      // 整份：后端还在从头扫那一份大记录，测试放行才回；尾段那一截：只读那一截，照回。
      return from === 0 ? whole.then(() => indexFrom(0)) : indexFrom(from);
    }
    return undefined;
  });
  vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => (await answer(cmd, args)) ?? null);
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
  let landTurns: () => void = () => undefined;
  const landed = new Promise<void>((r) => (landTurns = r));
  const turns = vi.fn(async () => {
    await landed;
    return { available: true, from: 0, end: N * LINE_B, turns: Array.from({ length: N / PER }, (_, k) => turn(k)) };
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
  tsv.wireTab(tab as never);
  layoutOf(dom.streamEl, dom.stream.contentElement);
  store.activeId = "c";
  tsv.showOnly("c");
  tsv.activate(tab as never);
  const content = dom.stream.contentElement;
  /** 视口里被露着的卡盖住的像素（占位不算）。 */
  const covered = (): number =>
    Array.from(content.children).reduce((a, c) => {
      if (c.classList.contains(SKELETON_GAP_CLASS)) return a;
      const b = (c as HTMLElement).getBoundingClientRect();
      return a + Math.max(0, Math.min(b.bottom, VIEW_H) - Math.max(b.top, 0));
    }, 0);
  /** 视口里露着的那几张卡：id → 屏幕上的顶。 */
  const tops = (): Map<string, number> => {
    const out = new Map<string, number>();
    for (const c of Array.from(content.children) as HTMLElement[]) {
      if (c.classList.contains(SKELETON_GAP_CLASS) || !c.dataset.id) continue;
      const b = c.getBoundingClientRect();
      if (b.height > 0 && b.bottom > 0 && b.top < VIEW_H) out.set(c.dataset.id, b.top);
    }
    return out;
  };
  const gaps = (): HTMLElement[] => Array.from(content.querySelectorAll<HTMLElement>(`.${SKELETON_GAP_CLASS}`));
  /** 开页那一批：尾段从老到新逐行进来、批收尾（宿主 `onBatchEnd`）。 */
  const bootBatch = (): void => {
    store.inBatch = true;
    for (let s = 0; s < N; s++) host.onLine(payload(s));
    store.inBatch = false;
    tsv.batchEnd();
  };
  return { tab, covered, tops, gaps, bootBatch, turns, landTurns, landWhole, asked };
}

const frames = async (n: number): Promise<void> => {
  for (let k = 0; k < n; k++) await new Promise((r) => requestAnimationFrame(() => r(null)));
};

/** 批收尾铺满 → 尾段索引接上 → 轮次落地折起（整份一直没回）。 */
async function foldedWhileWholeIndexIsOut(r: Awaited<ReturnType<typeof rig>>): Promise<void> {
  r.bootBatch();
  expect(r.covered(), "批收尾就没铺满（前提不成立）").toBeGreaterThanOrEqual(VIEW_H - CARD_H);
  await vi.waitFor(() => expect(r.turns).toHaveBeenCalled(), UNTIL_CEILING);
  await vi.waitFor(() => expect(r.asked, "整份那一问没发").toContain(0), UNTIL_CEILING);
  r.landTurns();
  await vi.waitFor(() => expect(r.tab.turnFold.all.length, "轮次没落地").toBeGreaterThan(0), UNTIL_CEILING);
  await frames(10);
}

describe("轮次落地、过程折起来以后视口不满：整份骨架没到，照尾段那一截补到一屏", () => {
  it("★ 批收尾铺满 → 轮次落地折起 → 整份索引一直不回：尾段那一截从它的字节起点要，视口里露着的卡照样铺满视口", async () => {
    const r = await rig();
    await foldedWhileWholeIndexIsOut(r);
    // 尾段那一问从见过的最老那一行的末字节起（第 0 行之后 ＝ 字节 100），不从 0。
    expect(r.asked, "尾段那一问没从尾段的字节起点要").toContain(LINE_B);
    expect(r.tab.skeleton?.ledger.base, "接上的该是尾段那一截的账本").toBe(1);
    expect(r.covered(), "过程折起来以后视口没再补满（等整份骨架时停住了）").toBeGreaterThanOrEqual(VIEW_H - CARD_H);
  });

  it("整份随后到了：同一个骨架换上整份账本，上面那一截成普通占位，视口里那几张卡不动", async () => {
    const r = await rig();
    await foldedWhileWholeIndexIsOut(r);
    const view = r.tab.skeleton;
    const before = r.tops();
    expect(before.size, "视口里该有卡").toBeGreaterThan(0);
    r.landWhole();
    await vi.waitFor(() => expect(r.tab.skeleton?.ledger.base, "整份没换上").toBe(0), UNTIL_CEILING);
    expect(r.tab.skeleton, "换账本不该换掉骨架").toBe(view);
    expect(
      r.gaps().some((g) => g.dataset.skeletonLo === "0" && g.dataset.skeletonFetching === undefined),
      "账本之上那一截没成普通占位",
    ).toBe(true);
    const after = r.tops();
    for (const [id, top] of before) if (after.has(id)) expect(after.get(id), `卡 ${id} 换账本时挪了`).toBe(top);
    expect(r.covered(), "换账本之后视口不满").toBeGreaterThanOrEqual(VIEW_H - CARD_H);
  });
});
