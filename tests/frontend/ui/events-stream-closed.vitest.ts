/**
 * 一台的会话流关了 · 一直看不见 ⇒ 不弹 toast（A2-17-15-2）：那台连没连上、为什么、怎么修都由栏底机器条那一行说
 * （照那台的状态成品，`machine-strip.ts`）；断线是一种状态，不是一次出错，不拽人。
 *
 * | 判什么 | 形态 |
 * |---|---|
 * | 本机 / 远端的流交来一格 `closed` ⇒ 零条 toast | 零命中 |
 * | 一开始就看不见 · 看见过之后断了，过了 20 秒还没回来 ⇒ 零条 toast | 零命中 |
 * | 正控：流照常订上了（两条） | 相等 |
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));
vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({ listen: vi.fn(() => Promise.resolve(() => {})), label: "main" }),
}));
vi.mock("../../../src/frontend/ui/ipc/commands", () => ({
  commands: new Proxy({}, { get: () => vi.fn().mockResolvedValue(undefined) }),
}));
vi.mock("../../../src/comms/inward/chan", async () => (await import("../../test-support/chan-stream-fake.ts")).chanStreamModule);
const toast = vi.fn();
vi.mock("../../../src/frontend/ui/kit/toast", () => ({ toast: (...a: unknown[]) => toast(...a) }));

import { bindEvents } from "../../../src/frontend/ui/events";
import { streamFake } from "../../test-support/chan-stream-fake.ts";

async function bind(): Promise<void> {
  await bindEvents({ onLine: vi.fn(), onSessionFace: vi.fn() } as never, {
    streams: [
      { origin: "<local>", kind: "session-lines" },
      { origin: "devbox", kind: "session-lines" },
    ],
  });
  expect(streamFake.subscriptions.length, "没订到两条会话流 —— 下面零命中地绿").toBe(2);
}

beforeEach(() => {
  streamFake.reset();
  toast.mockReset();
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
});
afterEach(() => vi.useRealTimers());

describe("会话流关了 · 看不见 ⇒ 不弹 toast（机器条说）", () => {
  it("★ 本机 · 远端那条关了 ⇒ 一条都不说", async () => {
    await bind();
    streamFake.subscriptions[0].sink([{ t: "closed", by: { ours: "Broken" } }]);
    streamFake.subscriptions[1].sink([{ t: "closed", by: { peer: '{"code":"bad_args"}' } }]);
    expect(toast).not.toHaveBeenCalled();
  });

  it("★ 一开始就看不见 · 看见过之后断了，过了 20 秒还没回来 ⇒ 一条都不说", async () => {
    await bind();
    streamFake.subscriptions[1].sink([{ t: "unseen", at: { idx: 1, tag: "open" }, why: "Unreachable" }]);
    streamFake.subscriptions[0].sink([{ t: "seen", from: null }]);
    streamFake.subscriptions[0].sink([{ t: "unseen", at: { idx: 1, tag: "read" }, why: "Dropped" }]);
    vi.advanceTimersByTime(60_000);
    expect(toast).not.toHaveBeenCalled();
  });
});
