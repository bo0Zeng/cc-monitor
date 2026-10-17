/**
 * 会话流每一格处理掉都还一格 credit（起停那几格也还）：「没 credit 时也照交」的例外只在 monitor 那一侧，界面不分格种。
 * 格种从生成的 `SessionStreamFrame` 联合里逐种造一个（漏一种 tsc 就红）；整台的那一条与跟一个会话的那一条各核一次。
 */
import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));
vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }),
}));
vi.mock("../../../src/frontend/ui/ipc/commands", () => ({
  commands: new Proxy({}, { get: () => vi.fn().mockResolvedValue(undefined) }),
}));
vi.mock("../../../src/comms/inward/chan", async () => (await import("../../test-support/chan-stream-fake.ts")).chanStreamModule);

import { bindEvents, followSession } from "../../../src/frontend/ui/events";
import type { SessionStreamFrame } from "../../../src/frontend/ui/generated/SessionStreamFrame";
import { streamFake } from "../../test-support/chan-stream-fake.ts";
import { UNTIL_CEILING } from "../../test-support/wall-ceiling";

type Kind = SessionStreamFrame extends infer F ? (F extends unknown ? keyof F : never) : never;

/** 每种格一个（内容只求形状大致对：读不懂的那一格同样要还）。 */
const ONE: { [K in Kind]: unknown } = {
  line: { session_id: "s", cwd: "/p", path: "/p/s.jsonl", seq: 1, record: { type: "assistant", uuid: "u" } },
  batch: "start",
  file_notice: { session_id: "s", origin: "<local>", path: "/p/s.jsonl", change: "gone" },
  added: { sid: "s", agent_kind: "claude", pid: 1, container: { host: "none" } },
  status: { sid: "s" },
  state: { sid: "s", state: "ended" },
  unseen: { origin: "<local>" },
  listed: { origin: "<local>", all: true },
  snapshot_inflight: { count: 0 },
  runs: { sid: "s", runs: [], ended: [] },
  branch: { sid: "s", off: [] },
  watch: { sid: "s", path: "/p/s.jsonl", seq: 0 },
};
const FRAMES = Object.entries(ONE).map(([k, v]) => ({ [k]: v }));
const quiet = new Proxy({}, { get: () => () => {} });
const returned = (i = 0): number => streamFake.subscriptions[i]!.wants.reduce((a, b) => a + b, 0);

describe("会话流的 credit：每格还一格", () => {
  it("整台的会话流：每种格处理完各还一格", async () => {
    streamFake.reset();
    await bindEvents(quiet as never, { streams: [{ origin: "<local>", kind: "session-lines" }] });
    streamFake.lifecycle(FRAMES);
    await vi.waitFor(() => expect(returned(), "有格种处理完没还 credit").toBe(FRAMES.length), UNTIL_CEILING);
  });

  it("跟一个会话的那一条：每种格各还一格", async () => {
    streamFake.reset();
    const h = await followSession("<local>", "s", () => {});
    streamFake.lifecycle(FRAMES);
    expect(returned(), "有格种没还 credit").toBe(FRAMES.length);
    h.stop();
  });
});
