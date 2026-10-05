/**
 * 「会打断什么」一问（I12）：2 秒上限 · 什么都没有就不问 · 重复按并进同一个框 · 一族名字多了写 ×n。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { askWithin, confirmInterrupts, familyLine, INTERRUPTS_WITHIN_MS } from "../../../../src/frontend/ui/kit/interrupts";
import { decodeInterrupts } from "../../../../src/frontend/ui/interrupt-reads";
import { copyText } from "../../../../src/frontend/ui/copy-table";

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe("I12 会打断什么", () => {
  it("2 秒没答 ⇒ null（当有东西在跑）；答了 ⇒ 原样；抛 ⇒ null", async () => {
    const late = askWithin(() => new Promise(() => {}));
    vi.advanceTimersByTime(INTERRUPTS_WITHIN_MS);
    await expect(late).resolves.toBeNull();
    await expect(askWithin(async () => ({ families: [] }))).resolves.toEqual({ families: [] });
    await expect(askWithin(() => Promise.reject(new Error("x")))).resolves.toBeNull();
  });

  it("什么都没有 ⇒ 不问、直接做；有提醒 ⇒ 照问", async () => {
    const confirm = vi.fn().mockReturnValue(true);
    await expect(confirmInterrupts({ title: "t", action: "a", ask: async () => ({ families: [] }), confirm })).resolves.toBe(true);
    expect(confirm).not.toHaveBeenCalled();
    await confirmInterrupts({ title: "t2", action: "a", note: "未信任目录", ask: async () => ({ families: [] }), confirm });
    expect(confirm.mock.calls[0][0].body).toBe("未信任目录");
  });

  it("同一件事重复按：并进同一个框，不叠第二个", async () => {
    let answer!: (v: boolean) => void;
    const confirm = vi.fn(() => new Promise<boolean>((r) => (answer = r)));
    const ask = async () => ({ families: [{ family: "turn" as const, names: [] }] });
    const a = confirmInterrupts({ title: "退出 cc-monitor", action: "仍然退出", ask, confirm });
    const b = confirmInterrupts({ title: "退出 cc-monitor", action: "仍然退出", ask, confirm });
    expect(a).toBe(b);
    await vi.waitFor(() => expect(confirm).toHaveBeenCalledTimes(1));
    answer(true);
    await expect(b).resolves.toBe(true);
  });

  it("一族的字：没名字只写族名；三个以内列名字；再多写 ×n", () => {
    expect(familyLine("turn", [])).toBe(copyText("kit.interrupts.turn"));
    expect(familyLine("task", ["build", "lint"])).toBe(`${copyText("kit.interrupts.task")} build · lint`);
    expect(familyLine("agent", ["a", "b", "c", "d", "e"])).toBe(`${copyText("kit.interrupts.agent")} ×5`);
  });

  it("后端应答按形状收：认不得的族 / 名字不是串 ⇒ 抛（当没答）", () => {
    expect(decodeInterrupts({ families: [{ family: "agent", names: ["x"] }] })).toEqual({ families: [{ family: "agent", names: ["x"] }] });
    expect(() => decodeInterrupts({ families: [{ family: "edit", names: [] }] })).toThrow();
    expect(() => decodeInterrupts({ families: [{ family: "task", names: [1] }] })).toThrow();
    expect(() => decodeInterrupts({})).toThrow();
  });
});
