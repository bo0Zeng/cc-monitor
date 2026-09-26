// 〔TAP · V124〕`events.ts` 的 `session-tap` 订阅：tap 走通道 `subscribe`（与会话行同一条帧路，`设计/05 §15`），
// 不开裸 Tauri 事件（`设计/01 §2.2`：前端只有 `call` / `subscribe`）。
//
// 守的要求（住址）：`设计/05 §15.3`「经通道 `subscribe`」· `§3.3.4`「credit 的单位是格，前端每处理完一批就批量还」·
// `设计/20 §8`（SSE 只保快：缺口不补，活卡那一侧按位置号自己撤）。设计住仓外 `调研/第四波记录/TAP.md §1.3`。

import { describe, it, expect, vi, beforeEach } from "vitest";

type Cb = (e: { payload: unknown }) => void;
const subs = new Map<string, Cb>();

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((event: string, cb: Cb) => {
    subs.set(event, cb);
    return Promise.resolve(() => {});
  }),
}));
vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({
    listen: vi.fn((event: string, cb: Cb) => {
      subs.set(event, cb);
      return Promise.resolve(() => {});
    }),
  }),
}));
vi.mock("../src/ipc/commands", () => ({ commands: new Proxy({}, { get: () => vi.fn() }) }));
vi.mock("../src/ipc/chan", async () => (await import("./test-support/chan-stream-fake.ts")).chanStreamModule);

import { bindEvents, TAP_WINDOW } from "../src/events";
import { chanStreamModule, streamFake } from "./test-support/chan-stream-fake.ts";

const tap = (n: number): unknown => ({ origin: "<local>", stream: "sid", resp: 0, n, data: "{}" });

describe("〔TAP〕session-tap 走 subscribe（不是裸事件）", () => {
  beforeEach(() => {
    subs.clear();
    streamFake.reset();
    chanStreamModule.chan.subscribe.mockClear();
  });

  it("订的是 (本机, session-tap)、窗口 TAP_WINDOW；一格一个 tap 交处理器、当场还 credit；Gap 不交；Unseen ⇒ 那台活卡全撤", async () => {
    const got: unknown[] = [];
    const lost: string[] = [];
    await bindEvents(
      { onLine: () => {}, onSessionEnded: () => {}, onSessionTap: (p) => got.push(p), onSessionTapLost: (o) => lost.push(o) },
      { taps: ["<local>"] },
    );
    expect(streamFake.subscriptions.map((s) => [s.origin, s.kind])).toEqual([["<local>", "session-tap"]]);
    expect(chanStreamModule.chan.subscribe.mock.calls[0]?.[3]).toBe(TAP_WINDOW);
    // 没有叫 session-tap 的裸事件监听（零命中，正控：session-ended 那一个在）。
    expect(subs.has("session-tap")).toBe(false);
    expect(subs.has("session-ended")).toBe(true);

    const rec = streamFake.subscriptions[0]!;
    rec.sink([
      { t: "frame", seq: 0, body: JSON.stringify(tap(0)) },
      { t: "frame", seq: 1, body: JSON.stringify(tap(1)) },
    ]);
    expect(got).toEqual([tap(0), tap(1)]);
    expect(rec.wants).toEqual([2]);

    streamFake.gap(2, 5);
    expect(got).toHaveLength(2);
    expect(rec.wants).toEqual([2]); // Gap 不占 credit

    rec.sink([{ t: "frame", seq: 5, body: "not json" }]);
    expect(got).toHaveLength(2); // 读不懂的一格跳过，但它占过 credit ⇒ 照还
    expect(rec.wants).toEqual([2, 1]);

    rec.sink([{ t: "unseen", at: { idx: 1, tag: "read" }, why: "dropped" }]);
    expect(lost).toEqual(["<local>"]);
  });
});
