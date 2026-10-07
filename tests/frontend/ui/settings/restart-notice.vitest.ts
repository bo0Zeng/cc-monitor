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
} from "../../../../src/frontend/ui/settings/restart-notice";
import { copyText } from "../../../../src/frontend/ui/copy-table";

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
    expect(bar.textContent).toBe(copyText("restartNotice.render.pending", { list: "远端机器配置" }));
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
 * 「改动还没生效」是这一次启动的状态：设置窗关了是藏起来（同一个网页），重开条还在；
 * 真重启 = 新进程 = 新的设置窗网页 —— 它的会话存储是空的，而浏览器的持久存储（localStorage）还在。
 * ⇒ 下面「真重启」那条照这个样子造：模块重新载入、会话存储清空、持久存储原样留着，**不碰任何产品自己的键**。
 */
describe("同一次启动里关窗再开条还在，真重启就消", () => {
  beforeEach(() => {
    localStorage.clear();
    sessionStorage.clear();
    __resetRestartNoticeForTests();
  });

  it("★★ 关窗再开（同一次启动）→ 原因还在", () => {
    markRestartNeeded("远端机器配置");
    markRestartNeeded(copyText("settingsPanel.save.claudeDir"));
    // 模拟「设置窗网页重新载入」：内存清空，从这一次启动的存储里重新读
    __rehydrateRestartNoticeForTests();
    expect(restartReasons()).toEqual(["远端机器配置", copyText("settingsPanel.save.claudeDir")]);
  });

  it("★★ monitor 真重启（新进程：模块重新载入、会话存储是空的、持久存储还在）→ 原因清空", async () => {
    const before = await import("../../../../src/frontend/ui/settings/restart-notice");
    before.markRestartNeeded("远端机器配置");
    vi.resetModules();
    sessionStorage.clear(); // 新的设置窗网页：会话存储从空开始（持久存储 localStorage 不动）
    const after = await import("../../../../src/frontend/ui/settings/restart-notice");
    expect(after.restartReasons(), "上一次启动的待重启项在重启后已经生效了，不该还挂着").toEqual([]);
  });

  it("★ 对照：同一次启动里网页重新载入（会话存储还在）→ 原因还在", async () => {
    const before = await import("../../../../src/frontend/ui/settings/restart-notice");
    before.markRestartNeeded("远端机器配置");
    vi.resetModules();
    const after = await import("../../../../src/frontend/ui/settings/restart-notice");
    expect(after.restartReasons()).toEqual(["远端机器配置"]);
  });

  it("存的东西坏了当没有，不抛（这条只是提示，不值得为它报错）", () => {
    sessionStorage.setItem("cc-monitor.settings.restart-reasons", "{ 这不是 JSON");
    expect(() => __rehydrateRestartNoticeForTests()).not.toThrow();
    expect(restartReasons()).toEqual([]);
  });

  it("★ 反向自检：不存的话第一条会绿吗——会，所以钉住确实存了", () => {
    markRestartNeeded("远端机器配置");
    const raw = sessionStorage.getItem("cc-monitor.settings.restart-reasons");
    expect(raw, "根本没存").toBeTruthy();
    expect(JSON.parse(raw as string)).toEqual(["远端机器配置"]);
  });
});
