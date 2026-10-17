/**
 * 取首行预览不整条切（`format.ts::firstLineOf`）。
 *
 * 判据（异源：参照一侧是原式 `split("\n").find(非空).trim()` 逐字抄在本文件里，被判一侧是 `firstLineOf`）：
 * 在随机文本上逐字相等（行、是否超长两样都比）；大单行只切出前 max 个字。
 */
import { describe, it, expect } from "vitest";
import { firstLineOf } from "../../../../src/frontend/ui/format";

/** 决定性伪随机（mulberry32） */
function rng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const ALPHABET = ["a", "Z", "0", " ", "\t", "\n", "\r", " ", "　", "中", "😀", '"', "\\", "\u0001", " ", "é", "$"];

function randText(r: () => number, maxLen: number): string {
  const n = Math.floor(r() * maxLen);
  let s = "";
  for (let i = 0; i < n; i++) s += ALPHABET[Math.floor(r() * ALPHABET.length)];
  return s;
}

/** 改之前 `cards/index.ts::firstLinePreview` 取行的那两句（原式） */
function firstLineOld(text: string): string {
  return (text.split("\n").find((l) => l.trim().length > 0) ?? "").trim();
}

describe("firstLineOf == 原式 split/find/trim", () => {
  it("随机文本 × 三种 max：行与是否超长逐字相等", () => {
    const r = rng(20260925);
    let compared = 0;
    for (let i = 0; i < 4000; i++) {
      const text = randText(r, i % 50 === 0 ? 2000 : 80);
      const want = firstLineOld(text);
      for (const max of [1, 5, 60]) {
        const got = firstLineOf(text, max);
        expect(got.more, JSON.stringify(text)).toBe(want.length > max);
        expect(got.line, JSON.stringify(text)).toBe(got.more ? want.slice(0, max) : want);
        compared++;
      }
    }
    expect(compared).toBe(12000);
  });

  it("617 KB 单行：只切出前 max 个字", () => {
    const big = "  " + "x".repeat(617 * 1024) + "  ";
    const got = firstLineOf(big, 60);
    expect(got).toEqual({ line: "x".repeat(60), more: true });
  });
});
