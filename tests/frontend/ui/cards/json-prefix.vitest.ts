/**
 * 〔W5-RENDER R2〕工具卡预览的两处「整条切 / 整条序列化只为取 ≤60 字」收成有界的两个函数。
 *
 * 守的要求（`设计/17 §2.5`，逐字）：「三处『整条切 / 整条序列化只为取 ≤60 字』：`cards/index.ts::firstLinePreview`
 * · `summarizeInput` · `trackAgents` 取首行」—— 修法「`indexOf("\n")` ＋ `slice`；只序列化头几个 key；
 * 顺手抽一个 `firstLineOf(s, max)` 消掉三个住址」。第三处（`trackAgents`）住 STC 的写区，本文件不管。
 *
 * 判据（异源：参照一侧是**改之前的原式**逐字抄在本文件里，被判一侧是 `src/frontend/ui/format.ts` 的两个新函数）：
 * - `firstLineOf` 与原式 `split("\n").find(非空).trim()` 在随机文本上逐字相等（行、是否超长两样都比）；
 * - `jsonPrefix` 是 `JSON.stringify` 的前缀、「超没超 limit」与完整序列化一致；拼回调用方的样子
 *   `truncate(…, 60)` 与原式 `truncate(JSON.stringify(v), 60)` 逐字相等；
 * - 「只序列化头几个 key」不看墙钟：数 Proxy 上被读的值的个数，远小于键数。
 */
import { describe, it, expect } from "vitest";
import { firstLineOf, jsonPrefix } from "../../../../src/frontend/ui/format";

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

function randValue(r: () => number, depth: number): unknown {
  const k = Math.floor(r() * (depth > 3 ? 6 : 9));
  switch (k) {
    case 0:
      return null;
    case 1:
      return r() < 0.5;
    case 2:
      return Math.floor((r() - 0.5) * 1e6) / (r() < 0.5 ? 1 : 7);
    case 3:
    case 4:
      return randText(r, r() < 0.2 ? 400 : 40);
    case 5:
      return r() < 0.1 ? undefined : "x";
    case 6:
    case 7: {
      const o: Record<string, unknown> = {};
      const n = Math.floor(r() * 4);
      for (let i = 0; i < n; i++) o[r() < 0.2 ? String(Math.floor(r() * 10)) : randText(r, 12)] = randValue(r, depth + 1);
      return o;
    }
    default: {
      const n = Math.floor(r() * 4);
      return Array.from({ length: n }, () => (r() < 0.1 ? undefined : randValue(r, depth + 1)));
    }
  }
}

/** 改之前 `cards/index.ts::firstLinePreview` 取行的那两句（原式） */
function firstLineOld(text: string): string {
  return (text.split("\n").find((l) => l.trim().length > 0) ?? "").trim();
}

/** 改之前 `cards/index.ts::truncate`（只比串本身，省略号那一截由文案表出，与本判据无关） */
function truncateOld(s: string, n: number): string {
  return s.length > n ? `${s.slice(0, n)}…` : s;
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

describe("jsonPrefix：JSON.stringify 的前缀，拼回调用方与原式逐字相等", () => {
  it("随机值 × 三种 limit", () => {
    const r = rng(7);
    let compared = 0;
    for (let i = 0; i < 4000; i++) {
      const v = randValue(r, 0);
      const full = JSON.stringify(v);
      for (const limit of [0, 10, 60]) {
        const p = jsonPrefix(v, limit);
        if (full === undefined) {
          expect(p).toBeUndefined();
          continue;
        }
        const s = p as string;
        const ok =
          typeof p === "string" &&
          full.startsWith(s.slice(0, limit)) &&
          s.length > limit === full.length > limit &&
          (full.length > limit || s === full) &&
          truncateOld(s, limit) === truncateOld(full, limit);
        if (!ok) expect({ limit, full: full.slice(0, 200), got: String(p).slice(0, 200) }).toBe("一致");
        compared++;
      }
    }
    expect(compared).toBeGreaterThan(9000);
  }, 30_000);

  it("切口劈开代理对：前 60 个字仍与原式相同", () => {
    for (let pad = 0; pad < 70; pad++) {
      const v = { k: "a".repeat(pad) + "😀".repeat(40) };
      expect(truncateOld(jsonPrefix(v, 60) as string, 60)).toBe(truncateOld(JSON.stringify(v), 60));
    }
  });

  it("环：与 JSON.stringify 一样抛 TypeError", () => {
    const o: Record<string, unknown> = { a: 1 };
    o.self = o;
    expect(() => JSON.stringify(o)).toThrow(TypeError);
    expect(() => jsonPrefix(o, 1e9)).toThrow(TypeError);
  });

  it("只序列化头几个 key：1000 个键的对象只读了几个值（不看墙钟）", () => {
    const target: Record<string, string> = {};
    for (let i = 0; i < 1000; i++) target[`key${i}`] = "v".repeat(100);
    let reads = 0;
    const p = new Proxy(target, {
      get(t, k) {
        reads++;
        return (t as Record<string | symbol, unknown>)[k];
      },
    });
    const s = jsonPrefix(p, 60) as string;
    expect(s.startsWith('{"key0":"vvv')).toBe(true);
    // `toJSON` 探一次 ＋ 第一个值；够 61 个字就停
    expect(reads).toBeLessThanOrEqual(3);
    // 正控：原式把 1000 个值全读了
    reads = 0;
    JSON.stringify(p);
    expect(reads).toBeGreaterThanOrEqual(1000);
  });
});
