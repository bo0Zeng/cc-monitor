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
  /** 限用词的认法正则（不是 `scan`）：文案表里命中它的每一条都要在自己的 `limited` 里写明。 */
  tally?: { re: string; flags: string };
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
  /** 界面角色（按钮 · 菜单项 · 标题 …），闭集与它许配的 kind 住 `copy-table.vitest.ts::ROLE_KINDS`。 */
  role?: string;
  zh: string;
  args: string[];
  waive?: Record<string, string>;
  /** 这一条用到了哪几个限用词（写的人对着 `terms.json` 的 context 看过语境）。 */
  limited?: string[];
}
export type Table = Record<string, Entry>;

/** 生产代码里一处取文调用：在哪 · 哪条 · 给了哪几个参数（Rust 那一侧另记哪几个参数喂的是原话，见 `rust-refs.ts`）。 */
export interface Ref {
  file: string;
  key: string;
  args: string[];
  /** 值是下层原话（`e.to_string()` · `format!("{e}")`）的那几个参数名（只 Rust 那一侧记）。 */
  raw?: string[];
}

/** 一条文案命中了哪几个限用词（对 `speech()` 跑：占位符名不算文字）。 */
export function limitedHits(terms: Term[], zh: string): string[] {
  return terms
    .filter((t) => t.tier === "限用" && t.tally)
    .filter((t) => new RegExp(t.tally!.re, t.tally!.flags.replace(/g/g, "")).test(speech(zh)))
    .map((t) => t.word)
    .sort();
}

/** 限用词声明对不上的条目：扫出来的 ≠ 它自己 `limited` 里写的（两向），或写了一个不是限用词的名字。 */
export function limitedMismatches(terms: Term[], table: Table): string[] {
  const limited = new Set(terms.filter((t) => t.tier === "限用").map((t) => t.word));
  const out: string[] = [];
  for (const [key, e] of Object.entries(table)) {
    const got = limitedHits(terms, e.zh);
    const said = [...(e.limited ?? [])].sort();
    for (const w of said) if (!limited.has(w)) out.push(`${key}：limited 里的「${w}」不是限用词`);
    const miss = got.filter((w) => !said.includes(w));
    const extra = said.filter((w) => limited.has(w) && !got.includes(w));
    if (miss.length) out.push(`${key}：用了限用词 ${miss.join("、")}，limited 里没写`);
    if (extra.length) out.push(`${key}：limited 写了 ${extra.join("、")}，这句话里其实没有`);
  }
  return out;
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
