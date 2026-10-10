// Batch14-F42 TurnEndNotifier 判定链测试(依赖全注入,零 DOM/插件依赖)。
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../../src/frontend/ui/behavior", () => ({
  getBehavior: vi.fn().mockResolvedValue({ notifyTurnEnd: true }),
}));

import { TurnEndNotifier, type TurnNotifyPayload } from "../../../src/frontend/ui/turn-notify";

const T0 = 1_700_000_000_000;

function endTurnPayload(tsOffsetMs = 0): TurnNotifyPayload {
  return { record: { t: "reply", at: new Date(T0 + tsOffsetMs).toISOString(), atMs: T0 + tsOffsetMs, endsTurn: true } };
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
    n.observe("s1", "[devbox] 项目甲", endTurnPayload(), false);
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
    n.observe("s1", "t", { record: { t: "reply", endsTurn: true } }, false);
    await flush();
    expect(send).not.toHaveBeenCalled();
  });

  it("不是一轮结束 / 不是回复 → 不通知", async () => {
    const { n, send } = makeNotifier();
    n.observe("s1", "t", { record: { t: "reply", at: new Date(T0).toISOString(), endsTurn: false } }, false);
    n.observe("s1", "t", { record: { t: "said", at: new Date(T0).toISOString() } }, false);
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

// 一轮结束的判定只在后端（`agents/<名>/turn.rs`，与帧 `turn_end` 同一个判定），随记录成品的 `endsTurn` 带来。
// 界面这一份只读那一格：不许再自己认盘上的停止原因 / 报错标记（那是从前两份判定各自漂开的来源，audit-0805 F12）。
describe("一轮结束只读后端的 endsTurn", () => {
  it("turn-notify.ts 的实现区只认 endsTurn，不认盘上的格", async () => {
    const { readFileSync } = await import("node:fs");
    const { resolve, dirname } = await import("node:path");
    const { fileURLToPath } = await import("node:url");
    const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
    const stripComments = (src: string): string =>
      src.replace(/\/\*[\s\S]*?\*\//g, "").replace(/^\s*\/\/.*$/gm, "");
    const ts = stripComments(readFileSync(resolve(ROOT, "src/frontend/ui/turn-notify.ts"), "utf8"));
    const impl = ts.slice(ts.indexOf("observe("));
    expect(impl.length, "剥完注释连 observe( 之后都没了 —— 剥法坏了").toBeGreaterThan(200);
    expect(impl).toContain("endsTurn");
    for (const raw of ["stop_reason", "end_turn", "isApiErrorMessage", '"assistant"']) {
      expect(impl, `实现区又在认盘上的 \`${raw}\`：一轮结束的判定只在后端`).not.toContain(raw);
    }
  });
});

// L2（10-08 · GNOME 真窗口）：系统通知只经壳的 `notify_desktop` 发（平台那一半在 `platform/notify.rs`）。
// 不再经 notification 插件的 JS：插件在 Linux 上每条通知新开一条会话总线连接、发完就丢，GNOME 见发信人的总线名一没、
// 而它又认得出是哪个有窗口的程序 ⇒ 当场把那条通知关掉（真窗口上 dbus-monitor 现打：Notify 之后十几毫秒 NotificationClosed）。
describe("系统通知只经壳的 notify_desktop", () => {
  it("notifySend ⇒ commands.notify_desktop({title, body})，不碰插件", async () => {
    vi.resetModules();
    const sent: unknown[] = [];
    vi.doMock("../../../src/frontend/ui/ipc/commands", () => ({
      commands: { notify_desktop: (a: unknown) => (sent.push(a), Promise.resolve()) },
    }));
    const m = await import("../../../src/frontend/ui/turn-notify");
    await m.notifySend("标题", "正文");
    expect(sent).toEqual([{ title: "标题", body: "正文" }]);
    const { readFileSync } = await import("node:fs");
    expect(readFileSync("src/frontend/ui/turn-notify.ts", "utf8")).not.toContain("plugin-notification");
    vi.doUnmock("../../../src/frontend/ui/ipc/commands");
  });
});
