/**
 * 拉窗结局 ⇒ 底部一条：写好的那一张照摆（句子由 `face_core::front` 写，逐种判在 `tests/common/face-core/front_tests.rs`；
 * 出口这一跳没成的那几张也由壳照它写，`front_missed`）。切过去了 ⇒ 什么都不出。没有锚在按钮上的浮层（一次性结局只有底部条一种样子）。
 * 接线（session-window 的 tmux 原样交壳、壳没对上摆后端那一张、没成的交壳写）在 `tabs.vitest.ts`「拉窗」那一组里走。
 */
import { describe, expect, it, vi } from "vitest";
// 底部条本身（排版 · 停留 · ［复制详情］）住 `kit/toast.ts` 自己的判据；这里换成 spy，判的是交给它的那几样。
vi.mock("../../../src/frontend/ui/kit/toast", () => ({ toast: vi.fn(), failToast: vi.fn() }));
import { frontActLabel, frontView, showFrontResult } from "../../../src/frontend/ui/front-result";
import { toast, type ToastAction } from "../../../src/frontend/ui/kit/toast";

describe("拉窗结局 ⇒ 底部一条", () => {
  it("切过去了 ⇒ null（窗口到前面就是回执）", () => {
    expect(frontView({ kind: "switched" })).toBeNull();
  });

  it("写好的那一张照摆：字照抄、按钮照带", () => {
    const v = frontView({ kind: "said", title: "标题原文", body: "正文原文", hint: "灰字原文", tone: "warn", acts: [{ kind: "open-in-terminal" }, { kind: "copy", detail: "细节" }] });
    expect(v).toEqual({ title: "标题原文", body: "正文原文", hint: "灰字原文", tone: "warn", acts: [{ kind: "open-in-terminal" }, { kind: "copy", detail: "细节" }] });
    expect(frontView({ kind: "said", title: "t", body: "", tone: "plain", acts: [] })?.hint, "没带灰字 ⇒ 不出那一行").toBeNull();
  });

  it("出成底部一条：语气 ⇒ 级别；正文 ＋ 灰字进细节；按钮点了交回调；［复制详情］交底部条自己那一颗", () => {
    const act = vi.fn();
    const at = (tone: "plain" | "warn" | "fail" | "need") => {
      vi.mocked(toast).mockClear();
      showFrontResult({ title: `t-${tone}`, body: "正文", hint: "办法", tone, acts: [{ kind: "open-here" }, { kind: "copy", detail: "细节" }] }, act);
      const [title, detail, opts] = vi.mocked(toast).mock.calls[0];
      return { title, detail, opts: opts ?? {} };
    };
    expect(at("fail").opts.level).toBe("error");
    expect(at("warn").opts.level).toBe("warn");
    expect(at("need").opts.level).toBe("warn");
    const plain = at("plain");
    expect(plain.title).toBe("t-plain");
    expect(plain.opts.level).toBe("info");
    expect(plain.detail).toBe("正文\n办法");
    expect(plain.opts.detail, "［复制详情］复制的是那份细节").toBe("细节");
    const acts = plain.opts.action as ToastAction[];
    expect(acts.map((a) => a.label)).toEqual([frontActLabel({ kind: "open-here" })]);
    acts[0].run();
    expect(act).toHaveBeenCalledWith({ kind: "open-here" });
  });

  it("每一种按钮都有字", () => {
    for (const a of [{ kind: "connect" }, { kind: "open-in-terminal" }, { kind: "open-here" }, { kind: "update" }, { kind: "retry" }, { kind: "retry-update" }, { kind: "reconnect" }] as const) {
      expect(frontActLabel(a), a.kind).not.toBe("");
    }
  });
});
