/**
 * 〔W5-UI〕会话流关了 ⇒ 出声。
 *
 * 要求住址：`设计/05 §3.3.5`「`subscribe` 不会失败 —— 说不了的在流里原位说」（`closed` 那一格就是「说」，
 * 消费方得把它说给人听）· `audit/E-compat.md §3.3` 吞错普查（`chan_want` 失败被吞 ⇒ 静默停流；
 * 通道那一半在 `tests/ipc/chan.vitest.ts`「S5 信用报不上去」，这里是消费方那一半）。
 *
 * | 判什么 | 形态 |
 * |---|---|
 * | 本机 / 远端的流交来一格 `closed` ⇒ 恰一条 toast，说的是那一台 | 相等 |
 * | 正控：`gap` / `unseen` / `seen` / 普通行 ⇒ 零条 toast（关了才说，别的不吵） | 零命中 |
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));
vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({ listen: vi.fn(() => Promise.resolve(() => {})), label: "main" }),
}));
vi.mock("../src/ipc/commands", () => ({
  commands: new Proxy({}, { get: () => vi.fn().mockResolvedValue(undefined) }),
}));
vi.mock("../src/ipc/chan", async () => (await import("./test-support/chan-stream-fake.ts")).chanStreamModule);
const toast = vi.fn();
vi.mock("../src/error-toast", () => ({ showActionFailureToast: (...a: unknown[]) => toast(...a) }));

import { bindEvents } from "../src/events";
import { streamFake } from "./test-support/chan-stream-fake.ts";

async function bind(): Promise<void> {
  await bindEvents({ onLine: vi.fn(), onSessionEnded: vi.fn() } as never, {
    streams: [
      { origin: "<local>", kind: "session-lines" },
      { origin: "aya", kind: "session-lines" },
    ],
  });
  expect(streamFake.subscriptions.length, "没订到两条会话流 —— 下面零命中地绿").toBe(2);
}

beforeEach(() => {
  streamFake.reset();
  toast.mockReset();
});

describe("〔W5-UI〕会话流关了要出声", () => {
  it("本机那条关了 ⇒ 恰一条 toast，说本机", async () => {
    await bind();
    streamFake.subscriptions[0].sink([{ t: "closed", by: { ours: "Broken" } }]);
    expect(toast.mock.calls).toEqual([
      ["会话更新停了", "本机的会话不会再自动更新。重启 cc-monitor 可以重新接上。"],
    ]);
  });

  it("远端那条关了 ⇒ 恰一条 toast，点名那台", async () => {
    await bind();
    streamFake.subscriptions[1].sink([{ t: "closed", by: { peer: '{"code":"bad_args"}' } }]);
    expect(toast.mock.calls).toEqual([
      ["会话更新停了", "aya 上的会话不会再自动更新。重启 cc-monitor 可以重新接上。"],
    ]);
  });

  it("正控：丢格 / 看不见 / 又看得见 / 普通行 ⇒ 一条都不说", async () => {
    await bind();
    streamFake.gap(3, 5, 0);
    streamFake.subscriptions[1].sink([
      { t: "unseen", at: { idx: 1, tag: "open" }, why: "Unreachable" },
      { t: "seen", from: null },
    ]);
    streamFake.lines([{ session_id: "s", seq: 1, message: { type: "user" } }], 0);
    expect(toast).not.toHaveBeenCalled();
  });
});
