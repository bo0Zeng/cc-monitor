/**
 * 会话流关了 ⇒ 出声。
 *
 * 要求：「`subscribe` 不会失败 —— 说不了的在流里原位说」（`closed` 那一格就是「说」，
 * 消费方得把它说给人听）· `audit/E-compat.md §3.3` 吞错普查（`chan_want` 失败被吞 ⇒ 静默停流；
 * 通道那一半在 `tests/comms/inward/chan.vitest.ts`「S5 信用报不上去」，这里是消费方那一半）。
 *
 * | 判什么 | 形态 |
 * |---|---|
 * | 本机 / 远端的流交来一格 `closed` ⇒ 恰一条 toast，说的是那一台 | 相等 |
 * | 正控：`gap` / `unseen` / `seen` / 普通行 ⇒ 零条 toast（关了才说，别的不吵） | 零命中 |
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
vi.mock("../../../src/frontend/ui/error-toast", () => ({ showActionFailureToast: (...a: unknown[]) => toast(...a) }));

import { bindEvents } from "../../../src/frontend/ui/events";
import { streamFake } from "../../test-support/chan-stream-fake.ts";
import { copyText } from "../../../src/frontend/ui/copy-table";

async function bind(): Promise<void> {
  await bindEvents({ onLine: vi.fn(), onSessionEnded: vi.fn() } as never, {
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
      ["会话更新停了", "devbox 上的会话不会再自动更新。重启 cc-monitor 可以重新接上。"],
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

/**
 * 一台机器的会话流一直看不见 ⇒ 说出来（哪台 · 为什么 · 能做什么）。
 * 起步那几秒流里第一格常是「看不见」（那台还在连）——只有过了一段还没看见才说，不猜原因：
 * 从没看见过 ⇒「还没连上」；看见过之后断了 ⇒ 照流里给的那一跳原因说（断了 / 没回应 / 连不上）。
 */
describe("一台机器的会话流一直看不见 ⇒ 说出来", () => {
  const unseen = (why: string): unknown => ({ t: "unseen", at: { idx: 1, tag: "open" }, why });
  const said = (): unknown[] => toast.mock.calls.map((c) => [c[0], c[1]]);
  beforeEach(() => vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] }));
  afterEach(() => vi.useRealTimers());

  it("一开始就看不见、20 秒还没看见 ⇒ 恰一条「还没连上那台」，带「打开设置」", async () => {
    const help = vi.fn();
    await bindEvents({ onLine: vi.fn(), onSessionEnded: vi.fn(), openMachineSettings: help } as never, {
      streams: [{ origin: "devbox", kind: "session-lines" }],
    });
    streamFake.subscriptions[0].sink([unseen("Unreachable")]);
    vi.advanceTimersByTime(19_999);
    expect(toast).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(said()).toEqual([[copyText("events.unseen.neverTitle", { machine: "devbox" }), copyText("events.unseen.neverBody", { machine: "devbox" })]]);
    const opts = toast.mock.calls[0][2] as { action?: { label: string; run: () => void } };
    expect(opts.action?.label).toBe(copyText("main.cmd.openSettings"));
    opts.action?.run();
    expect(help).toHaveBeenCalledTimes(1);
  });

  it("起步还在连：看不见之后不到 20 秒又看见了 ⇒ 一句都不说", async () => {
    await bind();
    streamFake.subscriptions[1].sink([unseen("Unreachable")]);
    vi.advanceTimersByTime(5_000);
    streamFake.subscriptions[1].sink([{ t: "seen", from: null }]);
    vi.advanceTimersByTime(60_000);
    expect(toast).not.toHaveBeenCalled();
  });

  it("看见过之后断了、20 秒没回来 ⇒ 说连接断了（照那一跳的原因），本机说「本机」", async () => {
    await bind();
    streamFake.subscriptions[0].sink([{ t: "seen", from: null }]);
    streamFake.subscriptions[0].sink([unseen("Dropped")]);
    vi.advanceTimersByTime(20_000);
    const local = copyText("control.machine.local");
    expect(said()).toEqual([[copyText("events.unseen.droppedTitle", { machine: local }), copyText("events.unseen.lostBody", { machine: local })]]);
  });

  it("订的时候那台已经看得见（流里没先来「看不见」）、读着读着断了 ⇒ 说连接断了，不说还没连上", async () => {
    await bind();
    streamFake.subscriptions[1].sink([{ t: "unseen", at: { idx: 1, tag: "read" }, why: "Dropped" }]);
    vi.advanceTimersByTime(20_000);
    expect(said()).toEqual([[copyText("events.unseen.droppedTitle", { machine: "devbox" }), copyText("events.unseen.lostBody", { machine: "devbox" })]]);
  });
});
