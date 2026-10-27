/**
 * ↗ 结局 ⇒ 浮层：写好的那一张照摆（句子由 `cmd_wire::front` 写，逐种判在后端 `front_face_tests.rs`）；
 * 出口自己那一跳的失败（本机后端不认 · 期限到 · 断开 · 认不出）逐种手写期望。切过去了 ⇒ 什么都不出。
 * 接线（session-window 的 tmux 原样交壳、壳没对上摆后端那一张）在 `tabs.vitest.ts`「↗」那一组里走。
 */
import { describe, expect, it } from "vitest";
import { frontActLabel, frontView, type FrontResult } from "../../../src/frontend/ui/front-result";
import { copyText } from "../../../src/frontend/ui/copy-table";

const shape = (r: FrontResult) => {
  const v = frontView(r, "gpu-01");
  return v && { title: v.title, body: v.body, hint: v.hint, tone: v.tone, acts: v.acts.map(frontActLabel) };
};

describe("↗ 结局 ⇒ 浮层", () => {
  it("切过去了 ⇒ null（终端到前面就是回执）", () => {
    expect(frontView({ kind: "switched" }, "本机")).toBeNull();
  });

  it("写好的那一张照摆：字照抄、语气 ⇒ 色、按钮照带", () => {
    const said = (tone: "plain" | "warn" | "fail" | "need") =>
      frontView({ kind: "said", title: "标题原文", body: "正文原文", hint: "灰字原文", tone, acts: [{ kind: "open-in-terminal" }, { kind: "copy", detail: "细节" }] }, "gpu-01");
    expect(said("warn")).toEqual({
      title: "标题原文",
      body: "正文原文",
      hint: "灰字原文",
      tone: "amber",
      acts: [{ kind: "open-in-terminal" }, { kind: "copy", detail: "细节" }],
    });
    expect(said("need")?.tone).toBe("amber");
    expect(said("fail")?.tone).toBe("red");
    expect(said("plain")?.tone).toBe("grey");
    expect(frontView({ kind: "said", title: "t", body: "", tone: "plain", acts: [] }, "gpu-01")?.hint, "没带灰字 ⇒ 不出那一行").toBeNull();
  });

  it("出口自己那一跳的失败", () => {
    expect(shape({ kind: "too-old" }), "本机后端不认这一问 ⇒ 要更新").toEqual({
      title: copyText("front.title.update", { machine: "gpu-01" }),
      body: copyText("front.body.update"),
      hint: null,
      tone: "amber",
      acts: [copyText("front.act.update")],
    });
    expect(shape({ kind: "timeout" })).toEqual({
      title: copyText("front.title.failed"),
      body: copyText("front.body.timeout", { machine: "gpu-01" }),
      hint: null,
      tone: "red",
      acts: [copyText("front.act.retry")],
    });
    expect(shape({ kind: "offline", said: "连接层写的那一句" }), "断开：连接层随失败交出的那一句照摆").toEqual({
      title: copyText("front.title.failed"),
      body: "连接层写的那一句",
      hint: null,
      tone: "red",
      acts: [copyText("front.act.reconnect")],
    });
    expect(shape({ kind: "unknown", detail: "x" })).toEqual({ title: copyText("front.title.failed"), body: copyText("reason.io.unknown"), hint: null, tone: "red", acts: [copyText("detail.act.copy")] });
  });

  it("［复制详情］复制的是带来的细节原文", () => {
    const v = frontView({ kind: "bad-shape", detail: "session-window: empty chain without face" }, "gpu-01")!;
    expect(v.acts).toEqual([{ kind: "copy", detail: "session-window: empty chain without face" }]);
  });

  it("每一种按钮都有字", () => {
    for (const a of [
      { kind: "connect" },
      { kind: "open-in-terminal" },
      { kind: "open-here" },
      { kind: "copy", detail: "x" },
      { kind: "update" },
      { kind: "retry" },
      { kind: "retry-update" },
      { kind: "reconnect" },
    ] as const) {
      expect(frontActLabel(a), a.kind).not.toBe("");
    }
  });
});
