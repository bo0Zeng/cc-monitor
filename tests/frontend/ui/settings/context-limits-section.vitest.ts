/**
 * 上下文上限表住**那台**后端（`prefs.json` 的 `contextLimits`，`history-facts` 在那台现读）；机器页那一折按 origin 问那台。
 *
 * 钉三件（期望值手写）：① 文本 ⇔ 覆盖表两向（读不懂的行说第几行、不交）；② 读回的是那台的那一张；
 * ③ 真交那台：有内容 ⇒ `prefs-set {contextLimits: 表}`，清空 ⇒ `{contextLimits: {}}`，交好广播让主窗口重问；那台读不懂 ⇒ 不许改、说那一句。
 */
import { describe, expect, it, vi, beforeEach } from "vitest";

const { sets, emitted, machines } = vi.hoisted(() => ({
  sets: [] as [string, unknown][],
  emitted: [] as string[],
  machines: new Map<string, { state: string; reason: string | null; limits: Record<string, number> }>(),
}));
vi.mock("../../../../src/frontend/ui/machine-prefs", () => ({
  readPrefs: async (origin: string) => {
    const m = machines.get(origin)!;
    return { state: m.state, reason: m.reason, detail: null, path: null, prefs: { launchers: {}, resumeInTmux: false, models: {}, contextLimits: { ...m.limits } } };
  },
  setPrefs: async (origin: string, patch: { contextLimits: Record<string, number> }) => {
    sets.push([origin, patch]);
    machines.get(origin)!.limits = patch.contextLimits;
    return { state: "ok", reason: null, detail: null, path: null, prefs: { launchers: {}, resumeInTmux: false, models: {}, contextLimits: patch.contextLimits } };
  },
}));
vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn(async (n: string) => void emitted.push(n)), listen: vi.fn() }));

import { ContextLimitsSection, limitsToText, textToLimits } from "../../../../src/frontend/ui/settings/context-limits-section";
import { copyText } from "../../../../src/frontend/ui/copy-table";

const flush = () => new Promise((r) => setTimeout(r, 0));

describe("那台的上下文上限表", () => {
  beforeEach(() => {
    sets.length = 0;
    emitted.length = 0;
    machines.clear();
    machines.set("devbox", { state: "ok", reason: null, limits: { "claude-sonnet-4-5": 1000000 } });
    machines.set("gpu", { state: "ok", reason: null, limits: {} });
  });

  it("文本 ⇔ 覆盖表：两向；读不懂的行说第几行", () => {
    expect(textToLimits("claude-sonnet-4-5 = 1000000\n\n  opus=200000 ")).toEqual({ ok: { "claude-sonnet-4-5": 1000000, opus: 200000 } });
    expect(limitsToText({ a: 1, b: 2 })).toBe("a = 1\nb = 2");
    for (const [t, n] of [["a = 1\nb = x", 2], ["= 5", 1], ["a = 0", 1], ["a = -3", 1], ["a", 1]] as const) {
      expect(textToLimits(t), t).toEqual({ badLine: n });
    }
  });

  it("★ 读回那台的 · 交那台 · 清空 · 读不懂的行不交", async () => {
    const s = new ContextLimitsSection("devbox");
    void s.load();
    await flush();
    const box = s.element.querySelector("textarea")!;
    expect(box.value).toBe("claude-sonnet-4-5 = 1000000");
    box.value = "x = 12";
    await s.save();
    box.value = "   ";
    await s.save();
    box.value = "nope";
    await s.save();
    expect(sets).toEqual([
      ["devbox", { contextLimits: { x: 12 } }],
      ["devbox", { contextLimits: {} }],
    ]);
    expect(emitted).toEqual(["settings-applied", "settings-applied"]);
    expect(s.element.textContent).toContain(copyText("contextLimits.editor.badLine", { n: 1 }));
  });

  it("折着：标题「高级：上下文上限」、右侧「已改 N」（N ＝ 覆盖几条，0 不写）；展开才见文本框；存了数跟着变；reveal() 展开", async () => {
    const s = new ContextLimitsSection("devbox");
    void s.load();
    await flush();
    const head = s.element.querySelector<HTMLElement>("[aria-expanded]")!;
    expect(head.getAttribute("aria-expanded")).toBe("false");
    expect(head.textContent).toContain(copyText("contextLimits.editor.title"));
    expect(head.textContent).toContain(copyText("contextLimits.editor.count", { n: 1 }));
    expect(s.element.querySelector("textarea")!.closest("[hidden]")).not.toBeNull();
    s.element.querySelector("textarea")!.value = "a = 1\nb = 2";
    await s.save();
    expect(head.textContent).toContain(copyText("contextLimits.editor.count", { n: 2 }));
    s.element.querySelector("textarea")!.value = "";
    await s.save();
    expect(head.textContent).not.toContain(copyText("contextLimits.editor.count", { n: 0 }));
    s.reveal();
    expect(head.getAttribute("aria-expanded")).toBe("true");
    expect(s.element.dataset.anchor, "主窗口［设上限］落到这一折").toBe("context-limits");
  });

  it("★ 每台一张：另一台读回的是它自己那一张，交也只交它", async () => {
    const s = new ContextLimitsSection("gpu");
    void s.load();
    await flush();
    const box = s.element.querySelector("textarea")!;
    expect(box.value).toBe("");
    box.value = "opus = 5";
    await s.save();
    expect(sets).toEqual([["gpu", { contextLimits: { opus: 5 } }]]);
    expect(machines.get("devbox")!.limits).toEqual({ "claude-sonnet-4-5": 1000000 });
  });

  it("★ 那台读不懂 ⇒ 文本框不许改、说那台写好的那一句、不交", async () => {
    machines.set("bad", { state: "unreadable", reason: "那份偏好读不懂-abc", limits: {} });
    const s = new ContextLimitsSection("bad");
    void s.load();
    await flush();
    const box = s.element.querySelector("textarea")!;
    expect(box.disabled).toBe(true);
    expect(s.element.textContent).toContain("那份偏好读不懂-abc");
    await s.save();
    expect(sets).toEqual([]);
  });
});
