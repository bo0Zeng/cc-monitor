/**
 * S7：「有改动待重启」条。
 *
 * 最要紧的性质不是「能显示」，而是 **它只在真有改动时出现**——
 * 一个恒显示的警告就是背景噪音，等到真该看的时候没人会看（§12 那次事故的成因）。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import {
  markRestartNeeded,
  restartReasons,
  subscribeRestart,
  createRestartBar,
  __resetRestartNoticeForTests,
} from "./restart-notice";

beforeEach(() => __resetRestartNoticeForTests());

describe("restart-notice", () => {
  it("★ 没有改动时条不出现（不是显示一句空话）", () => {
    const bar = createRestartBar();
    expect(bar.hidden).toBe(true);
    expect(bar.textContent).toBe("");
  });

  it("★ 有改动才出现，且**列出改了什么**（只说「有改动」用户没法判断要不要现在重启）", () => {
    const bar = createRestartBar();
    markRestartNeeded("远端机器配置");
    expect(bar.hidden).toBe(false);
    expect(bar.textContent).toContain("远端机器配置");
    expect(bar.textContent).toContain("重启");
  });

  it("多条改动都列出来，同一条重复标记只算一次", () => {
    const bar = createRestartBar();
    markRestartNeeded("远端机器配置");
    markRestartNeeded("远端机器配置");
    markRestartNeeded("诊断日志");
    expect(restartReasons()).toEqual(["远端机器配置", "诊断日志"]);
    expect(bar.textContent).toContain("远端机器配置");
    expect(bar.textContent).toContain("诊断日志");
  });

  it("同值重复标记不重复通知（否则每敲一个字符就重渲染一次）", () => {
    const fn = vi.fn();
    subscribeRestart(fn);
    markRestartNeeded("x");
    markRestartNeeded("x");
    markRestartNeeded("  x  ");
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it("空/纯空白的原因被忽略（不给条塞一个空条目）", () => {
    markRestartNeeded("");
    markRestartNeeded("   ");
    expect(restartReasons()).toEqual([]);
  });

  it("一个订阅者抛异常不影响其余（同 machine-context 的隔离）", () => {
    const good = vi.fn();
    subscribeRestart(() => {
      throw new Error("BOOM");
    });
    subscribeRestart(good);
    expect(() => markRestartNeeded("x")).not.toThrow();
    expect(good).toHaveBeenCalled();
  });

  it("★ 条上**没有**「知道了」这类关闭按钮", () => {
    // 「改动还没生效」不会因为用户点一下就不成立。给关闭按钮 = 允许他把一个
    // 仍然为真的状态划掉，那正是 §12 那类事故的做法。
    const bar = createRestartBar();
    markRestartNeeded("x");
    expect(bar.querySelector("button")).toBeNull();
  });
});
