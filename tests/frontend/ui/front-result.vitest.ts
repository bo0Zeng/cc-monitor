/**
 * 拉窗结局 ⇒ 底部一条：写好的那一张照摆（句子由本机后端照 `face_core::front` 写、壳照结局码挑，逐种判在 `tests/common/face-core/front_tests.rs`）。切过去了 ⇒ 什么都不出。没有锚在按钮上的浮层（一次性结局只有底部条一种样子）。
 * 接线（只发壳一条命令、壳拒了走失败提示）在 `tabs.vitest.ts`「拉窗」那一组里走。
 */
import { describe, expect, it, vi } from "vitest";
// 底部条本身（排版 · 停留 · 同标题合流 ×N · ［复制详情］）住 `kit/toast.ts` 自己的判据；这里换成 spy，判的是交给它的那几样。
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

  it("同一张连点两次 ⇒ 两次底部条：出新的之前先收起上一条（底部条不把它并进上一条成 ×2，「消息」里各记一条）", () => {
    vi.mocked(toast).mockClear();
    const order: string[] = [];
    vi.mocked(toast).mockImplementation((title) => {
      const n = order.filter((x) => x.startsWith("出")).length + 1;
      order.push(`出${n}:${title}`);
      return () => order.push(`收${n}`);
    });
    const view = { title: "同一类", body: "正文", hint: null, tone: "plain" as const, acts: [] };
    showFrontResult(view, () => {});
    showFrontResult(view, () => {});
    expect(order).toEqual(["出1:同一类", "收1", "出2:同一类"]);
    vi.mocked(toast).mockReset();
  });
});
