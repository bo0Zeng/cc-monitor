// 启动管线读数的两处出声（`startup-perf.ts`）：首批行到前端那一句 · 整条 timeline 经后端落盘（`fe_perf`）。
// 冷起首个标签页的正文今天走「按行号补读」那一趟（壳冷起不整份重放、流里没有那一段批），这一句与 timeline 要在那一趟上打得出来；
// 接线（`TabStreamView.catchUpTo` 冷起那一形调到这几处）在 `tabs.vitest.ts`「〔stream-watch〕」那一组。

import { describe, it, expect, vi, beforeEach } from "vitest";

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
