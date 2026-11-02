/**
 * 「需手动」的账（`needs-book.ts`）什么时候重问：收到 `needs` · 那台接上 · 一问没问到之后自己退避再问。
 * 没问到 ⇒ 旧的一份照留、过一会儿自己再问（1s 起、翻倍、封顶 30s）；问到了退避归零。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { NeedsBook } from "../../../src/frontend/ui/needs-book";
import { LOCAL } from "../../../src/frontend/ui/ipc/origin";
import type { NeedsMerged } from "../../../src/frontend/ui/generated/NeedsMerged";

const EMPTY: NeedsMerged = { items: [], total: 0, unreadable: [], down: [] };

describe("NeedsBook 读失败退避再问", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("第一次没问到 ⇒ 不用谁再叫，过一会儿自己又问；问到了就不再自己问", async () => {
    let calls = 0;
    const merge = vi.fn(async (): Promise<NeedsMerged> => {
      calls += 1;
      if (calls === 1) throw new Error("本机后端没合成");
      return { ...EMPTY, total: 2 };
    });
    const book = new NeedsBook(merge);
    await book.refresh(LOCAL);
    expect(merge).toHaveBeenCalledTimes(1);
    expect(book.total()).toBe(0);
    await vi.advanceTimersByTimeAsync(1_000);
    expect(merge).toHaveBeenCalledTimes(2);
    expect(book.total()).toBe(2);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(merge).toHaveBeenCalledTimes(2);
  });

  it("一直没问到 ⇒ 间隔翻倍、封顶 30s", async () => {
    const merge = vi.fn(async (): Promise<NeedsMerged> => {
      throw new Error("本机后端没合成");
    });
    const book = new NeedsBook(merge);
    await book.refresh(LOCAL);
    const at: number[] = [];
    let t = 0;
    for (let i = 0; i < 8; i++) {
      const before = merge.mock.calls.length;
      while (merge.mock.calls.length === before && t < 200_000) {
        await vi.advanceTimersByTimeAsync(500);
        t += 500;
      }
      at.push(t);
    }
    const gaps = at.map((x, i) => x - (i === 0 ? 0 : at[i - 1]));
    expect(gaps).toEqual([1_000, 2_000, 4_000, 8_000, 16_000, 30_000, 30_000, 30_000]);
  });
});
