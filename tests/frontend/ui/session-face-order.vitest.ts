/**
 * 会话的脸按流里到达的先后落：`added` · `status` · `state` 三格都带整份脸，界面只走一条路（与行同一个队列）落它们，
 * 后到的盖先到的。曾经 `state` 进队列、`status` / `added` 当场派 ⇒ 「可重连 → 活」这一串里排在后面的旧「已退出」
 * 盖掉「空闲」，之后真退出脸不变、探针也不打。
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

import { bindEvents, type EventHandlers } from "../../../src/frontend/ui/events";
import { streamFake } from "../../test-support/chan-stream-fake.ts";
import { UNTIL_CEILING } from "../../test-support/wall-ceiling";

/** 脸只取判据看的那一格（`dot`）；其余形状不在这条判据里。 */
const face = (dot: string): unknown => ({ dot });
const label = { text: "s" };

async function run(frames: unknown[], faces = frames.length): Promise<{ dots: string[]; names: string[] }> {
  streamFake.reset();
  const dots: string[] = [];
  const names: string[] = [];
  const quiet = new Proxy({}, { get: () => () => {} }) as EventHandlers;
  await bindEvents(
    {
      ...quiet,
      onSessionFace: (_sid, f) => dots.push((f as unknown as { dot: string }).dot),
      onSessionName: (sid) => names.push(sid),
    },
    { streams: [{ origin: "<local>", kind: "session-lines" }] },
  );
  streamFake.lifecycle(frames);
  await vi.waitFor(() => expect(dots).toHaveLength(faces), UNTIL_CEILING);
  return { dots, names };
}

describe("会话的脸按到达先后落（一条路）", () => {
  it("可重连 → 活（status）：最后是「空闲」；之后真退出 ⇒ 落「已退出」", async () => {
    const { dots, names } = await run([
      { state: { sid: "s", state: "reconnectable", face: face("exited") } },
      { status: { sid: "s", face: face("idle"), label } },
      { state: { sid: "s", state: "reconnectable", face: face("exited") } },
    ]);
    expect(dots).toEqual(["exited", "idle", "exited"]);
    expect(names, "名字跟着那一格走").toEqual(["s"]);
  });

  it("可重连 → 活（added 又宣告一次）：最后是「空闲」", async () => {
    const { dots } = await run([
      { state: { sid: "s", state: "reconnectable", face: face("exited") } },
      { added: { sid: "s", agent_kind: "claude", pid: 1, container: { host: "none" }, face: face("idle"), label } },
    ]);
    expect(dots).toEqual(["exited", "idle"]);
  });

  it("排在一大段行后面的「活」不抢到前面：行之前的「已退出」先落", async () => {
    const line = (seq: number): unknown => ({
      line: { session_id: "s", cwd: "/p", path: "/p/s.jsonl", seq, record: { type: "assistant", uuid: `u${seq}` } },
    });
    const { dots } = await run([
      ...Array.from({ length: 40 }, (_, i) => line(i)),
      { state: { sid: "s", state: "reconnectable", face: face("exited") } },
      ...Array.from({ length: 40 }, (_, i) => line(40 + i)),
      { status: { sid: "s", face: face("idle"), label } },
    ], 2);
    expect(dots).toEqual(["exited", "idle"]);
  });
});
