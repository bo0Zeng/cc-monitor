/**
 * ↗ 结局 ⇒ 浮层：写好的那一张照摆（句子由 `face_core::front` 写，逐种判在 `tests/common/face-core/front_tests.rs`；
 * 出口这一跳没成的那几张也由壳照它写，`front_missed`）。切过去了 ⇒ 什么都不出。
 * 接线（session-window 的 tmux 原样交壳、壳没对上摆后端那一张、没成的交壳写）在 `tabs.vitest.ts`「↗」那一组里走。
 */
import { describe, expect, it } from "vitest";
import { frontActLabel, frontView } from "../../../src/frontend/ui/front-result";

describe("↗ 结局 ⇒ 浮层", () => {
  it("切过去了 ⇒ null（终端到前面就是回执）", () => {
    expect(frontView({ kind: "switched" })).toBeNull();
  });

  it("写好的那一张照摆：字照抄、语气 ⇒ 色、按钮照带", () => {
    const said = (tone: "plain" | "warn" | "fail" | "need") =>
      frontView({ kind: "said", title: "标题原文", body: "正文原文", hint: "灰字原文", tone, acts: [{ kind: "open-in-terminal" }, { kind: "copy", detail: "细节" }] });
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
    expect(frontView({ kind: "said", title: "t", body: "", tone: "plain", acts: [] })?.hint, "没带灰字 ⇒ 不出那一行").toBeNull();
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
