// CP7：「复制给 agent」的文本必须自带住址、索引读数与 CP4 那一行。
// 纯函数 —— 被判的就是 `src/frontend/ui/panorama/agent-clip.ts` 本身，本文件**零 mock**。
// 判法全是**整串相等**：少一行、多一行、字改了都红（不是「包含某个词」）。
import { describe, it, expect } from "vitest";
import {
  clipForFile,
  clipForSymbol,
  stampLine,
  unseenLine,
  unsureLine,
  CLIP_HEAD,
  type ClipContext,
} from "../../../../src/frontend/ui/panorama/agent-clip";
import type { Edge, Symbol as PanoSymbol } from "../../../../src/frontend/ui/panorama/types";
import { copyText } from "../../../../src/frontend/ui/copy-table";

// 索引时间那一行与「看不见」缺值那一句按文案键取（判据按键、不钉原文）；其余照旧整串相等。
const ISO = "2026-09-21T14:13:20.000Z";
const STAMP = copyText("agentClip.stamp.line", { iso: ISO, indexedAt: 1_790_000_000, stale: "" });
const STAMP_STALE = copyText("agentClip.stamp.line", { iso: ISO, indexedAt: 1_790_000_000, stale: copyText("agentClip.stamp.stale") });

const sym = (over: Partial<PanoSymbol> = {}): PanoSymbol => ({
  id: "src/a.rs#f",
  name: "f",
  file: "src/a.rs",
  kind: "Function",
  lang: "Rust",
  start_line: 10,
  end_line: 20,
  signature: "fn f(x: u32) -> u32",
  ...over,
});
const edge = (from: string, to: string, confidence: Edge["confidence"], line: number | null): Edge => ({
  from,
  to,
  kind: "Calls",
  call_site_line: line,
  confidence,
});
const ctx: ClipContext = {
  repo: "/home/u/repo",
  stamp: { indexedAt: 1_790_000_000, stale: false },
  coverage: { unresolved_calls: 12, parse_errors: 1, total_files: 40, drawn_files: 9 },
};

describe("CP7 复制给 agent", () => {
  it("C1 符号级：整段逐行相等（住址 · 索引读数 · 看不见 · 分不清 · 边 · 批注）", () => {
    const text = clipForSymbol(
      ctx,
      sym(),
      [edge("src/b.rs#g", "src/a.rs#f", "Heuristic", 3)],
      [edge("src/a.rs#f", "src/c.rs#h", "Exact", 12), edge("src/a.rs#f", "x#y", "DynamicGuess", null)],
      [{ id: "1", file: "src/a.rs", symbol: "f", body: "这里要改", author: "me", status: "Active", origin: "Human" }],
    );
    expect(text.split("\n")).toEqual([
      CLIP_HEAD,
      "仓：/home/u/repo",
      "对象：符号 src/a.rs#f（Function · Rust · src/a.rs:10-20）",
      "签名：fn f(x: u32) -> u32",
      STAMP,
      "看不见：全仓 12 处调用未解析 · 1 个文件解析失败 · 全景图只画了 9/40 个文件",
      "分不清：本符号 3 条直接边里 2 条按名字凑（启发 1 · 动态猜测 1），动态派发 0 条，确定 1 条",
      "调用了（callees，2）：",
      "  - src/c.rs#h  [Exact]  L12",
      "  - x#y  [DynamicGuess]",
      "被调用（callers，1）：",
      "  - src/b.rs#g  [Heuristic]  L3",
      "生效批注（1）：",
      "  - me：这里要改",
    ]);
  });

  it("C2 文件级：住址带子系统，分不清那一行明说文件级没有边", () => {
    const text = clipForFile(
      ctx,
      { file: "src/a.rs", subsystem: "core", symbols: 2, isEntry: true },
      [sym(), sym({ id: "src/a.rs#g", name: "g", start_line: 30 })],
    );
    expect(text.split("\n")).toEqual([
      CLIP_HEAD,
      "仓：/home/u/repo",
      "对象：文件 src/a.rs（子系统「core」· 2 个符号 · 含入口点）",
      STAMP,
      "看不见：全仓 12 处调用未解析 · 1 个文件解析失败 · 全景图只画了 9/40 个文件",
      "分不清：文件级没有边 —— 边的确定度要点进符号那一级看",
      "符号（2）：",
      "  - src/a.rs#f  Function  L10",
      "  - src/a.rs#g  Function  L30",
    ]);
  });

  it("C3 读数缺了不许省掉那一行 —— 如实说「未取到 / 时效未知 / 陈旧」", () => {
    expect(stampLine(null)).toBe(copyText("agentClip.stamp.failed"));
    expect(stampLine({ indexedAt: null, stale: false })).toBe(copyText("agentClip.stamp.none"));
    expect(stampLine({ indexedAt: 1_790_000_000, stale: true })).toBe(STAMP_STALE);
    expect(unseenLine(null)).toBe(copyText("agentClip.unseen.unknown"));
    expect(unsureLine([], [])).toBe(
      "分不清：本符号 0 条直接边里 0 条按名字凑（启发 0 · 动态猜测 0），动态派发 0 条，确定 0 条",
    );
  });

  it("C4 反空真：任何缺值都不许漏成 undefined / NaN / null 字样（零命中）", () => {
    const bare: ClipContext = { repo: "/r", stamp: null, coverage: null };
    const texts = [
      clipForSymbol(bare, sym({ signature: null }), [], [], []),
      clipForFile(bare, { file: "a", subsystem: "s", symbols: 0, isEntry: false }, []),
      clipForSymbol(ctx, sym({ signature: undefined }), [], [], []),
    ];
    const hits = texts.flatMap((t) => t.match(/undefined|NaN|null/g) ?? []);
    expect(hits).toEqual([]);
    // 分母：三段都真的拼出来了，而且每段都带上了 CP4 两行。
    expect(texts.map((t) => t.split("\n").filter((l) => /^(看不见|分不清)：/.test(l)).length)).toEqual([
      2, 2, 2,
    ]);
  });
});
