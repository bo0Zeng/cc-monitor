// DEV 探针（`e2e-probe.ts`）：冷开抖动探针跨在冷起补读上 · 反转计数 · 快照里的 tab 行。
import { afterEach, describe, expect, it, vi } from "vitest";
import { countReversals, withTabRows } from "../../../src/frontend/ui/e2e-probe";

const sent = vi.hoisted(() => [] as string[]);
vi.mock("@tauri-apps/api/webviewWindow", () => ({ getCurrentWebviewWindow: () => ({ label: "main" }) }));
vi.mock("../../../src/frontend/ui/ipc/commands", () => ({
  commands: {
    frontend_perf_log: (a: { lines: string }) => {
      sent.push(a.lines);
      return Promise.resolve();
    },
  },
}));

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

// 冷开那一段不走批（壳冷起不整份重放）：探针起停挂在冷起补读上 —— 首段补读的行要喂进 tab 时起，冷起补读落地时停。
describe("冷开抖动：探针跨在冷起补读上", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    document.body.innerHTML = "";
  });

  async function rig(): Promise<{
    probe: typeof import("../../../src/frontend/ui/e2e-probe");
    perf: typeof import("../../../src/frontend/ui/startup-perf");
    frame: (n: number) => void;
  }> {
    vi.resetModules();
    sent.length = 0;
    window.__ccmPerf = { domContentLoaded: 10, themeLoaded: 20, frontendReadyEmit: 30 };
    vi.spyOn(console, "info").mockImplementation(() => {});
    let queued: FrameRequestCallback[] = [];
    vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => {
      queued.push(cb);
      return queued.length;
    });
    vi.stubGlobal("cancelAnimationFrame", () => {
      queued = [];
    });
    const stream = document.createElement("div");
    stream.className = "stream active";
    const content = document.createElement("div");
    content.className = "stream-content";
    content.append(boxed(document.createElement("div"), { left: 0, top: 100, width: 600, height: 40 }));
    stream.append(content);
    document.body.append(stream);
    const frame = (n: number): void => {
      for (let i = 0; i < n; i++) {
        const now = queued;
        queued = [];
        for (const cb of now) cb(0);
      }
    };
    return {
      probe: await import("../../../src/frontend/ui/e2e-probe"),
      perf: await import("../../../src/frontend/ui/startup-perf"),
      frame,
    };
  }

  it("登记之后：首段补读到 ⇒ 起（再到不重起）；冷起补读落地 ⇒ 停，日志里恰好一行抖动读数，帧数是整个窗口的", async () => {
    const { probe, perf, frame } = await rig();
    perf.registerColdProbe({ start: probe.startColdJitterProbe, stop: probe.stopColdJitterProbe });
    perf.noteHistoryArrived();
    frame(2);
    perf.noteHistoryArrived();
    frame(1);
    perf.noteColdFilled();
    frame(2);
    perf.noteColdFilled();
    expect(sent.filter((l) => l.startsWith("[e2e] jitter"))).toEqual([
      "[e2e] jitter frames=3 reversals=0 density=0.000 retargets=0",
    ]);
  });

  it("没登记（生产构建不加载探针）⇒ 补读照常、timeline 照落，没有抖动行", async () => {
    const { perf, frame } = await rig();
    perf.noteHistoryArrived();
    frame(2);
    perf.noteColdFilled();
    expect(sent.some((l) => l.includes("[perf] === 启动管线 timeline"))).toBe(true);
    expect(sent.filter((l) => l.startsWith("[e2e] jitter"))).toEqual([]);
  });
});
