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
import { ACCOUNTS_CHANGED_KIND, ACCOUNTS_CHANGED_WINDOW } from "../src/session-accounts-poll";
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
    // 没有叫 session-tap 的裸事件监听（零命中，正控：task-update 那一个在；〔MIG-1〕session-ended 并进了会话流）。
    expect(subs.has("session-tap")).toBe(false);
    // 〔合并 MIG-1 × 主线 eebf51de〕`bindEvents` 里最后几条裸 Tauri 监听两边各自退役（`task-update` · 会话起停），正控已无可指 ⇒ 改判「一条裸监听都没有」（`listen` 仍被替身截着，谁长回来谁红）。
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

// 〔DL1 · 第五波〕`accounts-changed`（替掉裸事件 `remote-backend-ready`）与 tap 同一条帧路、同一处 `chan.subscribe`。
// 守的要求：`设计/01 §2.2`「前端只有两个动作」· `设计/05 §15.3`「经通道 `subscribe`」· `§3.3.4`（credit 按格还）。
// 设计住仓外 `调研/第四波记录/DL1.md §3`。
describe("〔DL1〕accounts-changed 走 subscribe（不是裸事件）", () => {
  beforeEach(() => {
    subs.clear();
    streamFake.reset();
    chanStreamModule.chan.subscribe.mockClear();
  });

  it("每台一条 (机器, accounts-changed)、窗口 ACCOUNTS_CHANGED_WINDOW；seen / frame / gap ⇒ 刷（一批一次），frame 的 credit 当场还；unseen 不刷", async () => {
    let refreshed = 0;
    await bindEvents(
      { onLine: () => {}, onSessionEnded: () => {}, onAccountsChanged: () => refreshed++ },
      { accounts: ["<local>", "box-a"] },
    );
    expect(streamFake.subscriptions.map((s) => [s.origin, s.kind])).toEqual([
      ["<local>", "accounts-changed"],
      ["box-a", "accounts-changed"],
    ]);
    expect(ACCOUNTS_CHANGED_KIND).toBe("accounts-changed");
    expect(chanStreamModule.chan.subscribe.mock.calls.map((c) => c[3])).toEqual([
      ACCOUNTS_CHANGED_WINDOW,
      ACCOUNTS_CHANGED_WINDOW,
    ]);
    // 没有叫 remote-backend-ready 的裸事件监听（零命中，正控：task-update 那一个在）。
    expect(subs.has(["remote", "backend", "ready"].join("-"))).toBe(false);
    // 〔合并 MIG-1 × 主线 eebf51de〕`bindEvents` 里最后几条裸 Tauri 监听两边各自退役（`task-update` · 会话起停），正控已无可指 ⇒ 改判「一条裸监听都没有」（`listen` 仍被替身截着，谁长回来谁红）。
    expect(subs.size).toBe(0);

    const a = streamFake.subscriptions[1]!;
    a.sink([{ t: "unseen", at: { idx: 1, tag: "read" }, why: "Dropped" }]);
    expect(refreshed, "unseen 不该刷").toBe(0);
    a.sink([{ t: "seen", from: null }]);
    expect(refreshed).toBe(1);
    a.sink([
      { t: "frame", seq: 0, body: '{"accounts_changed":true}' },
      { t: "frame", seq: 1, body: '{"accounts_changed":true}' },
    ]);
    expect(refreshed, "一批两格只刷一次").toBe(2);
    expect(a.wants).toEqual([2]);
    a.sink([{ t: "gap", fromSeq: 2, toSeq: 4 }]);
    expect(refreshed).toBe(3);
    expect(a.wants, "gap 不占 credit").toEqual([2]);
    expect(streamFake.subscriptions[0]!.wants, "别台那条没动").toEqual([]);
  });
});

// 〔MIG-3b · 要求住址 `设计/99 §2.1 ㉓②`「`session.tasks` 推送改 `chan.subscribe(origin, …)`，监视进后端，本机远端同形，monitor 的 notify 与 `task-update` 事件删」〕
// 每台一条 `session-tasks`；一格 = 一个 sid 要重问；seen / gap ⇒ 那台整台重问；frame 的 credit 当场还；没有 `task-update` 裸事件。
describe("〔MIG-3b〕session-tasks 走 subscribe（不是裸事件 task-update）", () => {
  beforeEach(() => {
    subs.clear();
    streamFake.reset();
    chanStreamModule.chan.subscribe.mockClear();
  });

  it("每台一条 (机器, session-tasks)；frame ⇒ 那几个 sid、seen / gap ⇒ 整台；credit 按格还；unseen 不问", async () => {
    const asked: [string, string[], boolean][] = [];
    await bindEvents(
      { onLine: () => {}, onSessionEnded: () => {}, onTasksChanged: (o, sids, all) => asked.push([o, [...sids], all]) },
      { tasks: ["<local>", "box-a"] },
    );
    expect(streamFake.subscriptions.map((s) => [s.origin, s.kind])).toEqual([
      ["<local>", SESSION_TASKS_KIND],
      ["box-a", SESSION_TASKS_KIND],
    ]);
    expect(SESSION_TASKS_KIND).toBe("session-tasks");
    expect(chanStreamModule.chan.subscribe.mock.calls.map((c) => c[3])).toEqual([SESSION_TASKS_WINDOW, SESSION_TASKS_WINDOW]);
    expect(subs.has(["task", "update"].join("-")), "裸事件 task-update 又长回来了").toBe(false);
    // 〔合并 MIG-1 × 主线 eebf51de〕`bindEvents` 里最后几条裸 Tauri 监听两边各自退役（`task-update` · 会话起停），正控已无可指 ⇒ 改判「一条裸监听都没有」（`listen` 仍被替身截着，谁长回来谁红）。
    expect(subs.size).toBe(0);

    const a = streamFake.subscriptions[1]!;
    a.sink([{ t: "unseen", at: { idx: 1, tag: "read" }, why: "Dropped" }]);
    expect(asked).toEqual([]);
    a.sink([
      { t: "frame", seq: 0, body: '{"sid":"s1"}' },
      { t: "frame", seq: 1, body: '{"sid":"s1"}' },
      { t: "frame", seq: 2, body: '{"sid":"s2"}' },
    ]);
    expect(asked).toEqual([["box-a", ["s1", "s2"], false]]);
    expect(a.wants).toEqual([3]);
    a.sink([{ t: "gap", fromSeq: 3, toSeq: 5 }]);
    expect(asked[1]).toEqual(["box-a", [], true]);
    expect(a.wants, "gap 不占 credit").toEqual([3]);
  });

  it("两侧同一个串：Rust `event_replay.rs::SESSION_TASKS_KIND` == TS `SESSION_TASKS_KIND`", () => {
    const rs = readFileSync(resolve(REPO_ROOT, "src/frontend/shell/src/event_replay.rs"), "utf8");
    expect(rs).toContain(`pub const SESSION_TASKS_KIND: &str = "${SESSION_TASKS_KIND}";`);
  });
});

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { REPO_ROOT } from "./test-support/repo-root";
import { SESSION_TASKS_KIND, SESSION_TASKS_WINDOW } from "../src/tasks-stream";
