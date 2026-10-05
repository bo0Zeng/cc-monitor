/**
 * ↗ 结局族 ⇒ 浮层（逐族手写期望：标题 · 正文 · 按钮 · 色）。切过去了 ⇒ 什么都不出。
 * 远端那几族（那台 / 本机后端的原因 · 通道那一跳）在 `tabs.vitest.ts`「↗ 远端那一格按顺序问三方」里走真接线。
 */
import { describe, expect, it } from "vitest";
import { frontActLabel, frontView, type FrontResult } from "../../../src/frontend/ui/front-result";

const shape = (r: FrontResult, inTmux = false) => {
  const v = frontView(r, "gpu-01", inTmux);
  return v && { title: v.title, body: v.body, hint: v.hint, tone: v.tone, acts: v.acts.map(frontActLabel) };
};

describe("↗ 结局族", () => {
  it("切过去了 ⇒ null（终端到前面就是回执）", () => {
    expect(frontView({ kind: "switched" }, "本机", false)).toBeNull();
  });

  it("壳那一跳的各族", () => {
    expect(shape({ kind: "several", program: "WindowsTerminal.exe", count: 3 }), "分不清：照实说拉不了、带候选个数，不切不闪").toEqual({
      title: "未切换 · 窗口无法确定",
      body: "WindowsTerminal.exe 3 个窗口 · 本会话终端未登记",
      hint: "在目标窗口新开 PowerShell 标签页并重连后可识别",
      tone: "amber",
      acts: [],
    });
    expect(shape({ kind: "unbound" })).toEqual({ title: "终端窗口无法识别", body: "启动于接上终端之前的 PowerShell", hint: "接上终端后新开的 PowerShell 可识别", tone: "amber", acts: ["接上终端"] });
    expect(shape({ kind: "refused" })).toEqual({ title: "任务栏闪烁中", body: "系统阻止抢前台 · 点任务栏闪烁的窗口", hint: null, tone: "grey", acts: [] });
    expect(shape({ kind: "window-gone" })?.acts, "不在 tmux 里：没有可接回的").toEqual([]);
    expect(shape({ kind: "window-gone" }, true)).toEqual({ title: "终端窗口已关闭", body: "", hint: null, tone: "grey", acts: ["在终端里打开"] });
    expect(shape({ kind: "hosted-by-wt", program: "ssh.exe" })).toEqual({
      title: "未切换 · 终端由 Windows 托管",
      body: "Windows 交给「终端」应用托管 · 定位失败",
      hint: "在「终端」应用里新开标签页再连",
      tone: "amber",
      acts: [],
    });
    expect(shape({ kind: "background-tab", program: "ssh.exe" })).toEqual({
      title: "未切换 · 终端在后台标签页",
      body: "「终端」应用里那个标签页不在前台 · 找不到窗口",
      hint: "切到那个标签页再点 ↗",
      tone: "amber",
      acts: [],
    });
    expect(shape({ kind: "no-window", program: "ssh.exe" })).toEqual({ title: "无终端窗口", body: "后台 · 无终端连接", hint: null, tone: "grey", acts: [] });
    expect(shape({ kind: "unclear" }), "句柄 / 进程号复用：不上细节，并进「窗口无法确定」").toEqual({ title: "未切换 · 窗口无法确定", body: "", hint: null, tone: "grey", acts: [] });
    expect(shape({ kind: "unknown", detail: "x" })).toEqual({ title: "未切换", body: "原因不明", hint: null, tone: "red", acts: ["复制详情"] });
  });

  it("［复制详情］复制的是那一族带来的细节原文", () => {
    const v = frontView({ kind: "bad-shape", detail: "session-terminals: why=\"x\"" }, "gpu-01", false)!;
    expect(v.acts).toEqual([{ kind: "copy", detail: "session-terminals: why=\"x\"" }]);
  });
});
