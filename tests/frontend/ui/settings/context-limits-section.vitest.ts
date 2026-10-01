/**
 * （主会话 09-28 裁 FIX4）「`contextLimits`：设置页高级段露出来（可读可改；每个配置键要么有入口要么删）」。
 *
 * 钉三件（期望值手写）：① 文本 ⇔ 覆盖表两向（读不懂的行说第几行、不写盘）；② 读回与状态栏同一份（坏值丢掉）；
 * ③ 真写盘：有内容 ⇒ `set contextLimits`，清空 ⇒ `remove contextLimits`，写好广播让主窗口重读。
 */
import { describe, expect, it, vi, beforeEach } from "vitest";

const { edits, emitted } = vi.hoisted(() => ({ edits: [] as unknown[], emitted: [] as string[] }));
vi.mock("../../../../src/frontend/ui/ipc/commands", () => ({
  commands: {
    load_config: vi.fn(async () => ({ contextLimits: { "claude-sonnet-4-5": 1000000, bad: -1, "": 5 } })),
    patch_config: vi.fn(async (a: { edits: unknown[] }) => {
      edits.push(...a.edits);
    }),
  },
}));
vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn(async (n: string) => void emitted.push(n)), listen: vi.fn() }));

import { ContextLimitsSection, limitsToText, textToLimits } from "../../../../src/frontend/ui/settings/context-limits-section";
import { readContextLimits } from "../../../../src/frontend/ui/views/context-limit";

const flush = () => new Promise((r) => setTimeout(r, 0));

describe("FIX4 · contextLimits 的入口", () => {
  beforeEach(() => {
    edits.length = 0;
    emitted.length = 0;
  });

  it("文本 ⇔ 覆盖表：两向；读不懂的行说第几行", () => {
    expect(textToLimits("claude-sonnet-4-5 = 1000000\n\n  opus=200000 ")).toEqual({ ok: { "claude-sonnet-4-5": 1000000, opus: 200000 } });
    expect(limitsToText({ a: 1, b: 2 })).toBe("a = 1\nb = 2");
    for (const [t, n] of [["a = 1\nb = x", 2], ["= 5", 1], ["a = 0", 1], ["a = -3", 1], ["a", 1]] as const) {
      expect(textToLimits(t), t).toEqual({ badLine: n });
    }
    expect(readContextLimits({ good: 7, bad: -1, "": 5, str: "9" })).toEqual({ good: 7 });
    expect(readContextLimits([1])).toEqual({});
  });

  it("读回 · 写盘 · 清空 · 读不懂不写", async () => {
    const s = new ContextLimitsSection();
    await flush();
    const box = s.element.querySelector("textarea")!;
    expect(box.value).toBe("claude-sonnet-4-5 = 1000000");
    box.value = "x = 12";
    await s.save();
    box.value = "   ";
    await s.save();
    box.value = "nope";
    await s.save();
    expect(edits).toEqual([
      { op: "set", path: ["contextLimits"], value: { x: 12 } },
      { op: "remove", path: ["contextLimits"] },
    ]);
    expect(emitted).toEqual(["settings-applied", "settings-applied"]);
    expect(s.element.textContent).toContain("第 1 行读不懂");
  });
});
