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

  // ★★ audit-0805 F12 / 报告 §4.1「turn-end 判定两份」。
  //
  // daemon 侧 `observe/turn_detect.rs` 的四条件里有 `!isApiErrorMessage`，TS 这份**少了一条** ——
  // 而那个字段在生成物 `generated/JsonlRecord.ts` 的 assistant 变体里一直存在：**数据在线上、没人看**。
  //
  // ⚠ V3 复核订正过报告的因果：monitor **不消费** daemon 的 TurnEnd 帧（全仓零帧消费点），
  //   通知是前端自己逐行算的 ⇒ 两者是**互不相通的两个探测器**，不是「一个不发另一个弹」。
  //   而且本机语料 107 条 isApiErrorMessage 记录里 end_turn **0 条** ⇒ 这是**潜伏缺口不是冒烟 bug**。
  //   修它是因为「哪天某个 CC 版本在错误记录上写 end_turn，就直接弹」，不是因为它现在在响。
  it("isApiErrorMessage 行(API 错误带 end_turn)→ 不通知", async () => {
    const { n, send } = makeNotifier();
    n.observe(
      "s1",
      "t",
      {
        message: {
          type: "assistant",
          timestamp: new Date(T0).toISOString(),
          isApiErrorMessage: true,
          message: { stop_reason: "end_turn" },
        },
      },
      false,
    );
    await flush();
    expect(send).not.toHaveBeenCalled();
  });

  // 反向：**不是**错误消息时照常通知（防把上面写成「凡带这个字段就不通知」）。
  it("isApiErrorMessage:false 仍然照常通知", async () => {
    const { n, send } = makeNotifier();
    n.observe(
      "s1",
      "t",
      {
        message: {
          type: "assistant",
          timestamp: new Date(T0).toISOString(),
          isApiErrorMessage: false,
          message: { stop_reason: "end_turn" },
        },
      },
      false,
    );
    await flush();
    expect(send).toHaveBeenCalled();
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

// ★★ 跨语言对拍：**两份 turn-end 判定不许再各自漂**〔audit-0805 F12，定框 E3〕。
//
// 报告 §4.1 那张「同一职责多处落地」表里，turn-end 是「**是**（TS 少 `!isApiError`）」那一行。
// 修完只是把今天对齐了 —— **没有任何东西阻止它明天再漂开**。
// E3 要的不是「两份都改对」，是「**权威源恰好一个**」；两份实现天生做不到那个，
// 退而求其次就是**对拍**：daemon 那份的四个判别条件，TS 这份必须一个不少。
describe("turn-end 判定的跨语言对拍（audit-0805 F12）", () => {
  it("daemon 的四个判别条件在 TS 侧一个不少", async () => {
    const { readFileSync } = await import("node:fs");
    const { resolve, dirname } = await import("node:path");
    const { fileURLToPath } = await import("node:url");
    const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
    const rs = readFileSync(
      resolve(ROOT, "remote-daemon-proto/src/observe/turn_detect.rs"),
      "utf8",
    );
    // ⚠ **必须剥注释**。第一版没剥，于是变异「连字段带判定一起删」照样绿 ——
    //   因为 `isApiErrorMessage` 在我自己写的那段注释里还出现着。
    //   这正是本仓记过的失效形态：**判据匹配到了文档注释里的关键字**。
    const stripComments = (src: string): string =>
      src.replace(/\/\*[\s\S]*?\*\//g, "").replace(/^\s*\/\/.*$/gm, "");
    const ts = stripComments(readFileSync(resolve(ROOT, "src/turn-notify.ts"), "utf8"));

    // 抽取器自检：拿错文件时下面会零命中地绿。
    expect(rs, "turn_detect.rs 里没有 is_turn_end —— 抽取器坏了").toContain("fn is_turn_end");
    // 抽取器自检之二：剥完注释还得剩下真代码，否则下面全是零命中地绿。
    expect(ts.length, "剥完注释 turn-notify.ts 只剩空壳 —— 剥法坏了").toBeGreaterThan(500);
    expect(ts, "剥完注释连 observe( 都没了 —— 剥法把代码也吃了").toContain("observe(");

    // daemon 那份的四条（`is_turn_end` 的合取项），逐条要求 TS 侧有对应判别。
    const pairs: Array<[string, string, string]> = [
      ["is_assistant", '"assistant"', "assistant 类型判别"],
      ["end_turn", "end_turn", "stop_reason == end_turn"],
      ["is_api_error", "isApiErrorMessage", "API 错误消息要排除"],
      ["is_sidechain", "isSidechain", "subagent 行要排除"],
    ];
    for (const [rustToken, tsToken, what] of pairs) {
      expect(rs, `turn_detect.rs 里找不到 ${rustToken} —— 抽取器坏了或 daemon 那份变了`).toContain(
        rustToken,
      );
      expect(
        ts,
        `★ TS 侧缺「${what}」（找不到 \`${tsToken}\`）。\n` +
          `daemon 的 turn_detect.rs 有这一条，TS 这份没有 ⇒ 同一件事两个口径。\n` +
          `报告 §4.1 记的正是这个：turn-end 判定两份、TS 少 !isApiError。\n` +
          `⚠ 后果是潜伏的而不是在响的（本机 107 条 API 错误记录里 end_turn 0 条），\n` +
          `但哪天某个 CC 版本在错误记录上写 end_turn，用户就会为一次失败收到「完成」通知。`,
      ).toContain(tsToken);
    }
  });
});
