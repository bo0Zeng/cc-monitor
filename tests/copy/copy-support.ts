/**
 * CP2a（抽表 · `§5.2④` 术语表）：术语表与文案表的**读法**，两个判据文件共用一份。
 *
 * 只读 `src/shared/copy/` 下两份 JSON；不遍历目录（遍历住 `test-support/production-sources.ts`）。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { REPO_ROOT } from "../test-support/repo-root.ts";

export const TERMS_PATH = resolve(REPO_ROOT, "src", "shared", "copy", "terms.json");
export const TABLE_PATH = resolve(REPO_ROOT, "src", "shared", "copy", "table.json");

export type Tier = "正名" | "限用" | "禁";

export interface Term {
  word: string;
  tier: Tier;
  meaning?: string;
  context?: string;
  say?: string;
  points?: string;
  scan?: { re: string; flags: string } | null;
  noScanWhy?: string;
  examples?: string[];
  counterExamples?: string[];
  census?: string;
  ledger?: string[];
  source?: string;
  note?: string;
  ask?: string;
  /** 限用词计数棘轮：计数正则（不是 `scan`）与文案表里命中它的条数（「计数只许变少」）。 */
  tally?: { re: string; flags: string };
  inTable?: number;
}

export function loadTerms(path = TERMS_PATH): Term[] {
  const doc = JSON.parse(readFileSync(path, "utf8")) as { terms?: Term[] };
  return doc.terms ?? [];
}

/** 一条禁档词编成的扫描器：`null` = 这一条只靠人工 review。 */
export function scannerOf(t: Term): RegExp | null {
  if (!t.scan) return null;
  // 不带 g：`test()` 在带 g 的正则上有 lastIndex 状态，同一个正则连测两串会漏。
  return new RegExp(t.scan.re, t.scan.flags.replace(/g/g, ""));
}

/** R1：一段文本命中了哪些禁档词（只算有 scan 的那些）。 */
export function r1Hits(text: string, terms: Term[]): string[] {
  const out: string[] = [];
  for (const t of terms) {
    if (t.tier !== "禁") continue;
    const rx = scannerOf(t);
    if (rx && rx.test(text)) out.push(t.word);
  }
  return out;
}

/** 普查量具里一张词表的名字（`("名字", re.compile(...))` 那一列）。 */
export function censusList(py: string, name: string): string[] {
  const start = py.indexOf(`\n${name} = [`);
  if (start < 0) return [];
  const end = py.indexOf("\n]", start);
  const body = py.slice(start, end < 0 ? undefined : end);
  return [...body.matchAll(/^\s*\("([^"]+)",\s*re\.compile\(/gm)].map((m) => m[1]);
}

export interface Entry {
  kind: string;
  zh: string;
  args: string[];
  waive?: Record<string, string>;
}
export type Table = Record<string, Entry>;

/** 限用词在文案表里的命中条数（对 `speech()` 跑：占位符名不算文字）。 */
export function tallyOf(t: Term, table: Table): number {
  if (!t.tally) return 0;
  const rx = new RegExp(t.tally.re, t.tally.flags.replace(/g/g, ""));
  return Object.values(table).filter((e) => rx.test(speech(e.zh))).length;
}

export function loadTable(path = TABLE_PATH): Table {
  return (JSON.parse(readFileSync(path, "utf8")) as { entries?: Table }).entries ?? {};
}

/** 具名占位符。 */
export const NAMED_PH = /\{([A-Za-z][A-Za-z0-9]*)\}/g;

/** 剥掉插值点之后的「人话」—— 规矩都对它跑（占位符名是标识符，不上界面）。 */
export function speech(zh: string): string {
  return zh.replace(NAMED_PH, "");
}
