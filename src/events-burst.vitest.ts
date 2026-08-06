// audit-0805 F17：给 `src/events.ts` 补第一条**运行期**判据。
//
// 它是报告 I-9 那个「有守卫、0% 执行」的确凿样本：`generated-boundary-guard` 真会咬人，
// 但它只钉**名字**（把 events.ts 当文本读、从不 import 执行）；而这 135 条语句的
// **运行期覆盖率是 0%** —— 批量调度、突发哨兵、哨兵配对、300ms grace 续期，一行都没被执行过。
//
// 本条先钉**突发哨兵**：V5 已证明它是「live 路没拿到等价合批」这句话不成立的原因
// （`BURST_ENTER_THRESHOLD = 50`），也就是说**它承载着一条被报告写错的事实**——
// 这样一条东西零覆盖，是这批空白里最该先补的。

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
vi.mock("./ipc/commands", () => ({ commands: new Proxy({}, { get: () => vi.fn() }) }));

import { bindEvents } from "./events";

function line(seq: number): { payload: unknown } {
  return {
    payload: {
      session_id: "s",
      cwd: "/p",
      path: "/p/s.jsonl",
      seq,
      message: { type: "assistant", uuid: `u-${seq}` },
    },
  };
}

describe("events.ts 的突发哨兵（audit-0805 F17：这 135 条语句此前 0% 执行）", () => {
  beforeEach(() => subs.clear());

  it("积压超过阈值时主动进 batch 模式（这正是「live 没合批」不成立的原因）", async () => {
    const onBatchStart = vi.fn();
    const onLine = vi.fn();
    await bindEvents({
      onLine,
      onSessionEnded: vi.fn(),
      onBatchStart,
      onBatchEnd: vi.fn(),
    } as never);

    const cb = subs.get("jsonl-line");
    // 抽取器自检：没订上就什么都没测。
    expect(cb, "没订到 jsonl-line —— 本条会零命中地绿（检查 listen 的 mock）").toBeTruthy();

    // 同步连灌 60 条：drain 是 setTimeout(…,0) 排的，同步循环内不会被消费 ⇒ 队列真的堆起来。
    for (let i = 0; i < 60; i++) cb!(line(i));

    await new Promise((r) => setTimeout(r, 50));
    expect(
      onBatchStart.mock.calls.length,
      "★ 积压 60 条（阈值 50）却没有进 batch 模式。\n" +
        "报告 I5′ 写「live 路没拿到等价合批」——V5 复核发现**那句话不成立**，正是因为这里\n" +
        "有一层深度阈值的突发兜底。这条判据钉的就是它：它没了，那句话才真的成立，\n" +
        "而中低速率下逐行 O(N) 的分析也要跟着改。",
    ).toBeGreaterThan(0);
  });
});
