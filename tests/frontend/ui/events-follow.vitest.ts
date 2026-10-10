// `events.ts::followSession`：跟着一个会话（查看器 · 独立查看窗）订 `session-lines/<sid>`，与整台机器那几条同一处 `chan.subscribe`。
// 流里一批格 ⇒ 那一个会话的事件：行 · 丢了行 · 起停 · 那台看不看得见；吃 credit 的格当场还，不吃的（起停那几种）不还。

import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }),
}));
vi.mock("../../../src/frontend/ui/ipc/commands", () => ({ commands: new Proxy({}, { get: () => vi.fn() }) }));
vi.mock("../../../src/comms/inward/chan", async () => (await import("../../test-support/chan-stream-fake.ts")).chanStreamModule);

import { followSession, STREAM_WINDOW, type FollowEvent } from "../../../src/frontend/ui/events";
import { chanStreamModule, streamFake } from "../../test-support/chan-stream-fake.ts";

const line = (sid: string, seq: number): Record<string, unknown> => ({ session_id: sid, cwd: null, path: `/p/${sid}.jsonl`, seq, record: { agent: "claude", id: `u${seq}`, t: "said" } });
const frame = (seq: number, body: unknown): unknown => ({ t: "frame", seq, body: JSON.stringify(body) });

describe("跟着一个会话", () => {
  beforeEach(() => {
    streamFake.reset();
    chanStreamModule.chan.subscribe.mockClear();
  });

  it("订 (那台, session-lines/<sid>)、窗口 STREAM_WINDOW；行 ⇒ 一批交；别的会话的行不交；起停 · 看不见 · 丢行各一件；credit 只还吃 credit 的格", async () => {
    const got: FollowEvent[] = [];
    const h = await followSession("devbox", "s1", (e) => got.push(e));
    expect(streamFake.subscriptions.map((s) => [s.origin, s.kind])).toEqual([["devbox", "session-lines/s1"]]);
    expect(chanStreamModule.chan.subscribe.mock.calls[0]?.[3]).toBe(STREAM_WINDOW);
    const rec = streamFake.subscriptions[0]!;
    rec.sink([
      frame(0, { line: line("s1", 3) }),
      frame(1, { line: line("s9", 4) }),
      frame(2, { batch: "start" }),
      frame(3, { line: line("s1", 5) }),
      frame(4, { ended: { session_id: "s1" } }),
    ]);
    expect(got).toEqual([{ t: "lines", lines: [line("s1", 3), line("s1", 5)] }, { t: "live", live: false }]);
    expect(rec.wants, "起停那一格不吃 credit：4 格还 4").toEqual([4]);
    got.length = 0;
    rec.sink([frame(5, { live: { session_id: "s1", origin: "devbox" } }), { t: "gap", fromSeq: 6, toSeq: 9 }]);
    rec.sink([{ t: "unseen", at: { idx: 1, tag: "read" }, why: "Dropped" }]);
    rec.sink([{ t: "seen", from: null }]);
    expect(got).toEqual([{ t: "live", live: true }, { t: "gap" }, { t: "sight", seen: false }, { t: "sight", seen: true }]);
    expect(rec.wants, "只有起停格 ⇒ 不还").toEqual([4]);
    h.stop();
  });

  it("主线外清单那一格（branch）⇒ 一件 {t: branch, off}；别的会话的不交；形状不对不交；不吃 credit", async () => {
    const got: FollowEvent[] = [];
    const h = await followSession("devbox", "s1", (e) => got.push(e));
    const rec = streamFake.subscriptions[0]!;
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    rec.sink([
      frame(0, { branch: { session_id: "s1", off: ["u2", "u3"] } }),
      frame(1, { branch: { session_id: "s9", off: ["x"] } }),
      frame(2, { branch: { session_id: "s1", off: [1] } }),
      frame(3, { line: line("s1", 4) }),
    ]);
    warn.mockRestore();
    expect(got).toEqual([{ t: "lines", lines: [line("s1", 4)] }, { t: "branch", off: ["u2", "u3"] }]);
    expect(rec.wants, "branch 那几格不吃 credit：只还那一行").toEqual([1]);
    h.stop();
  });

  it("订阅返回之前就到的格：credit 欠着，返回时一起还", async () => {
    chanStreamModule.chan.subscribe.mockImplementationOnce(((origin: string, kind: string, _f: unknown, _w: number, sink: (items: unknown[]) => void) => {
      const rec = { origin, kind, sink, wants: [] as number[] };
      (streamFake.subscriptions as unknown as unknown[]).push(rec);
      sink([frame(0, { line: line("s1", 1) }), frame(1, { line: line("s1", 2) })]);
      return Promise.resolve({ want: (n: number) => rec.wants.push(n), stop: () => {} });
    }) as never);
    const got: FollowEvent[] = [];
    await followSession("<local>", "s1", (e) => got.push(e));
    expect(got).toEqual([{ t: "lines", lines: [line("s1", 1), line("s1", 2)] }]);
    expect(streamFake.subscriptions[0]!.wants).toEqual([2]);
  });
});
