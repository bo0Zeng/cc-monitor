/**
 * `P0b`（`#60` 灰灯不出现）**最后一跳里唯一便宜可测的那一段**〔第十二拍 08-13〕。
 *
 * # 为什么是这一段
 *
 * 08-13 全链真跑把 `#60` 的搜索面收到了**只剩前端**：后端每一跳都量到了，逐条有读数 ——
 * daemon 发 `session_removed`（`cause=Gone`）· `ssh_source` 转发 ·
 * 消费者判 `tmux_origin=Some(...)` → `RemovedDisposition::Idle` → emit `session-idle`
 *（`remote session idle-tmux` 那行日志）。
 *
 * 剩下的链条是：`session-idle` 事件 → `bindEvents` 的订阅 → `onSessionIdle`
 * → `tabs.markTmuxIdle` → 灰点。三段里：
 *
 * | 段 | 谁在钉 |
 * |---|---|
 * | `markTmuxIdle` → 灰点 | `tabs.vitest.ts` F03.2 那组（**已有**） |
 * | `session-idle` 事件 → `onSessionIdle` | **本文件**（此前是 0 条） |
 * | `onSessionIdle` → `markTmuxIdle` 的接线（`main.ts:703`） | 仍无（要真机 DOM，记为诚实边界） |
 *
 * ⚠ 中间那段此前**一条判据都没有**：`grep '"session-idle"' src/*.vitest.ts` 零命中。
 * 也就是说「后端喊了、前端听没听见」这件事，本仓一直没人验 —— 而 `#60` 问的正是它。
 *
 * # 保序也要钉
 *
 * `idle` 与 `ended` **同一个 queue**（`events.ts:116` 逐字：「与 ended 同 queue 保序，
 * 见 onSessionIdle」）。灰灯必须落在**该会话最后一行之后**：反过来的话，
 * 灰点先出现、随后那行 jsonl 又把 tab 拽回 live，用户看到的是「闪一下又亮了」。
 * ⇒ 除了「听见了」，还要钉**顺序**。
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
vi.mock("./commands", () => ({
  commands: { frontend_perf_log: vi.fn(() => Promise.resolve()) },
}));

import { bindEvents } from "./events";

describe("#60 最后一跳：session-idle 事件真的走到 onSessionIdle", () => {
  beforeEach(() => {
    subs.clear();
  });

  it("后端 emit session-idle ⇒ 前端 onSessionIdle 拿到那个 sid", async () => {
    const onSessionIdle = vi.fn();
    const onSessionEnded = vi.fn();
    await bindEvents({ onSessionIdle, onSessionEnded } as never);

    // 抽取器自检：**没订到就零命中地绿** —— 本仓一路在治的那种空真。
    expect(
      subs.get("session-idle"),
      "没订到 `session-idle` —— 本条会零命中地绿（检查 listen 的 mock 或 events.ts 的订阅）",
    ).toBeTruthy();

    subs.get("session-idle")!({ payload: { session_id: "sid-gray" } });
    await vi.waitFor(() => expect(onSessionIdle).toHaveBeenCalledWith("sid-gray"));
    // 反向：它不该顺手把 ended 也叫了（灰灯 ≠ 归档，两条路在后端就分开了）。
    expect(onSessionEnded).not.toHaveBeenCalled();
  });

  it("★ 最后那行接线在：onSessionIdle → tabs.markTmuxIdle（两个窗口各一处）", async () => {
    // # 射程（先说清楚它够不到什么）
    //
    // 这条**证明不了 tab 真的画成了灰** —— 那要真机 DOM（待决 `U10i`）。
    // 它只钉「那行接线还在、且接的是 `markTmuxIdle` 而不是别的」。
    // 值得钉的理由：08-13 全链量到后端四跳全通、前端事件层也通（上面两条），
    // **唯独这一行没有任何判据** —— 它是 `#60` 剩下的搜索面里唯一没人看着的一格。
    //
    // ⚠ 照 `daemon-policy.vitest.ts` 那条的写法**整行钉**，不做裸子串匹配：
    //   子串匹配对 `onSessionIdle: (s) => tabs.archiveTab(s)`（接错了函数）照样绿。
    const { readFileSync } = await import("node:fs");
    const { resolve } = await import("node:path");
    const src = readFileSync(resolve(__dirname, "main.ts"), "utf8");
    const lines = src.split("\n").map((l) => l.trim());
    // 主窗：无条件把该 sid 置灰。
    expect(
      lines.filter((l) => l === "onSessionIdle: (sessionId) => tabs.markTmuxIdle(sessionId),").length,
      "main.ts 主窗少了那行 `onSessionIdle: (sessionId) => tabs.markTmuxIdle(sessionId),`。\n" +
        "① 没有 ⇒ 后端喊了灰灯、前端一个字不做（`#60` 的现象 1 就长这样）；\n" +
        "② 形状变了（接到别的函数）⇒ 灰灯变成归档或复活，UI 说的不是同一件事。",
    ).toBe(1);
    // 视图窗：只认自己那个 sid（多开视图窗时不许互相置灰）。
    expect(
      lines.filter((l) => l === "if (s === sid) tabs.markTmuxIdle(s);").length,
      "视图窗少了 `if (s === sid) tabs.markTmuxIdle(s);` —— 它会停留在陈旧绿灯上。",
    ).toBe(1);
  });

  it("idle 与 ended 同队保序：先发的先到（灰灯不会插到该会话末行之前）", async () => {
    const order: string[] = [];
    await bindEvents({
      onSessionIdle: (s: string) => order.push(`idle:${s}`),
      onSessionEnded: (s: string) => order.push(`ended:${s}`),
    } as never);
    expect(subs.get("session-ended"), "没订到 `session-ended`").toBeTruthy();

    subs.get("session-ended")!({ payload: { session_id: "a" } });
    subs.get("session-idle")!({ payload: { session_id: "b" } });
    await vi.waitFor(() => expect(order.length).toBe(2));
    expect(order).toEqual(["ended:a", "idle:b"]);
  });
});
