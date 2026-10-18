/**
 * 主窗口订的那几台跟着壳的机器表走：起来之后再加一台 ⇒ 那台的会话流当场补订、它的会话宣告进标签页（不重启）；
 * 删掉一台 ⇒ 那台的几条订阅退掉。通知是壳照机器表对齐完远端流之后在本机那条 `changed/machines` 上说的一声，界面据它重问。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));
vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }),
}));
vi.mock("../src/commands", () => ({ commands: { frontend_perf_log: vi.fn(() => Promise.resolve()) } }));
vi.mock("../../../src/comms/inward/chan", async () => (await import("../../test-support/chan-stream-fake.ts")).chanStreamModule);

import { bindEvents } from "../../../src/frontend/ui/events";
import { streamFake } from "../../test-support/chan-stream-fake.ts";
import { UNTIL_CEILING } from "../../test-support/wall-ceiling";

const LOCAL = "<local>";
const face = { dot: "idle", live: { is: "yes", activity: "idle" }, settled: false, name: "空闲", tip: "", line: "", tone: "plain", order: 3, can: {} };
const plan = (origin: string) => ({
  streams: [{ origin, kind: "session-lines" }],
  liveCards: [origin],
  changed: [{ origin, topic: "accounts" as const }],
});
const tableMoved = (): void => streamFake.lifecycle([{}], streamFake.indexOf(LOCAL, "changed/machines"));

describe("主窗口订的那几台跟着机器表走", () => {
  beforeEach(() => streamFake.reset());

  it("起来之后再加一台 ⇒ 补订那台、它的会话宣告建标签页，不重启", async () => {
    let table = [LOCAL];
    const onRemoteSessionAdded = vi.fn();
    const moved = vi.fn();
    await bindEvents({ onSessionFace: vi.fn(), onRemoteSessionAdded } as never, {
      machines: { ask: () => Promise.resolve(table), plan, moved },
    });
    expect(streamFake.indexOf(LOCAL, "changed/machines"), "没订本机的 changed/machines —— 机器表变了听不见").toBeGreaterThanOrEqual(0);
    expect(streamFake.indexOf(LOCAL, "session-lines")).toBeGreaterThanOrEqual(0);
    expect(streamFake.indexOf("box", "session-lines")).toBe(-1);
    expect(moved).toHaveBeenLastCalledWith([LOCAL], [LOCAL], []);

    table = [LOCAL, "box"];
    tableMoved();
    await vi.waitFor(() => expect(streamFake.indexOf("box", "session-lines")).toBeGreaterThanOrEqual(0), UNTIL_CEILING);
    expect(streamFake.indexOf("box", "session-live-card")).toBeGreaterThanOrEqual(0);
    expect(streamFake.indexOf("box", "changed/accounts")).toBeGreaterThanOrEqual(0);
    await vi.waitFor(() => expect(moved).toHaveBeenLastCalledWith([LOCAL, "box"], ["box"], []), UNTIL_CEILING);

    streamFake.lifecycle([{ added: { sid: "s-box", agent_kind: "claude", project_dir: "/w", face, pid: 1 } }], streamFake.indexOf("box", "session-lines"));
    await vi.waitFor(() => expect(onRemoteSessionAdded).toHaveBeenCalledWith("s-box", "box", expect.anything()), UNTIL_CEILING);
  });

  it("删掉一台 ⇒ 那台的订阅全退；表没变 ⇒ 不重订", async () => {
    let table = [LOCAL, "box"];
    const moved = vi.fn();
    await bindEvents({ onSessionFace: vi.fn() } as never, { machines: { ask: () => Promise.resolve(table), plan, moved } });
    const boxSubs = () => streamFake.subscriptions.filter((r) => r.origin === "box");
    expect(boxSubs().length).toBe(3);

    const before = streamFake.subscriptions.length;
    tableMoved();
    await vi.waitFor(() => expect(moved).toHaveBeenCalledTimes(2), UNTIL_CEILING);
    expect(streamFake.subscriptions.length, "表没变却又订了一遍").toBe(before);

    table = [LOCAL];
    tableMoved();
    await vi.waitFor(() => expect(boxSubs().every((r) => r.stopped)).toBe(true), UNTIL_CEILING);
    expect(moved).toHaveBeenLastCalledWith([LOCAL], [], ["box"]);
    expect(streamFake.subscriptions.filter((r) => r.origin === LOCAL).every((r) => !r.stopped)).toBe(true);
  });

  it("问不到机器表 ⇒ 至少订本机，之后再问到再补", async () => {
    let fail = true;
    const moved = vi.fn();
    await bindEvents({ onSessionFace: vi.fn() } as never, {
      machines: { ask: () => (fail ? Promise.reject(new Error("no")) : Promise.resolve([LOCAL, "box"])), plan, moved },
    });
    expect(streamFake.indexOf(LOCAL, "session-lines")).toBeGreaterThanOrEqual(0);
    fail = false;
    tableMoved();
    await vi.waitFor(() => expect(streamFake.indexOf("box", "session-lines")).toBeGreaterThanOrEqual(0), UNTIL_CEILING);
  });
});
