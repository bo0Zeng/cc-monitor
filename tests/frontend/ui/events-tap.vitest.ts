// `events.ts` 的 `session-live-card` 订阅：活卡走通道 `subscribe`（与会话行同一条帧路），
// 不开裸 Tauri 事件（前端只有 `call` / `subscribe`）。
//
// 守的要求（住址）：「经通道 `subscribe`」· `§3.3.4`「credit 的单位是格，前端每处理完一批就批量还」·
// （SSE 只保快：缺口不补，活卡那一侧按位置号自己撤）。设计住仓外。

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
vi.mock("../../../src/frontend/ui/ipc/commands", () => ({ commands: new Proxy({}, { get: () => vi.fn() }) }));
vi.mock("../../../src/comms/inward/chan", async () => (await import("../../test-support/chan-stream-fake.ts")).chanStreamModule);

import { bindEvents, LIVE_CARD_WINDOW, SUBSCRIBE_COALESCE_MS, subscribeTopics } from "../../../src/frontend/ui/events";
import { CHANGED_WINDOW, type Changed, type Topic } from "../../../src/frontend/ui/changed-stream";
import { chanStreamModule, streamFake } from "../../test-support/chan-stream-fake.ts";

const tap = (n: number): unknown => ({
  origin: "<local>",
  live: { sid: "sid", cards: [{ card: n, rid: "m", phase: "awaiting", clipped: false, blocks: [] }] },
});

