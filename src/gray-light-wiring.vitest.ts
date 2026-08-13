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
