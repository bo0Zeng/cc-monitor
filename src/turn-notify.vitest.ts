// Batch14-F42 TurnEndNotifier 判定链测试(依赖全注入,零 DOM/插件依赖)。
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("./behavior", () => ({
  getBehavior: vi.fn().mockResolvedValue({ notifyTurnEnd: true }),
}));

import { TurnEndNotifier, type TurnNotifyPayload } from "./turn-notify";

const T0 = 1_700_000_000_000;

function endTurnPayload(tsOffsetMs = 0): TurnNotifyPayload {
  return {
    message: {
      type: "assistant",
      timestamp: new Date(T0 + tsOffsetMs).toISOString(),
      message: { stop_reason: "end_turn" },
    },
  };
}

function makeNotifier(over?: {
  focused?: boolean;
  enabled?: boolean;
  now?: number;
}) {
  const send = vi.fn().mockResolvedValue(undefined);
  const state = { now: over?.now ?? T0 };
  const n = new TurnEndNotifier({
    isFocused: () => over?.focused ?? false,
    now: () => state.now,
    enabled: async () => over?.enabled ?? true,
    send,
  });
  return { n, send, state };
}

async function flush(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
}

describe("F42 TurnEndNotifier", () => {
  beforeEach(() => vi.clearAllMocks());

  it("happy path:实时 end_turn + 失焦 → 通知(标题带 tab 名)", async () => {
    const { n, send } = makeNotifier();
    n.observe("s1", "[aya] 项目甲", endTurnPayload(), false);
    await flush();
    expect(send).toHaveBeenCalledTimes(1);
    expect(send.mock.calls[0][0]).toContain("项目甲");
  });

  it("批量重放(inBatch)→ 不通知", async () => {
    const { n, send } = makeNotifier();
    n.observe("s1", "t", endTurnPayload(), true);
    await flush();
    expect(send).not.toHaveBeenCalled();
  });

  it("窗口聚焦 → 不通知", async () => {
    const { n, send } = makeNotifier({ focused: true });
    n.observe("s1", "t", endTurnPayload(), false);
    await flush();
    expect(send).not.toHaveBeenCalled();
  });

  it("陈旧时间戳(>90s)/缺时间戳 → 不通知", async () => {
    const { n, send } = makeNotifier();
    n.observe("s1", "t", endTurnPayload(-120_000), false);
    n.observe("s1", "t", { message: { type: "assistant", message: { stop_reason: "end_turn" } } }, false);
    await flush();
    expect(send).not.toHaveBeenCalled();
  });

  it("非 end_turn / 非 assistant → 不通知", async () => {
    const { n, send } = makeNotifier();
    n.observe("s1", "t", { message: { type: "assistant", timestamp: new Date(T0).toISOString(), message: { stop_reason: null } } }, false);
    n.observe("s1", "t", { message: { type: "user", timestamp: new Date(T0).toISOString(), message: {} } }, false);
    await flush();
    expect(send).not.toHaveBeenCalled();
  });

  it("同会话 10s 防抖;不同会话互不影响", async () => {
    const { n, send, state } = makeNotifier();
    n.observe("s1", "t", endTurnPayload(), false);
    state.now = T0 + 5_000;
    n.observe("s1", "t", endTurnPayload(5_000), false); // 5s 内再来 → 抑制
    n.observe("s2", "t2", endTurnPayload(5_000), false); // 另一会话不受影响
    state.now = T0 + 11_000;
    n.observe("s1", "t", endTurnPayload(11_000), false); // 过窗 → 放行
    await flush();
    expect(send).toHaveBeenCalledTimes(3);
  });

  it("开关关 → 不通知(且不因判定链前段短路而误发)", async () => {
    const { n, send } = makeNotifier({ enabled: false });
    n.observe("s1", "t", endTurnPayload(), false);
    await flush();
    expect(send).not.toHaveBeenCalled();
  });

  it("disable()(viewer 窗口)→ 永不通知", async () => {
    const { n, send } = makeNotifier();
    n.disable();
    n.observe("s1", "t", endTurnPayload(), false);
    await flush();
    expect(send).not.toHaveBeenCalled();
  });

  it("isSidechain 行(旧版 CC subagent 写主文件)→ 不通知", async () => {
    const { n, send } = makeNotifier();
    n.observe(
      "s1",
      "t",
      {
        message: {
          type: "assistant",
          timestamp: new Date(T0).toISOString(),
          isSidechain: true,
          message: { stop_reason: "end_turn" },
        },
      },
      false,
    );
    await flush();
    expect(send).not.toHaveBeenCalled();
  });

  it("send 抛异常不外泄(console.warn 兜底)", async () => {
    const send = vi.fn().mockRejectedValue(new Error("no permission"));
    const n = new TurnEndNotifier({
      isFocused: () => false,
      now: () => T0,
      enabled: async () => true,
      send,
    });
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    n.observe("s1", "t", endTurnPayload(), false);
    await flush();
    expect(send).toHaveBeenCalled();
    expect(warn).toHaveBeenCalled();
    warn.mockRestore();
  });
});
