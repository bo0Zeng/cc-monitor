/**
 * ↗ 的「进行中」（主窗口 ↗ 切到终端：点了之后按钮进「进行中」、超过 300ms 才转圈、一旦显示至少 400ms；同一个会话在飞时再点不重发）。
 * 被测：`tab-session-actions.ts::frontOnce`（↗ 的三个入口都经它）。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../../src/frontend/ui/ipc/commands", () => ({ commands: new Proxy({}, { get: () => vi.fn() }) }));

import {
  FRONT_PENDING_AFTER_MS,
  FRONT_PENDING_MIN_MS,
  frontOnce,
} from "../../../src/frontend/ui/tab-session-actions";

describe("↗ 在飞：不重发 · 慢了才进「进行中」 · 进了至少停够", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("快的一趟（< 300ms）不闪；慢的一趟 300ms 进、完了停满 400ms 才收；在飞时第二下不发", async () => {
    const seen: boolean[] = [];
    const pending = (on: boolean): void => void seen.push(on);
    // 快：100ms 完 ⇒ 一次都不进「进行中」。
    let done!: () => void;
    const fast = frontOnce("a", () => new Promise<void>((r) => (done = r)), pending);
    await vi.advanceTimersByTimeAsync(100);
    done();
    await fast;
    await vi.advanceTimersByTimeAsync(1000);
    expect(seen).toEqual([]);

    // 慢：2s 才完 ⇒ 300ms 那一刻进，完了立刻可收（已停够 400ms）。
    const run = vi.fn(() => new Promise<void>((r) => (done = r)));
    const slow = frontOnce("b", run, pending);
    await vi.advanceTimersByTimeAsync(FRONT_PENDING_AFTER_MS - 1);
    expect(seen).toEqual([]);
    await vi.advanceTimersByTimeAsync(1);
    expect(seen).toEqual([true]);
    // 在飞时再点同一个会话：不重发。
    await frontOnce("b", run, pending);
    expect(run).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1700);
    done();
    await slow;
    expect(seen).toEqual([true, false]);

    // 刚过 300ms 就完：进了之后至少停满 400ms 才收。
    seen.length = 0;
    const edge = frontOnce("c", () => new Promise<void>((r) => (done = r)), pending);
    await vi.advanceTimersByTimeAsync(FRONT_PENDING_AFTER_MS + 50);
    done();
    await vi.advanceTimersByTimeAsync(FRONT_PENDING_MIN_MS - 51);
    expect(seen).toEqual([true]);
    await vi.advanceTimersByTimeAsync(1);
    await edge;
    expect(seen).toEqual([true, false]);
    // 完了之后同一个会话又能点。
    const again = vi.fn(() => Promise.resolve());
    await frontOnce("c", again, pending);
    expect(again).toHaveBeenCalledTimes(1);
  });
});
