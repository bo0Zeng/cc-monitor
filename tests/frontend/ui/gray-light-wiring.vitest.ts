/**
 * 「Claude 已退出 · tmux 会话还在」那一跳的前端一段：会话流里去向那一帧（带整份脸）→ `bindEvents` → `onSessionFace` →
 * `tabs.onFace`。脸落到 tab 上怎么画由 `tabs.vitest.ts` 钉；这里钉听见了、接对了、与行同队保序
 * （去向必须落在该会话最后一行之后，反过来就是「闪一下又亮了」）。
 */
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
vi.mock("../src/commands", () => ({
  commands: { frontend_perf_log: vi.fn(() => Promise.resolve()) },
}));
// 去向那一帧在会话流里（`{"state": …}`）：换成会话流的桩往里灌。
vi.mock("../../../src/comms/inward/chan", async () => (await import("../../test-support/chan-stream-fake.ts")).chanStreamModule);

import { bindEvents } from "../../../src/frontend/ui/events";
import { streamFake } from "../../test-support/chan-stream-fake.ts";

const STREAMS = { streams: [{ origin: "<local>", kind: "session-lines" }] };

import { srcDirOf } from "../../test-support/repo-root";
import { UNTIL_CEILING } from "../../test-support/wall-ceiling";
const face = (dot: string) => ({ dot, live: { is: "no" }, settled: dot === "ended", name: dot, tip: dot, line: dot, tone: "plain", order: 4, can: {} });

describe("#60 最后一跳：会话流里的去向那一帧真的走到 onSessionFace", () => {
  beforeEach(() => {
    subs.clear();
    streamFake.reset();
  });

  it("后端说 Claude 已退出 ⇒ 前端 onSessionFace 拿到那个 sid 与那张脸", async () => {
    const onSessionFace = vi.fn();
    await bindEvents({ onSessionFace } as never, STREAMS);
    // 抽取器自检：**没订到就零命中地绿**。
    expect(streamFake.subscriptions.length, "没订到会话流 —— 本条会零命中地绿").toBe(1);
    streamFake.lifecycle([{ state: { sid: "sid-gray", state: "reconnectable", face: face("exited") } }]);
    await vi.waitFor(() => expect(onSessionFace).toHaveBeenCalledWith("sid-gray", face("exited")), UNTIL_CEILING);
  });

  it("★ 最后那行接线在：onSessionFace → tabs.onFace（主窗一处，独立查看窗零处）", async () => {
    // 证明不了 tab 真的画成了灰（那要真机 DOM）；只钉那行接线还在、接的是 `onFace`。整行钉，不做裸子串匹配。
    const { readFileSync } = await import("node:fs");
    const { resolve } = await import("node:path");
    const src = readFileSync(resolve(srcDirOf(__dirname), "main.ts"), "utf8");
    const lines = src.split("\n").map((l) => l.trim());
    expect(lines.filter((l) => l === "tabs.onFace(sessionId, face);").length, "main.ts 主窗少了那行 `tabs.onFace(sessionId, face);`").toBe(1);
    // 独立查看窗不建标签页：起停走它自己那一条订阅（`followSession` 的 `live`），它不接脸。
    const viewer = readFileSync(resolve(srcDirOf(__dirname), "entry-viewer.ts"), "utf8");
    expect(viewer.includes("onFace"), "独立查看窗又接了脸 —— 它不建标签页，接了也没有点可画").toBe(false);
    expect(viewer.includes("follow: {"), "独立查看窗没跟着会话流（起停 · 跟着长都靠它）").toBe(true);
  });

  it("去向那几帧与行同队保序：先发的先到", async () => {
    const order: string[] = [];
    await bindEvents({ onSessionFace: (s: string, f: { dot: string }) => order.push(`${f.dot}:${s}`) } as never, STREAMS);
    streamFake.lifecycle([{ state: { sid: "a", state: "ended", face: face("ended") } }, { state: { sid: "b", state: "reconnectable", face: face("exited") } }]);
    await vi.waitFor(() => expect(order.length).toBe(2), UNTIL_CEILING);
    expect(order).toEqual(["ended:a", "exited:b"]);
  });
});
