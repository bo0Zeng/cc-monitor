// audit-0805 F17：给 `src/events.ts` 补第一条**运行期**判据。
//
// 它是报告 I-9 那个「有守卫、0% 执行」的确凿样本：`generated-boundary-guard` 真会咬人，
// 但它只钉**名字**（把 events.ts 当文本读、从不 import 执行）；而这 135 条语句的
// **运行期覆盖率是 0%** —— 批量调度、突发哨兵、哨兵配对、300ms grace 续期，一行都没被执行过。
//
// 本条先钉**突发哨兵**：V5 已证明它是「live 路没拿到等价合批」这句话不成立的原因
// （`BURST_ENTER_THRESHOLD = 50`），也就是说**它承载着一条被报告写错的事实**——
// 这样一条东西零覆盖，是这批空白里最该先补的。

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

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

/** 被 spy 走的 `console.warn` 原文（每条判据开头清空）。 */
const warned: string[] = [];

describe("events.ts 的突发哨兵（audit-0805 F17：这 135 条语句此前 0% 执行）", () => {
  beforeEach(() => {
    subs.clear();
    warned.length = 0;
    // 假定时器：让那个 300ms grace 定时器**归本文件管**，而不是归机器忙不忙管。
    vi.useFakeTimers();
    vi.spyOn(console, "warn").mockImplementation((...a: unknown[]) => {
      warned.push(a.map(String).join(" "));
    });
  });

  // 🔴 **收尾处置那个定时器 —— 不是「多等一会儿」。**
  //
  // 病灶逐字〔K-W1C 09-04，机制由 K-R24 道读出〕：`scheduleBatchEnd` 排的
  // `window.setTimeout(…, BATCH_END_GRACE_MS)` 在本文件断言完之后仍然挂着；
  // 没人 `clearTimeout`、也没人等它。它在**测试之外**触发，一路跑进
  // `emitPerfSummary` 的遥测那一跳，而那条路上没有任何调用栈接得住异常
  // ⇒ vitest 记成 **unhandled error：整格红，报文一个判据名都点不出来**，
  // 且红不红取决于机器忙不忙 —— 这就是那条 flaky 的全部机制。
  //
  // 那条前提逐字是「**那个 300ms 定时器会在桩还装着的时候触发**」。
  // 本钩子把它**建立**起来：在桩还装着、假定时器还在的时候把挂起的定时器跑完。
  // ⚠ 「建立前提」≠「等它」：**等是碰运气，跑是决定性的**。
  afterEach(() => {
    vi.runOnlyPendingTimers();
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

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

    await vi.advanceTimersByTimeAsync(50);
    expect(
      onBatchStart.mock.calls.length,
      "★ 积压 60 条（阈值 50）却没有进 batch 模式。\n" +
        "报告 I5′ 写「live 路没拿到等价合批」——V5 复核发现**那句话不成立**，正是因为这里\n" +
        "有一层深度阈值的突发兜底。这条判据钉的就是它：它没了，那句话才真的成立，\n" +
        "而中低速率下逐行 O(N) 的分析也要跟着改。",
    ).toBeGreaterThan(0);
  });

  // K-W1C 09-04：把那条 flaky 的**那一形**变成一条点得出名字的判据。
  //
  // 它钉的不是「不要炸」，是**前提的两半**：
  //   ① 建立 —— 那个 grace 定时器**确实**在桩还装着的时候跑完了（`runAllTimersAsync`，
  //      决定性，不是睡一会儿；也不复述 `BATCH_END_GRACE_MS` 那个常量的值，
  //      「跑完所有挂起的」比「等 300+ms」既更准也不会跟着常量漂）；
  //   ② 检查 —— 遥测那一跳拿到「没有 `.catch` 的返回值」时**出声**。
  //
  // ⚠ 本条**买不到**「真 tauri 环境下这一跳是对的」：这里的桩返回 `undefined`，
  // 走的正是那条岔路。真 Promise 那一支由生产环境自己承担（本波没动它一个字节）。
  it("300ms grace 定时器在桩还装着的时候跑完 —— 遥测那一跳只许出声，不许炸", async () => {
    await bindEvents({
      onLine: vi.fn(),
      onSessionEnded: vi.fn(),
      onBatchStart: vi.fn(),
      onBatchEnd: vi.fn(),
    } as never);

    const cb = subs.get("jsonl-line");
    expect(cb, "没订到 jsonl-line —— 本条会零命中地绿（检查 listen 的 mock）").toBeTruthy();
    for (let i = 0; i < 60; i++) cb!(line(i));

    // 前提①：把**所有**挂起的定时器跑完（drain 链 + 那个 grace 定时器）。
    // 这一句要是抛了，抛的就是那条 flaky 本体 —— 而它现在带着本条的名字。
    await vi.runAllTimersAsync();

    // 前提②：它出声了。⚠ 断的是**生产代码里那句话的子串**，不是夹具的名字。
    expect(
      warned.join("\n"),
      "★ grace 定时器跑完了，而遥测那一跳既没出声、也没炸 —— 两种可能：\n" +
        "  · `emitPerfSummary` 根本没被调到（那本条在零命中地绿，去核 onBatchEnd 那一支）；\n" +
        "  · 或者有人把那条「前提不成立就出声」的岔路删了 —— 它一删，这条链就退回\n" +
        "    「一条点不出判据名的 unhandled error，且红不红看机器忙不忙」。",
    ).toContain("frontend_perf_log 的返回值没有 .catch");
  });
});
