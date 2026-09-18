/**
 * ★ 步 4（`设计/10 §2.4`）：**让开机制** —— 换 `MessageChannel` ＋ 「干多久」从猜改成问。
 *
 * # 这里钉的是两件事，它们各有各的降级
 *
 * | # | 改了什么 | 探测什么 | 探不到怎么办 |
 * |---|---|---|---|
 * | ① | `drain → drain` 那条自链的让开方式 | `typeof MessageChannel === "function"` | 退回 `setTimeout(…, 0)` |
 * | ② | 一批干多久 | `navigator.scheduling.isInputPending` | 退回固定预算（40 条 / 8ms）|
 *
 * ②的降级**不是可选的**：`isInputPending` 只有 Chromium 有
 * ⇒ Windows 的 WebView2 有、**Linux 的 WebKitGTK 没有**。两个生产壳里有一个走降级路。
 *
 * # 🔴 为什么这几格要自己伪造时钟
 *
 * 判据量的是「**用哪个数决定停手**」。而在假定时器下 `performance.now()` 是冻住的
 * ⇒ 不伪造的话 `elapsed` 恒 0，两条路都会把队列一次清空，**两条路看起来一模一样**
 * （一格假绿）。所以这里装一个「每问一次走 X 毫秒」的假时钟，让预算真的会到期。
 *
 * # ⚠ 诚实边界
 *
 * - 这里量不到「真机上鼠标卡不卡」。那要真 webview + 真输入，登记在 `设计/10 §7`
 *   （`window.__ccmPerf` 那条读数）。
 * - ①的**正向**那一半（真用上了 `MessageChannel`）只能断「建了通道」——
 *   `MessagePort` 的投递时机**不归假定时器管**（实测：`advanceTimersByTime` 推不动它），
 *   拿它驱动 drain 会得到一格随机红绿的判据。所以正向断构造、反向断行为。
 */
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
vi.mock("../src/ipc/commands", () => ({
  commands: new Proxy({}, { get: () => vi.fn().mockResolvedValue(undefined) }),
}));

import { bindEvents } from "../src/events";

const line = (seq: number): { payload: unknown } => ({
  payload: {
    session_id: "s",
    cwd: "/p",
    path: "/p/s.jsonl",
    seq,
    message: { type: "assistant", uuid: `u-${seq}` },
  },
});

/** 每问一次时钟就走 `stepMs` —— 让预算在同步循环里真的会到期。 */
function fakeClock(stepMs: number): void {
  let t = 0;
  vi.spyOn(performance, "now").mockImplementation(() => {
    const cur = t;
    t += stepMs;
    return cur;
  });
}

/** 装 / 卸 `navigator.scheduling.isInputPending`。`null` = 这台机器问不了。 */
function setInputPending(answer: boolean | null): void {
  const nav = navigator as Navigator & { scheduling?: unknown };
  if (answer === null) {
    delete nav.scheduling;
    return;
  }
  Object.defineProperty(nav, "scheduling", {
    value: { isInputPending: () => answer },
    configurable: true,
  });
}

async function bind(): Promise<{ onLine: ReturnType<typeof vi.fn> }> {
  const onLine = vi.fn();
  await bindEvents({
    onLine,
    onSessionEnded: vi.fn(),
    onBatchStart: vi.fn(),
    onBatchEnd: vi.fn(),
  } as never);
  return { onLine };
}

/** 灌 n 条 jsonl-line（同步入队，drain 还没跑）。 */
function feed(n: number): void {
  const cb = subs.get("jsonl-line");
  expect(cb, "没订到 jsonl-line —— 本文件会零命中地绿").toBeTruthy();
  for (let i = 0; i < n; i++) cb!(line(i));
}

beforeEach(() => {
  subs.clear();
  vi.useFakeTimers();
  // ⚠ 把 `MessageChannel` 探没 —— 这几格要的是**决定性**的 drain 自链。
  //   （正向那一格自己把它装回来。）
  vi.stubGlobal("MessageChannel", undefined);
});
afterEach(() => {
  vi.runOnlyPendingTimers();
  vi.useRealTimers();
  vi.unstubAllGlobals();
  setInputPending(null);
  vi.restoreAllMocks();
});

