/**
 * 「需手动」的账（`needs-book.ts`）什么时候重问：收到 `needs` · 那台接上 · 一问没问到之后自己退避再问。
 * 没问到 ⇒ 旧的一份照留、过一会儿自己再问（1s 起、翻倍、封顶 30s）；问到了退避归零。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { NeedsBook } from "../../../src/frontend/ui/needs-book";
import type { NeedsMerged } from "../../../src/frontend/ui/generated/NeedsMerged";
import type { NeedsColumn } from "../../../src/frontend/ui/generated/NeedsColumn";
import { LOCAL } from "../../../src/frontend/ui/ipc/origin";
import type { NeedsAt } from "../../../src/frontend/ui/generated/NeedsAt";

/** 一条在等的会话（簿只读 kind · session.sid）。 */
const at = (sid: string): NeedsAt => ({ origin: LOCAL, item: { kind: "session", rank: 1, session: { sid } } }) as unknown as NeedsAt;
const plan = (slice: string): NeedsAt => ({ origin: LOCAL, item: { kind: "plan", rank: 9, workspace: "/w", slice } });

const NONE: NeedsColumn = { items: [], total: 0, head: "", sure: true };
const EMPTY: NeedsMerged = { asks: NONE, manual: NONE, unreadable: [], down: [], ended: [], windowTitle: "" };

describe("NeedsBook 读失败退避再问", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("第一次没问到 ⇒ 不用谁再叫，过一会儿自己又问；问到了就不再自己问", async () => {
    let calls = 0;
    const merge = vi.fn(async (): Promise<NeedsMerged> => {
      calls += 1;
      if (calls === 1) throw new Error("本机后端没合成");
      return { ...EMPTY, manual: { ...NONE, total: 2 } };
    });
    const book = new NeedsBook(merge);
    await book.again();
    expect(merge).toHaveBeenCalledTimes(1);
    expect(book.total("manual")).toBe(0);
    await vi.advanceTimersByTimeAsync(1_000);
    expect(merge).toHaveBeenCalledTimes(2);
    expect(book.total("manual")).toBe(2);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(merge).toHaveBeenCalledTimes(2);
  });

  it("一直没问到 ⇒ 间隔翻倍、封顶 30s", async () => {
    const merge = vi.fn(async (): Promise<NeedsMerged> => {
      throw new Error("本机后端没合成");
    });
    const book = new NeedsBook(merge);
    await book.again();
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

describe("NeedsBook 读核心合好的两列", () => {
  it("★ 读口照列分：待答请求那一列只出会话（计划 · 会外 · 会外读不出都空）；需手动那一列会话 · 计划 · 会外读不出各一格；哪条会话在哪一列（命令面板分组照它）；数 · 头那一句 · 说没说准各照那一列，不两列拼", async () => {
    const merged: NeedsMerged = {
      ...EMPTY,
      asks: { items: [at("可答")], total: 1, head: "核心·待答请求 1", sure: true },
      manual: { items: [at("不可答"), plan("片")], total: 2, head: "核心·需手动", sure: false },
      unreadable: [{ origin: "gpu-01", line: "核心·读不出那一行", said: "核心·那台那一句" }],
    };
    const book = new NeedsBook(() => Promise.resolve(merged));
    await book.again();
    const a = book.facts("answerable");
    expect([a.sessions.map((s) => s.session.sid), a.plans, a.manual, a.unreadable]).toEqual([["可答"], [], [], []]);
    const m = book.facts("manual");
    expect([m.sessions.map((s) => s.session.sid), m.plans.map((p) => p.slice), m.unreadable.map((u) => u.line)]).toEqual([["不可答"], ["片"], ["核心·读不出那一行"]]);
    expect(book.plans().map((p) => p.slice)).toEqual(["片"]);
    expect([book.columnOf("可答"), book.columnOf("不可答"), book.columnOf("不在账上")]).toEqual(["answerable", "manual", null]);
    expect([book.total("answerable"), book.head("answerable"), book.sure("answerable")]).toEqual([1, "核心·待答请求 1", true]);
    expect([book.total("manual"), book.head("manual"), book.sure("manual")]).toEqual([2, "核心·需手动", false]);
  });
});

describe("NeedsBook 窗口标题", () => {
  it("第一份合好之前不给标题（不由前端另写一份基底），合到了照抄核心那一句", async () => {
    const book = new NeedsBook(() => Promise.resolve({ ...EMPTY, windowTitle: "核心·标题" }));
    expect(book.windowTitle()).toBeNull();
    await book.again();
    expect(book.windowTitle()).toBe("核心·标题");
  });
});
