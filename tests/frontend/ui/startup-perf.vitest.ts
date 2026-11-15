// 启动管线读数的两处出声（`startup-perf.ts`）：首批行到前端那一句 · 整条 timeline 经后端落盘（`fe_perf`）。
// 冷起首个标签页的正文今天走「按行号补读」那一趟（壳冷起不整份重放、流里没有那一段批），这一句与 timeline 要在那一趟上打得出来；
// 接线（`TabStreamView.catchUpTo` 冷起那一形调到这几处）在 `tabs.vitest.ts`「〔stream-watch〕」那一组。

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const sent = vi.hoisted(() => [] as string[]);
vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({ label: "main", listen: () => Promise.resolve(() => {}) }),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: () => Promise.resolve(() => {}) }));
// 会话流换成桩（`test-support/chan-stream-fake.ts`）：下面那一组真跑 `events.ts` 的批收尾。
// 桩的灌口跟着模块一起出（`fake`）：每条 `resetModules` 以后 events 订的与判据灌的是同一份。
vi.mock("../../../src/comms/inward/chan", async () => {
  const m = await import("../../test-support/chan-stream-fake.ts");
  return { ...m.chanStreamModule, fake: m.streamFake };
});
vi.mock("../../../src/frontend/ui/ipc/commands", () => ({
  commands: {
    frontend_perf_log: (a: { lines: string }) => {
      sent.push(a.lines);
      return Promise.resolve();
    },
  },
}));

describe("启动管线读数：冷起补读那一趟", () => {
  beforeEach(() => {
    vi.resetModules();
    sent.length = 0;
    window.__ccmPerf = { domContentLoaded: 10, themeLoaded: 20, frontendReadyEmit: 30 };
  });

  it("取回的行喂进 tab ⇒ 首批那一句只打一次；落地 ⇒ timeline 落一次盘，建卡数照读、drained 是喂进去的行数", async () => {
    const perf = await import("../../../src/frontend/ui/startup-perf");
    const info = vi.spyOn(console, "info").mockImplementation(() => {});
    perf.noteHistoryArrived();
    window.__ccmPerf.recordsRendered = 7;
    perf.noteHistoryFed(40);
    perf.noteHistoryArrived();
    perf.noteHistoryFed(2);
    const firsts = info.mock.calls.filter((c) => String(c[0]).startsWith("[perf] first stream batch received @ "));
    expect(firsts, "首批那一句该恰好一次").toHaveLength(1);
    expect(window.__ccmPerf.firstJsonlBatch).toBeTypeOf("number");
    perf.noteColdFilled();
    perf.noteColdFilled();
    expect(sent, "timeline 该恰好落一次盘").toHaveLength(1);
    expect(sent[0]).toContain("[perf]   建卡 rendered=7 · 收纳 deferred=0 · drained=42(累计)");
    expect(sent[0]).toContain("[perf] === 启动管线 timeline");
    info.mockRestore();
  });

  it("重放批先到（会话流里的 `batch` 边界）⇒ 补读那一趟不再打第二句", async () => {
    const perf = await import("../../../src/frontend/ui/startup-perf");
    const info = vi.spyOn(console, "info").mockImplementation(() => {});
    perf.noteFirstBatch();
    perf.noteHistoryArrived();
    expect(info.mock.calls.filter((c) => String(c[0]).startsWith("[perf] first stream batch received @ "))).toHaveLength(1);
    info.mockRestore();
  });
});

// timeline 那几行都是同一个起点（页面的 performance.now）上的时刻，drained 与 onBatchEnd 记的是启动那一批的：
// 页面开了很久以后实时行突发再进一次批模式、又落一次盘，那两行也不许换成那一刻（11-13 读到「drained T+192364389」＝ 53 小时）。
describe("启动管线读数：drained 照启动那一批", () => {
  let clock = 0;
  beforeEach(() => {
    vi.resetModules();
    sent.length = 0;
    clock = 0;
    window.__ccmPerf = { domContentLoaded: 10, themeLoaded: 20, frontendReadyEmit: 30 };
    vi.useFakeTimers();
    vi.spyOn(performance, "now").mockImplementation(() => clock);
    vi.spyOn(console, "info").mockImplementation(() => {});
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  const payload = (seq: number): unknown => ({
    session_id: "s",
    cwd: "/p",
    path: "/p/s.jsonl",
    seq,
    message: { type: "assistant", uuid: `u-${seq}` },
  });

  async function bind(): Promise<typeof import("../../test-support/chan-stream-fake.ts").streamFake> {
    const { bindEvents } = await import("../../../src/frontend/ui/events");
    const streamFake = ((await import("../../../src/comms/inward/chan")) as unknown as {
      fake: typeof import("../../test-support/chan-stream-fake.ts").streamFake;
    }).fake;
    streamFake.reset();
    await bindEvents({ onLine: vi.fn(), onSessionFace: vi.fn(), onBatchStart: vi.fn(), onBatchEnd: vi.fn() } as never, {
      streams: [{ origin: "<local>", kind: "session-lines" }],
    });
    expect(streamFake.subscriptions.length, "没订到会话流 —— 本组会零命中地绿").toBe(1);
    return streamFake;
  }

  const drainedLine = (t: string): string => t.split("\n").find((l) => l.includes("batch payloads drained")) ?? "";

  it("冷起补读在 T+1100 喂完；53 小时后实时突发收尾 ⇒ 每一份 timeline 的 drained 都读 T+1100", async () => {
    const perf = await import("../../../src/frontend/ui/startup-perf");
    const stream = await bind();
    clock = 1000;
    perf.noteHistoryArrived();
    clock = 1100;
    perf.noteHistoryFed(5);
    clock = 1200;
    perf.noteColdFilled();
    expect(sent, "冷起那一份该落盘").toHaveLength(1);

    clock = 192_364_389;
    stream.chunk(true, [payload(1), payload(2)]);
    await vi.runAllTimersAsync();
    expect(sent, "突发那一批收尾该再落一份").toHaveLength(2);
    for (const t of sent) {
      expect(drainedLine(t), "drained 读成了突发那一刻的页龄").toContain("T+1100 (+100)");
      expect(t).toContain("onBatchEnd fired       T+1200 (+100)");
    }
  });

  it("重放批在 T+500 收尾；53 小时后又一批 ⇒ 两份 timeline 的 drained 都读 T+500", async () => {
    const stream = await bind();
    clock = 500;
    stream.chunk(true, [payload(1)]);
    await vi.advanceTimersByTimeAsync(0);
    clock = 800;
    await vi.runAllTimersAsync();
    expect(sent, "启动那一批收尾该落盘").toHaveLength(1);

    clock = 192_364_389;
    stream.chunk(true, [payload(2)]);
    await vi.runAllTimersAsync();
    expect(sent, "后一批收尾该再落一份").toHaveLength(2);
    for (const t of sent) {
      expect(drainedLine(t), "drained 读成了后一批那一刻").toContain("T+500 (+0)");
      expect(t).toContain("onBatchEnd fired       T+800 (+300)");
    }
  });
});
