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
  __rehydrateRestartNoticeForTests,
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

/**
 * ★★ E62：这条状态必须**活过设置窗口**。
 *
 * windowMode 下关设置窗 = 那个 webview 整个没了（`getCurrentWindow().close()`，
 * 下次是 `WebviewWindowBuilder::new` 建全新窗口）。而「改动还没生效」是**进程级**状态：
 * 改完远端配置 → 关窗（这恰恰是改完之后的标准动作）→ 再打开 → 条不该消失，
 * 因为 monitor 根本没重启。
 */
describe("E62：关窗再开条还在，真重启才消", () => {
  beforeEach(() => {
    localStorage.clear();
    __resetRestartNoticeForTests();
  });

  it("★★ 关窗再开（同一个进程）→ 原因还在", () => {
    markRestartNeeded("远端机器配置");
    markRestartNeeded("Claude 数据目录");
    // 模拟「设置窗关了又开」：内存清空，从落盘那份重新读
    __rehydrateRestartNoticeForTests();
    expect(restartReasons()).toEqual(["远端机器配置", "Claude 数据目录"]);
  });

  it("★★ monitor 真重启（bootId 换了）→ 原因清空", () => {
    markRestartNeeded("远端机器配置");
    // 真重启 = 新进程 = 新的 bootId。localStorage 里那份 reasons 还在，但它属于上一个进程。
    localStorage.setItem("cc-monitor.boot-id", "另一个进程");
    __rehydrateRestartNoticeForTests();
    expect(restartReasons(), "上一个进程的待重启项在重启后已经生效了，不该还挂着").toEqual([]);
  });

  it("落盘坏数据当没有，不抛（这条只是提示，不值得为它报错）", () => {
    localStorage.setItem("cc-monitor.settings.restart-reasons", "{ 这不是 JSON");
    expect(() => __rehydrateRestartNoticeForTests()).not.toThrow();
    expect(restartReasons()).toEqual([]);
  });

  it("★ 反向自检：不落盘的话上面第一条会绿吗——会，所以钉住落盘确实发生了", () => {
    markRestartNeeded("远端机器配置");
    const raw = localStorage.getItem("cc-monitor.settings.restart-reasons");
    expect(raw, "根本没写盘").toBeTruthy();
    expect(JSON.parse(raw as string).reasons).toEqual(["远端机器配置"]);
  });
});