describe("步 4①：让开方式的特性探测与降级", () => {
  it("探得到 ⇒ 真的建了一条 MessageChannel（自链走它，不走 setTimeout）", async () => {
    const ctor = vi.fn(function (this: unknown) {
      return { port1: {}, port2: { postMessage: (): void => {} } };
    });
    vi.stubGlobal("MessageChannel", ctor);
    await bind();
    expect(ctor, "探得到却没用上 ⇒ drain 自链仍被规范钳到最少 4ms").toHaveBeenCalledTimes(1);
  });

  it("探不到 ⇒ 退回 setTimeout，自链不许断（不是「没得用就不排」）", async () => {
    setInputPending(null);
    fakeClock(0.1);
    const { onLine } = await bind();
    feed(100);
    // 一跳一跳地推，直到队列被清空。断不掉就说明降级路是通的。
    for (let hop = 0; hop < 20 && onLine.mock.calls.length < 100; hop++) {
      await vi.advanceTimersToNextTimerAsync();
    }
    expect(onLine.mock.calls.length, "降级路把 drain 链断了 ⇒ 重放整个停摆").toBe(100);
  });
});

describe("步 4②：一批干多久 —— 能问就问，问不了才猜", () => {
  /**
   * 🔴 第一跳的条数里**有一条不是 payload**：同步灌 100 条时积压越过突发阈值（50），
   * `jsonl-line` 那一路会往**队首** unshift 一个 `batch-start` 哨兵。
   * 它照样占一条配额 ⇒ 固定预算 40 条 ⇒ 第一跳只出 **39** 次 `onLine`。
   * （这一条不是噪声，是真实现的行为；写死 40 才是错的。）
   */
  it("问不了（WebKitGTK）⇒ 固定预算：第一跳被 40 条那道闸掐住", async () => {
    setInputPending(null);
    fakeClock(0.1); // 时钟走得慢 ⇒ 8ms 那道闸到不了，掐住的是条数那道
    const { onLine } = await bind();
    feed(100);
    await vi.advanceTimersToNextTimerAsync();
    expect(onLine.mock.calls.length, "固定预算那道条数闸没生效").toBe(39);
  });

  it("能问 · 没人在操作 ⇒ 不再被 40 条猜死，一口气干完", async () => {
    setInputPending(false);
    fakeClock(0.1); // 全程 < 50ms 的硬上限
    const { onLine } = await bind();
    feed(100);
    await vi.advanceTimersToNextTimerAsync();
    expect(
      onLine.mock.calls.length,
      "这正是「8ms 是猜的」那条：没人碰鼠标时它白让开，一万条光排队就要好几秒",
    ).toBe(100);
  });

  it("能问 · 有人在操作 ⇒ 过了 8ms 立刻让开（不等 40 条）", async () => {
    setInputPending(true);
    fakeClock(1); // 每问一次走 1ms ⇒ 8ms 那道闸很快到
    const { onLine } = await bind();
    feed(100);
    await vi.advanceTimersToNextTimerAsync();
    const first = onLine.mock.calls.length;
    expect(first, "有人在操作还闷头干满一批 ⇒ 鼠标就是卡在这里").toBeLessThan(12);
    expect(first, "一条都不处理就让开 ⇒ 永远干不完").toBeGreaterThan(0);
  });

  it("🔴 能问也要有硬上限：`isInputPending` 说不急，也不许一直干下去", async () => {
    setInputPending(false);
    fakeClock(1); // 每问一次 1ms ⇒ 50ms 上限在第 50 条左右到
    const { onLine } = await bind();
    feed(400);
    await vi.advanceTimersToNextTimerAsync();
    const first = onLine.mock.calls.length;
    // `isInputPending()` 只报告**输入**事件 —— 它不知道"该画一帧了"。
    // 只听它的话，一个没人碰鼠标的长队列能把主线程占住几秒而它一路说"不急"。
    expect(first, "没有硬上限 ⇒ 400 条一口气跑完，掉帧没人管").toBeLessThan(400);
    expect(first, "上限不该把它掐得比固定预算还狠").toBeGreaterThan(39);
  });
});
