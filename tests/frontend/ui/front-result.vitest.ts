/**
 * ↗ 结局族 ⇒ 浮层（逐族手写期望：标题 · 正文 · 按钮 · 色）。切过去了 ⇒ 什么都不出。
 * 远端那几族（那台 / 本机后端的原因 · 通道那一跳）在 `tabs.vitest.ts`「↗ 远端那一格按顺序问三方」里走真接线。
 */
import { describe, expect, it } from "vitest";
import { frontActLabel, frontView, type FrontResult } from "../../../src/frontend/ui/front-result";
import { copyText } from "../../../src/frontend/ui/copy-table";

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
      title: copyText("front.title.unsure"),
      body: copyText("front.body.several", { program: "WindowsTerminal.exe", n: 3 }),
      hint: copyText("front.hint.several"),
      tone: "amber",
      acts: [],
    });
    expect(shape({ kind: "unbound" })).toEqual({ title: copyText("front.title.unbound"), body: copyText("front.body.unbound"), hint: copyText("front.hint.unbound"), tone: "amber", acts: [copyText("front.act.connect")] });
    expect(shape({ kind: "refused" })).toEqual({ title: copyText("front.title.flashing"), body: copyText("front.body.refused"), hint: null, tone: "grey", acts: [] });
    expect(shape({ kind: "window-gone" })?.acts, "不在 tmux 里：没有可接回的").toEqual([]);
    expect(shape({ kind: "window-gone" }, true)).toEqual({ title: copyText("front.title.gone"), body: "", hint: null, tone: "grey", acts: [copyText("front.act.openInTerminal")] });
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
    expect(shape({ kind: "no-window", program: "ssh.exe" })).toEqual({ title: copyText("front.title.noTerminal"), body: copyText("front.body.background"), hint: null, tone: "grey", acts: [] });
    expect(shape({ kind: "unclear" }), "句柄 / 进程号复用：不上细节，并进「窗口无法确定」").toEqual({ title: copyText("front.title.unsure"), body: "", hint: null, tone: "grey", acts: [] });
    expect(shape({ kind: "unknown", detail: "x" })).toEqual({ title: copyText("front.title.failed"), body: copyText("front.body.unknown"), hint: null, tone: "red", acts: [copyText("front.act.copy")] });
  });

  it("［复制详情］复制的是那一族带来的细节原文", () => {
    const v = frontView({ kind: "bad-shape", detail: "session-terminals: why=\"x\"" }, "gpu-01", false)!;
    expect(v.acts).toEqual([{ kind: "copy", detail: "session-terminals: why=\"x\"" }]);
  });
});