describe("〔TAP〕session-live-card 走 subscribe（不是裸事件）", () => {
  beforeEach(() => {
    subs.clear();
    streamFake.reset();
    chanStreamModule.chan.subscribe.mockClear();
  });

  it("订的是 (本机, session-live-card)、窗口 LIVE_CARD_WINDOW；一格一帧活卡交处理器、当场还 credit；Gap 不交；Unseen ⇒ 那台活卡全撤", async () => {
    const got: unknown[] = [];
    const lost: string[] = [];
    await bindEvents(
      { onLine: () => {}, onSessionFace: () => {}, onLiveCard: (p) => got.push(p), onLiveCardsLost: (o) => lost.push(o) },
      { liveCards: ["<local>"] },
    );
    expect(streamFake.subscriptions.map((s) => [s.origin, s.kind])).toEqual([["<local>", "session-live-card"]]);
    expect(chanStreamModule.chan.subscribe.mock.calls[0]?.[3]).toBe(LIVE_CARD_WINDOW);
    // 没有叫 session-live-card 的裸事件监听（零命中，正控：task-update 那一个在；session-ended 并进了会话流）。
    expect(subs.has("session-live-card")).toBe(false);
    // `bindEvents` 里最后几条裸 Tauri 监听两边各自退役（`task-update` · 会话起停），正控已无可指 ⇒ 改判「一条裸监听都没有」（`listen` 仍被替身截着，谁长回来谁红）。
    expect(subs.size).toBe(0);

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

// `changed/<topic>`（替掉裸事件 `remote-backend-ready` 与 `task-update`）与 tap 同一条帧路、同一处 `chan.subscribe`。
// 守的要求：「前端只有两个动作」· 「经通道 `subscribe`」· `§3.3.4`（credit 按格还）。
// 设计住仓外。
// 〔DL1 · MIG-3b〕「那台某样东西变了」每台每个主题一条 `changed/<topic>`（替掉裸事件 `remote-backend-ready` 与 `task-update`）；
// seen / gap ⇒ 那一样整份重问；frame ⇒ 变了的那几格（同一个 key 留后一格）；frame 的 credit 当场还；unseen 不问。
describe("〔DL1 · MIG-3b〕changed/<topic> 走 subscribe（不是裸事件）", () => {
  beforeEach(() => {
    subs.clear();
    streamFake.reset();
    chanStreamModule.chan.subscribe.mockClear();
  });

  it("每项一条 (机器, changed/<topic>)、窗口 CHANGED_WINDOW；按主题交处理器、一批一次；unseen 不交", async () => {
    const got: [string, Topic, Changed][] = [];
    await bindEvents(
      { onLine: () => {}, onSessionFace: () => {}, onChanged: (o, t, c) => got.push([o, t, c]) },
      {
        changed: [
          { origin: "<local>", topic: "accounts" },
          { origin: "box-a", topic: "tasks" },
        ],
      },
    );
    expect(streamFake.subscriptions.map((s) => [s.origin, s.kind])).toEqual([
      ["<local>", "changed/accounts"],
      ["box-a", "changed/tasks"],
    ]);
    expect(chanStreamModule.chan.subscribe.mock.calls.map((c) => c[3])).toEqual([CHANGED_WINDOW, CHANGED_WINDOW]);
    // 没有叫 remote-backend-ready / task-update 的裸事件监听；`listen` 仍被替身截着，谁长回来谁红。
    expect(subs.has(["remote", "backend", "ready"].join("-"))).toBe(false);
    expect(subs.has(["task", "update"].join("-"))).toBe(false);
    expect(subs.size).toBe(0);

    const acc = streamFake.subscriptions[0]!;
    acc.sink([{ t: "unseen", at: { idx: 1, tag: "read" }, why: "Dropped" }]);
    expect(got, "unseen 不该交").toEqual([]);
    acc.sink([{ t: "seen", from: null }]);
    expect(got).toEqual([["<local>", "accounts", { cells: [], all: true }]]);

    const t = streamFake.subscriptions[1]!;
    t.sink([
      { t: "frame", seq: 0, body: '{"key":"s1"}' },
      { t: "frame", seq: 1, body: '{"key":"s1"}' },
      { t: "frame", seq: 2, body: '{"key":"s2","body":{"tasks":[]}}' },
    ]);
    expect(got[1]).toEqual([
      "box-a",
      "tasks",
      {
        cells: [
          { key: "s1", rev: null, body: undefined },
          { key: "s2", rev: null, body: { tasks: [] } },
        ],
        all: false,
      },
    ]);
    expect(t.wants).toEqual([3]);
    t.sink([{ t: "gap", fromSeq: 3, toSeq: 5 }]);
    expect(got[2]).toEqual(["box-a", "tasks", { cells: [], all: true }]);
    expect(t.wants, "gap 不占 credit").toEqual([3]);
    t.sink([{ t: "frame", seq: 5, body: '{"key":1}' }]);
    expect(got[3]?.[2].all, "读不懂的格 ⇒ 整份重问").toBe(true);
    expect(acc.wants, "别台那条没动").toEqual([]);
  });
});

// 主窗口之外那几页只调 `subscribeTopics`：同一台在合批窗口里连着来的几批（跨主题、跨批）合成一次交，各页不各合各的。
describe("subscribeTopics：一台一窗合一次", () => {
  beforeEach(() => {
    streamFake.reset();
    chanStreamModule.chan.subscribe.mockClear();
  });

  it("同一台几个主题、几批连着来 ⇒ 窗口到点只交一次（主题各一份、同 key 留后一格）；别的台各自一次；credit 每批当场还；退订之后不再交", async () => {
    vi.useFakeTimers();
    try {
      const got: { origin: string; topics: Record<string, Changed> }[] = [];
      const stop = await subscribeTopics(["<local>", "box-a"], ["accounts", "quota"], (origin, changes) =>
        got.push({ origin, topics: Object.fromEntries(changes) }),
      );
      expect(streamFake.subscriptions.map((s) => [s.origin, s.kind])).toEqual([
        ["<local>", "changed/accounts"],
        ["<local>", "changed/quota"],
        ["box-a", "changed/accounts"],
        ["box-a", "changed/quota"],
      ]);
      const [la, lq, ba] = streamFake.subscriptions;
      la!.sink([{ t: "frame", seq: 0, body: JSON.stringify({ key: "w", rev: "1" }) }]);
      la!.sink([{ t: "frame", seq: 1, body: JSON.stringify({ key: "w", rev: "2" }) }]);
      lq!.sink([{ t: "frame", seq: 0, body: JSON.stringify({}) }]);
      ba!.sink([{ t: "seen" }]);
      expect(la!.wants).toEqual([1, 1]);
      expect(got).toEqual([]);
      vi.advanceTimersByTime(SUBSCRIBE_COALESCE_MS);
      expect(got).toEqual([
        {
          origin: "<local>",
          topics: {
            accounts: { cells: [{ key: "w", rev: "2", body: undefined }], all: false },
            quota: { cells: [{ key: null, rev: null, body: undefined }], all: false },
          },
        },
        { origin: "box-a", topics: { accounts: { cells: [], all: true } } },
      ]);
      stop();
      la!.sink([{ t: "frame", seq: 2, body: JSON.stringify({ key: "w", rev: "3" }) }]);
      vi.advanceTimersByTime(SUBSCRIBE_COALESCE_MS);
      expect(got).toHaveLength(2);
      expect(streamFake.subscriptions.every((s) => s.stopped)).toBe(true);
    } finally {
      vi.useRealTimers();
    }
  });
});
